use std::io;

use nom::crlf;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct QuitCommand {
    _useless: (),
}

impl QuitCommand {
    pub fn new() -> QuitCommand {
        QuitCommand { _useless: () }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"QUIT\r\n")
    }

    pub fn take_ownership(self) -> QuitCommand {
        self
    }
}

named!(pub command_quit_args(&[u8]) -> QuitCommand,
    do_parse!(
        eat_spaces >> crlf >>
        (QuitCommand {
            _useless: ()
        })
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_quit_args() {
        let tests = vec![&b" \t  \t \r\n"[..], &b"\r\n"[..]];
        for test in tests.into_iter() {
            assert_eq!(
                command_quit_args(test),
                IResult::Done(&b""[..], QuitCommand { _useless: () })
            );
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        QuitCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"QUIT\r\n");
    }
}
