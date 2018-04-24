use bytes::BytesMut;
use nom::crlf;
use std::io;
use tokio::prelude::*;

use helpers::*;
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

    pub fn take_ownership(self) -> DataCommand {
        self
    }
}

named!(pub command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    (DataCommand { _useless: () })
));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DataStreamState {
    Running,
    CrPassed,
    CrLfPassed,
    Finished,
}

// Stream adapter that takes as input a stream that yields ByteMut elements,
// and outputs data unescaped as per RFC5321.
//
// The input stream should start giving elements since just after DATA\r\n, and
// will be consumed until the \r\n.\r\n sequence is found (or .\r\n if these
// are the first three bytes). The DataStream will output the unescaped data
// (ie. replacing \r\n. with \r\n when not in the \r\n.\r\n sequence) up to and
// including the first \r\n in \r\n.\r\n.
//
// Once the \r\n.\r\n sequence will have been reached, this stream will be
// terminated. At this point (and *not* before, as that would panic), please
// call `into_inner` to recover the original stream, advanced until after the
// \r\n.\r\n sequence.
//
// In order to handle the case of a packet received that doesn't end exactly
// after the \r\n.\r\n, the received stream must be Prependable, so that the
// additional data can be pushed back into it if need be.
pub struct DataStream<S: Stream<Item = BytesMut>> {
    source: Prependable<S>,
    // state is the state of the state machine at the BEGINNING of `buf`
    state: DataStreamState,
    buf:   BytesMut,
}

impl<S: Stream<Item = BytesMut>> DataStream<S> {
    pub fn new(source: Prependable<S>) -> DataStream<S> {
        DataStream {
            source,
            state: DataStreamState::CrLfPassed,
            buf: BytesMut::new(),
        }
    }

    // Beware: this will panic if it hasn't been fully consumed.
    pub fn into_inner(mut self) -> Prependable<S> {
        assert_eq!(self.state, DataStreamState::Finished);
        if !self.buf.is_empty() {
            // If this `unwrap` fails, this means that somehow:
            //  1. The stream passed to `new` was already prepended
            //  2. Somehow the state managed to go into `Finished` and the buffer has been
            //     filled without ever pulling a single element from the stream
            // So, quite obviously, that'd be a programming error from here, so let's just
            // unwrap
            self.source.prepend(self.buf).unwrap();
        }
        self.source
    }
}

