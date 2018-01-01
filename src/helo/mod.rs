use std::fmt;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct HeloCommand<'a> {
    domain: &'a [u8],
}

impl<'a> HeloCommand<'a> {
    pub fn domain(&self) -> &'a [u8] {
        self.domain
    }
}

impl<'a> fmt::Debug for HeloCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "HeloCommand {{ domain: {} }}", bytes_to_dbg(self.domain))
    }
}

named!(pub command_helo_args(&[u8]) -> HeloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (HeloCommand {
            domain: domain
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_helo_args() {
        let tests = vec![
            (&b" \t hello.world \t \r\n"[..], HeloCommand {
                domain: &b"hello.world"[..],
            }),
            (&b"hello.world\r\n"[..], HeloCommand {
                domain: &b"hello.world"[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_helo_args(s), IResult::Done(&b""[..], r));
        }
    }
}
