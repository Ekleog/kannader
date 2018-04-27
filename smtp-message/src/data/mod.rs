mod sink;
mod stream;

use nom::crlf;
use std::io;

use parse_helpers::*;

pub use self::{sink::*, stream::*};

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

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    use bytes::{Bytes, BytesMut};
    use tokio::{self, prelude::*};

    use helpers::*;

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
                let sink = DataSink::new(&mut on_the_wire);
                sink.send_all(tokio::prelude::stream::iter_ok(input.iter().cloned()))
                    .wait()
                    .unwrap()
                    .0
                    .end()
                    .wait()
                    .unwrap();
            }
            eprintln!("Moving on the wire: {:?}", on_the_wire);
            let received = DataStream::new(
                tokio::prelude::stream::iter_ok(
                    on_the_wire.into_iter().map(BytesMut::from)
                ).prependable()
            ).map_err(|()| ())
                .concat2()
                .wait()
                .unwrap();
            if !end_with_crlf && !input.is_empty() {
                raw_input.extend_from_slice(b"\r\n");
            }
            received == raw_input
        }
    }
}
