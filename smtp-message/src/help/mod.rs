use std::io;

use byteslice::ByteSlice;
use smtpstring::SmtpString;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct HelpCommand {
    subject: SmtpString,
}

impl HelpCommand {
    pub fn new(subject: SmtpString) -> HelpCommand {
        HelpCommand { subject }
    }

    pub fn subject(&self) -> &SmtpString {
        &self.subject
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELP ")?;
        w.write_all(&self.subject.bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

// TODO: (B) this opt!(…) allows HELPfoo, which is wrong, like other commands
named!(pub command_help_args(ByteSlice) -> HelpCommand, do_parse!(
    tag_no_case!("HELP") >> opt!(one_of!(spaces!())) >>
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (HelpCommand {
        subject: res.promote().into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::IResult;

    #[test]
    fn valid_command_help_args() {
        let tests = vec![
            (
                &b"help \t hello.world \t \r\n"[..],
                HelpCommand {
                    subject: (&b"\t hello.world \t "[..]).into(),
                },
            ),
            (
                &b"HELP\r\n"[..],
                HelpCommand {
                    subject: (&b""[..]).into(),
                },
            ),
            (
                &b"hElP \r\n"[..],
                HelpCommand {
                    subject: (&b""[..]).into(),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_help_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected result: {:?}", x),
            }
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
