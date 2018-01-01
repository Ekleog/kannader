use nom::crlf;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RsetCommand {
    _useless: (),
}

named!(pub command_rset_args(&[u8]) -> RsetCommand,
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
    use nom::*;

    #[test]
    fn valid_command_rset_args() {
        let tests = vec![
            &b" \t  \t \r\n"[..],
            &b"\r\n"[..],
        ];
        for test in tests.into_iter() {
            assert_eq!(command_rset_args(test), IResult::Done(&b""[..], RsetCommand {
                _useless: ()
            }));
        }
    }
}
