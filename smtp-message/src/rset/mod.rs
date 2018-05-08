use nom::crlf;
use std::io;

use byteslice::ByteSlice;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RsetCommand {
    _useless: (),
}

impl RsetCommand {
    pub fn new() -> RsetCommand {
        RsetCommand { _useless: () }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"RSET\r\n")
    }

    pub fn take_ownership(self) -> RsetCommand {
        self
    }
}

named!(pub command_rset_args(ByteSlice) -> RsetCommand,
    do_parse!(
        eat_spaces >> crlf >>
        (RsetCommand {
            _useless: ()
        })
    )
);

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::*;

    #[test]
    fn valid_command_rset_args() {
        let tests = vec![&b" \t  \t \r\n"[..], &b"\r\n"[..]];
        for test in tests.into_iter() {
            let b = Bytes::from(test);
            match command_rset_args(ByteSlice::from(&b)) {
                IResult::Done(rem, RsetCommand { _useless: () }) if rem.len() == 0 => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        RsetCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"RSET\r\n");
    }
}
