use std::io;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct HelpCommand<'a> {
    subject: SmtpString<'a>,
}

impl<'a> HelpCommand<'a> {
    pub fn new(subject: SmtpString) -> HelpCommand {
        HelpCommand { subject }
    }

    pub fn subject(&self) -> &SmtpString {
        &self.subject
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELP ")?;
        w.write_all(self.subject.as_bytes())?;
        w.write_all(b"\r\n")
    }

    pub fn take_ownership<'b>(self) -> HelpCommand<'b> {
        HelpCommand {
            subject: self.subject.take_ownership(),
        }
    }
}

named!(pub command_help_args(&[u8]) -> HelpCommand, do_parse!(
    eat_spaces >>
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (HelpCommand {
        subject: res.into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_help_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                HelpCommand {
                    subject: (&b"hello.world \t "[..]).into(),
                },
            ),
            (
                &b"\r\n"[..],
                HelpCommand {
                    subject: (&b""[..]).into(),
                },
            ),
            (
                &b" \r\n"[..],
                HelpCommand {
                    subject: (&b""[..]).into(),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_help_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        HelpCommand::new((&b"topic"[..]).into())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"HELP topic\r\n");
    }
}
