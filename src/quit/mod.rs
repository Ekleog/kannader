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

    pub fn build(&self) -> Vec<u8> {
        vec![b'\r', b'\n']
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
        let tests = vec![
            &b" \t  \t \r\n"[..],
            &b"\r\n"[..],
        ];
        for test in tests.into_iter() {
            assert_eq!(command_quit_args(test), IResult::Done(&b""[..], QuitCommand {
                _useless: ()
            }));
        }
    }

    #[test]
    fn valid_build() {
        assert_eq!(QuitCommand::new().build(), b"\r\n");
    }
}
