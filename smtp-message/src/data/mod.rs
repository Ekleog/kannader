use nom::crlf;
use std::io;
use tokio::prelude::*;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct DataCommand {
    _useless: (),
}

impl DataCommand {
    // SMTP-escapes (ie. doubles leading ‘.’) messages first
    pub fn new() -> DataCommand {
        DataCommand { _useless: () }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"DATA\r\n")
    }
}

named!(pub command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    (DataCommand { _useless: () })
));

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DataStreamState {
    Start,
    CrPassed,
    CrLfPassed,
    WaitingToSendDot,
    WaitingToSendDotCr,
    NeedsToSend(u8),
}

pub struct DataStream<S: Stream<Item = u8>> {
    source: S,
    state:  DataStreamState,
}

impl<S: Stream<Item = u8>> DataStream<S> {
    pub fn new(source: S) -> DataStream<S> {
        DataStream {
            source,
            state: DataStreamState::CrLfPassed,
        }
    }

    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S: Stream<Item = u8>> Stream for DataStream<S> {
    type Item = u8;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        use self::{Async::*, DataStreamState::*};
        // First, handle when we're behind sending some stuff
        match self.state {
            NeedsToSend(c) => {
                self.state = if c == b'\r' { CrPassed } else { Start };
                return Ok(Ready(Some(c)));
            }
            _ => (),
        }
        loop {
            let res = self.source.poll()?;
            match res {
                NotReady => return Ok(NotReady),
                Ready(None) => return Ok(Ready(None)),
                Ready(Some(c)) => match (self.state, c) {
                    // Then, we were waiting to send something
                    (WaitingToSendDot, b'\r') => {
                        self.state = WaitingToSendDotCr;
                        // Do not send the .\r (yet)
                    }
                    (WaitingToSendDotCr, b'\n') => {
                        return Ok(Ready(None));
                        // Just reached end-of-stream, we were right not to send the .\r
                    }
                    (WaitingToSendDot, c) => {
                        // Found "\r\n." + c, already sent "\r\n", drop the leading transparency .
                        self.state = if c == b'\r' { CrPassed } else { Start };
                        return Ok(Ready(Some(c)));
                    }
                    (WaitingToSendDotCr, c) => {
                        // Found "\r\n.\r" + c, already sent "\r\n", drop the transparency .
                        self.state = NeedsToSend(c);
                        return Ok(Ready(Some(b'\r')));
                    }
                    // Then, if all was normal up until now, move forward in the state
                    (_, b'\r') => {
                        self.state = CrPassed;
                        return Ok(Ready(Some(c)));
                    }
                    (CrPassed, b'\n') => {
                        self.state = CrLfPassed;
                        return Ok(Ready(Some(c)));
                    }
                    (CrLfPassed, b'.') => {
                        self.state = WaitingToSendDot;
                        // Do not send the leading dot (yet)
                    }
                    // Finally, just not move forward and send in the stuff
                    (_, _) => {
                        self.state = Start;
                        return Ok(Ready(Some(c)));
                    }
                },
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DataSinkState {
    Start,
    CrPassed,
    CrLfPassed,
    NeedsToSendDot,
}

pub struct DataSink<S: Sink<SinkItem = u8>> {
    sink:  S,
    state: DataSinkState,
}

impl<S: Sink<SinkItem = u8>> DataSink<S> {
    pub fn new(sink: S) -> DataSink<S> {
        DataSink {
            sink,
            state: DataSinkState::CrLfPassed,
        }
    }

    pub fn into_inner(self) -> S {
        self.sink
    }

    pub fn end(self) -> DataSinkFuture<S> {
        use self::DataSinkState::*;
        match self.state {
            Start => DataSinkFuture::new(self.into_inner(), b"\r\n.\r\n"),
            CrPassed => DataSinkFuture::new(self.into_inner(), b"\r\n.\r\n"),
            CrLfPassed => DataSinkFuture::new(self.into_inner(), b".\r\n"),
            NeedsToSendDot => DataSinkFuture::new(self.into_inner(), b".\r\n.\r\n"),
        }
    }
}

impl<S: Sink<SinkItem = u8>> Sink for DataSink<S> {
    type SinkItem = u8;
    type SinkError = S::SinkError;

    fn start_send(&mut self, item: u8) -> Result<AsyncSink<u8>, Self::SinkError> {
        use self::DataSinkState::*;
        if self.state == NeedsToSendDot {
            if self.sink.start_send(b'.')?.is_not_ready() {
                return Ok(AsyncSink::NotReady(item));
            }
            self.state = Start;
        }
        if self.sink.start_send(item)?.is_not_ready() {
            return Ok(AsyncSink::NotReady(item));
        }
        match (self.state, item) {
            (_, b'\r') => {
                self.state = CrPassed;
            }
            (CrPassed, b'\n') => {
                self.state = CrLfPassed;
            }
            (CrLfPassed, b'.') => {
                self.state = NeedsToSendDot;
            }
            (_, _) => {
                self.state = Start;
            }
        }
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        if self.state == DataSinkState::NeedsToSendDot {
            if self.sink.start_send(b'.')?.is_not_ready() {
                return Ok(Async::NotReady);
            }
            self.state = DataSinkState::Start;
        }
        self.sink.poll_complete()
    }
}

pub struct DataSinkFuture<S: Sink<SinkItem = u8>> {
    sink: Option<S>,
    data: &'static [u8],
}

impl<S: Sink<SinkItem = u8>> DataSinkFuture<S> {
    fn new(sink: S, data: &'static [u8]) -> DataSinkFuture<S> {
        DataSinkFuture {
            sink: Some(sink),
            data,
        }
    }
}

impl<S: Sink<SinkItem = u8>> Future for DataSinkFuture<S> {
    type Item = S;
    type Error = S::SinkError;

    fn poll(&mut self) -> Result<Async<S>, S::SinkError> {
        use self::Async::*;
        loop {
            if self.data.is_empty() {
                return Ok(Ready(self.sink.take().unwrap()));
            }
            let send_char = self.data[0];
            if self.sink
                .as_mut()
                .map(|x| Ok(x.start_send(send_char)?.is_not_ready()))
                .unwrap()?
            {
                return Ok(NotReady);
            }
            self.data = &self.data[1..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_data_args() {
        let tests = vec![&b" \t  \t \r\n"[..], &b"\r\n"[..]];
        for test in tests.into_iter() {
            assert_eq!(
                command_data_args(test),
                IResult::Done(&b""[..], DataCommand { _useless: () })
            );
        }
    }

    #[test]
    fn valid_command_data_build() {
        let mut v = Vec::new();
        DataCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"DATA\r\n");
    }

    #[test]
    fn valid_data_stream() {
        let tests: &[(&[u8], &[u8], &[u8])] = &[
            (b"foo bar\r\n.\r\n", b"foo bar\r\n", b""),
            (b"\r\n.\r\n\r\n", b"\r\n", b"\r\n"),
            (b".baz\r\n.\r\nfoo", b"baz\r\n", b"foo"),
            (b" .baz\r\n.\r\nfoo", b" .baz\r\n", b"foo"),
            (b".\r\nMAIL FROM", b"", b"MAIL FROM"),
        ];
        for &(inp, out, rem) in tests {
            use helpers::SmtpString;
            println!(
                "Trying to parse {:?} into {:?} with {:?} remaining",
                SmtpString::copy_bytes(inp),
                SmtpString::copy_bytes(out),
                SmtpString::copy_bytes(rem)
            );
            let mut stream = DataStream::new(stream::iter_ok(inp.iter().cloned()).map_err(|()| ()));
            let output = stream.by_ref().collect().wait().unwrap();
            println!("Now computing remaining stuff");
            let remaining = stream.into_inner().collect().wait().unwrap();
            println!(
                " -> Got {:?} with {:?} remaining",
                SmtpString::copy_bytes(&output),
                SmtpString::copy_bytes(&remaining)
            );
            assert_eq!(output, out.to_vec());
            assert_eq!(remaining, rem.to_vec());
        }
    }

    #[test]
    fn valid_data_sink() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"foo bar", b"foo bar\r\n.\r\n"),
            (b"", b".\r\n"),
            (b".", b"..\r\n.\r\n"),
            (b"foo\r", b"foo\r\r\n.\r\n"),
            (b"foo bar\r\n", b"foo bar\r\n.\r\n"),
        ];
        for &(inp, out) in tests {
            let mut v = Vec::new();
            {
                let sink = DataSink::new(&mut v);
                sink.send_all(stream::iter_ok(inp.iter().cloned()))
                    .wait()
                    .unwrap()
                    .0
                    .end()
                    .wait()
                    .unwrap();
            }
            assert_eq!(v, out.to_vec());
        }
    }
}
