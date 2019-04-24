use std::io;

use crate::{byteslice::ByteSlice, smtpstring::SmtpString};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct ExpnCommand {
    name: SmtpString,
}

impl ExpnCommand {
    pub fn new(name: SmtpString) -> ExpnCommand {
        ExpnCommand { name }
    }

    pub fn name(&self) -> &SmtpString {
        &self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EXPN ")?;
        w.write_all(&self.name.bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_expn_args(ByteSlice) -> ExpnCommand, do_parse!(
    tag_no_case!("EXPN") >> one_of!(spaces!()) >>
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (ExpnCommand {
        name: res.promote().into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::IResult;

    #[test]
    fn valid_command_expn_args() {
        let tests = vec![(
            &b"EXpN \t hello.world \t \r\n"[..],
            ExpnCommand {
                name: (&b"\t hello.world \t "[..]).into(),
            },
        )];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_expn_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        ExpnCommand::new((&b"foobar"[..]).into())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EXPN foobar\r\n");
    }
}
