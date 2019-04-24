mod sink;
mod stream;

use nom::crlf;
use std::io;

use crate::{byteslice::ByteSlice, stupidparsers::eat_spaces};

pub use self::{sink::DataSink, stream::DataStream};

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

named!(pub command_data_args(ByteSlice) -> DataCommand, do_parse!(
    tag_no_case!("DATA") >> eat_spaces >> crlf >>
    (DataCommand { _useless: () })
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::{Bytes, BytesMut};
    use futures::{executor::block_on, stream, StreamExt};
    use nom::IResult;

    use crate::streamext::StreamExt as SmtpStreamExt;

    #[test]
    fn valid_command_data_args() {
        let tests = vec![&b"DATA \t  \t \r\n"[..], &b"daTa\r\n"[..]];
        for test in tests.into_iter() {
            let b = Bytes::from(test);
            match command_data_args(ByteSlice::from(&b)) {
                IResult::Done(rem, DataCommand { _useless: () }) if rem.len() == 0 => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_command_data_build() {
        let mut v = Vec::new();
        DataCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"DATA\r\n");
    }

    quickcheck! {
        fn data_stream_and_sink_are_compatible(end_with_crlf: bool, v: Vec<Vec<u8>>) -> bool {
            let mut input = v.into_iter().map(|x| Bytes::from(x)).collect::<Vec<_>>();
            eprintln!("Got: ({:?}, {:?})", end_with_crlf, input);
            let mut raw_input = input.iter().flat_map(|x| x.iter().cloned()).collect::<Vec<_>>();
            if end_with_crlf && (raw_input.len() < 2 || &raw_input[(raw_input.len() - 2)..] != b"\r\n") {
                raw_input.extend_from_slice(b"\r\n");
                input.push(Bytes::from(&b"\r\n"[..]));
            }
            if !end_with_crlf && raw_input.len() >= 2 && &raw_input[(raw_input.len() - 2)..] == b"\r\n" {
                raw_input.pop();
                let l = input.len();
                let ll = input[l - 1].len();
                input[l - 1].truncate(ll - 1);
            }
            let mut on_the_wire = Vec::new();
            {
                let mut sink = DataSink::new(&mut on_the_wire);
                block_on(async {
                    for i in input.iter().cloned() {
                        await!(sink.send(i)).unwrap();
                    }
                    await!(sink.end()).unwrap();
                });
            }
            eprintln!("Moving on the wire: {:?}", on_the_wire);
            let received = block_on(async {
                let mut stream = DataStream::new(
                    stream::iter(
                        on_the_wire.into_iter().map(BytesMut::from)
                    ).prependable()
                );
                let mut res = BytesMut::new();
                while let Some(i) = await!(stream.next()) {
                    res.unsplit(i);
                }
                res
            });
            eprintln!("Recovered: {:?}", received);
            if !end_with_crlf && !raw_input.is_empty() {
                raw_input.extend_from_slice(b"\r\n");
            }
            eprintln!("Expected: {:?}", Bytes::from(&raw_input[..]));
            received == raw_input
        }
    }
}
