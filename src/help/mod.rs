use std::{fmt, io};

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct HelpCommand<'a> {
    subject: &'a [u8],
}

impl<'a> HelpCommand<'a> {
    pub fn new(subject: &[u8]) -> HelpCommand {
        HelpCommand { subject }
    }

    pub fn subject(&self) -> &'a [u8] {
        self.subject
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELP ")?;
        w.write_all(self.subject)?;
        w.write_all(b"\r\n")
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

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        HelpCommand::new(b"topic").send_to(&mut v).unwrap();
        assert_eq!(v, b"HELP topic\r\n");
    }
}
