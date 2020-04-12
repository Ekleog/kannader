use nom::crlf;
use std::io;

use crate::{byteslice::ByteSlice, stupidparsers::eat_spaces};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct QuitCommand {
    _useless: (),
}

impl QuitCommand {
    pub fn new() -> QuitCommand {
        QuitCommand { _useless: () }
    }

    pub fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        w.write_all(b"QUIT\r\n")
    }

    pub fn take_ownership(self) -> QuitCommand {
        self
    }
}

named!(pub command_quit_args(ByteSlice) -> QuitCommand,
    do_parse!(
        tag_no_case!("QUIT") >> eat_spaces >> crlf >>
        (QuitCommand {
            _useless: ()
        })
    )
);

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::IResult;

    #[test]
    fn valid_command_quit_args() {
        let tests = vec![&b"QUIT \t  \t \r\n"[..], &b"quit\r\n"[..]];
        for test in tests.into_iter() {
            let b = Bytes::from(test);
            match command_quit_args(ByteSlice::from(&b)) {
                IResult::Done(rem, QuitCommand { _useless: () }) if rem.len() == 0 => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        QuitCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"QUIT\r\n");
    }
}
