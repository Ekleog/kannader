use std::fmt;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct HelpCommand<'a> {
    subject: &'a [u8],
}

impl<'a> HelpCommand<'a> {
    pub fn subject(&self) -> &'a [u8] {
        self.subject
    }
}

impl<'a> fmt::Debug for HelpCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "HelpCommand {{ subject: {} }}", bytes_to_dbg(self.subject))
    }
}

named!(pub command_help_args(&[u8]) -> HelpCommand, do_parse!(
    eat_spaces >>
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (HelpCommand {
        subject: res,
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_help_args() {
        let tests = vec![
            (&b" \t hello.world \t \r\n"[..], HelpCommand {
                subject: &b"hello.world \t "[..],
            }),
            (&b"\r\n"[..], HelpCommand {
                subject: &b""[..],
            }),
            (&b" \r\n"[..], HelpCommand {
                subject: &b""[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_help_args(s), IResult::Done(&b""[..], r));
        }
    }
}