// TODO: specifically fuzz DataStream, making sure it is equivalent to a
// naively-written version or to the opposite of Sink
impl<S: Stream<Item = BytesMut, Error = ()>> Stream for DataStream<S> {
    type Item = BytesMut;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        use self::{Async::*, DataStreamState::*};
        // First, handle the case when we're done
        if self.state == Finished {
            return Ok(Ready(None));
        }
        loop {
            // Figure out what to send from the current buf
            #[derive(Debug, Eq, PartialEq)]
            enum BufSplit {
                Nowhere,        // Should send the whole buffer as a result
                Eof(usize),     // Should send [arg] bytes as a result, then skip .\r\n and EOF
                Escape(usize),  // Should send [arg] bytes as a result, then skip a dot
                Unknown(usize), // Should send [arg] bytes as a result, then wait for more data
            }
            let mut split = BufSplit::Nowhere;

            // First, look at all that's in the buffe rexcept for the last 2 characters
            for (idx, w) in self.buf.windows(3).enumerate() {
                match (self.state, w[0]) {
                    // Move forward in the \r\n state machine
                    (_, b'\r') => self.state = CrPassed,
                    (CrPassed, b'\n') => self.state = CrLfPassed,

                    // If there is a \r\n., what should we do?
                    (CrLfPassed, b'.') if w == b".\r\n" => {
                        split = BufSplit::Eof(idx);
                        break;
                    }
                    (CrLfPassed, b'.') => {
                        split = BufSplit::Escape(idx);
                        break;
                    }

                    // If we can't do either of the above, just continue reading stuff
                    (_, _) => self.state = Running,
                }
            }

            // Then, look at the last 2 characters
            let l = self.buf.len();
            if split == BufSplit::Nowhere {
                if l >= 2 {
                    match (self.state, self.buf[l - 2], self.buf[l - 1]) {
                        // If we may be stopping the buffer somewhere in \r\n.\r\n
                        (CrLfPassed, b'.', b'\r') => split = BufSplit::Unknown(l - 2),
                        (CrPassed, b'\n', b'.') => {
                            self.state = CrLfPassed;
                            split = BufSplit::Unknown(l - 1);
                        }

                        // If there is a \r\n.
                        (CrLfPassed, b'.', _) => split = BufSplit::Escape(l - 2),

                        // Move forward in the \r\n state machine
                        (_, b'\r', b'\n') => self.state = CrLfPassed,
                        (_, _, b'\r') => self.state = CrPassed,

                        // Or just continue reading stuff
                        (_, _, _) => self.state = Running,
                    }
                } else if l == 1 {
                    match (self.state, self.buf[l - 1]) {
                        // If we may be stopping the buffer somewhere in \r\n.\r\n
                        (CrLfPassed, b'.') => split = BufSplit::Unknown(l - 1),

                        // Move forward in the \r\n state machine
                        (_, b'\r') => self.state = CrPassed,
                        (CrPassed, b'\n') => self.state = CrLfPassed,

                        // Or just continue reading stuff
                        (_, _) => self.state = Running,
                    }
                } // Ignore the case l == 0, as it wouldn't send anything anyway
            }

            // Send the buffer if we have something to send
            match split {
                BufSplit::Nowhere if self.buf.len() > 0 => return Ok(Ready(Some(self.buf.take()))),
                BufSplit::Nowhere => (), // Continue to read more data if nothing to send
                BufSplit::Eof(x) => {
                    let res = self.buf.split_to(x);
                    self.buf.advance(3);
                    self.state = Finished;
                    if res.len() > 0 {
                        return Ok(Ready(Some(res)));
                    } else {
                        return Ok(Ready(None));
                    }
                }
                BufSplit::Escape(x) => {
                    let res = self.buf.split_to(x);
                    self.buf.advance(1);
                    self.state = Running;
                    if res.len() > 0 {
                        return Ok(Ready(Some(res)));
                    } else {
                        // Continue to read more data if nothing is to be sent before the escape
                        // point
                        continue;
                    }
                }
                BufSplit::Unknown(x) if x > 0 => return Ok(Ready(Some(self.buf.split_to(x)))),
                BufSplit::Unknown(_) => (), // Continue to read more data if nothing to send
            }

            // Didn't find anything to send, so let's just gather more data from the network
            match self.source.poll()? {
                NotReady => return Ok(NotReady),
                Ready(None) => return Err(()),
                // TODO: print warning and/or add metadata to the error
                Ready(Some(b)) => self.buf.unsplit(b),
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

// TODO: SinkItem = BytesMut
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
        let tests: &[(&[&[u8]], &[u8], &[u8])] = &[
            (
                &[b"foo", b" bar", b"\r\n", b".\r", b"\n"],
                b"foo bar\r\n",
                b"",
            ),
            (&[b"\r\n.\r\n", b"\r\n"], b"\r\n", b"\r\n"),
            (&[b".baz\r\n", b".\r\n", b"foo"], b"baz\r\n", b"foo"),
            (&[b" .baz", b"\r\n.", b"\r\nfoo"], b" .baz\r\n", b"foo"),
            (&[b".\r\n", b"MAIL FROM"], b"", b"MAIL FROM"),
            (&[b"..\r\n.\r\n"], b".\r\n", b""),
            (&[b"foo\r\n. ", b"bar\r\n.\r\n"], b"foo\r\n bar\r\n", b""),
        ];
        for &(inp, out, rem) in tests {
            use helpers::SmtpString;
            println!(
                "\nTrying to parse {:?} into {:?} with {:?} remaining",
                inp.iter().map(|x| SmtpString::from(*x)).collect::<Vec<_>>(),
                SmtpString::from(out),
                SmtpString::from(rem),
            );
            let mut stream = DataStream::new(
                stream::iter_ok(inp.iter().map(|x| BytesMut::from(*x)))
                    .map_err(|()| ())
                    .prependable(),
            );
            let output = stream.by_ref().concat2().wait().unwrap();
            println!("Now computing remaining stuff");
            let remaining = stream.into_inner().concat2().wait().unwrap();
            println!(
                " -> Got {:?} with {:?} remaining",
                SmtpString::from(&output[..]),
                SmtpString::from(&remaining[..])
            );
            assert_eq!(output, BytesMut::from(out));
            assert_eq!(remaining, BytesMut::from(rem));
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

    quickcheck! {
        fn data_stream_and_sink_are_compatible(end_with_crlf: bool, v: Vec<u8>) -> bool {
            eprintln!("Got: ({:?}, {:?})", end_with_crlf, SmtpString::from(&v[..]));
            let mut input = v;
            if end_with_crlf && (input.len() < 2 || &input[(input.len() - 2)..] != b"\r\n") {
                input.extend_from_slice(b"\r\n");
            }
            if !end_with_crlf && input.len() >= 2 && &input[(input.len() - 2)..] == b"\r\n" {
                input.pop();
            }
            let mut on_the_wire = Vec::new();
            {
                let sink = DataSink::new(&mut on_the_wire);
                sink.send_all(stream::iter_ok(input.iter().cloned()))
                    .wait()
                    .unwrap()
                    .0
                    .end()
                    .wait()
                    .unwrap();
            }
            eprintln!("Moving on the wire: {:?}", SmtpString::from(&on_the_wire[..]));
            let received = DataStream::new(stream::iter_ok(vec![on_the_wire].into_iter().map(BytesMut::from)).prependable())
                .map_err(|()| ())
                .concat2()
                .wait()
                .unwrap();
            if !end_with_crlf && !input.is_empty() {
                input.extend_from_slice(b"\r\n");
            }
            received == input
        }

        fn all_leading_dots_are_escaped(v: Vec<Vec<u8>>) -> bool {
            let mut v = v;
            v.extend_from_slice(&[vec![b'\r', b'\n', b'.', b'\r', b'\n']]);
            let r = DataStream::new(stream::iter_ok(v.into_iter().map(BytesMut::from)).prependable())
                .map_err(|()| ())
                .concat2()
                .wait()
                .unwrap();
            r.windows(5).position(|x| x == b"\r\n.\r\n").is_none()
        }
    }
}
