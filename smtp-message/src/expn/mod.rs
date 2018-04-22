use std::io;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct ExpnCommand<'a> {
    name: SmtpString<'a>,
}

impl<'a> ExpnCommand<'a> {
    pub fn new(name: SmtpString) -> ExpnCommand {
        ExpnCommand { name }
    }

    pub fn name(&self) -> &SmtpString {
        &self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EXPN ")?;
        w.write_all(self.name.as_bytes())?;
        w.write_all(b"\r\n")
    }

    pub fn take_ownership<'b>(self) -> ExpnCommand<'b> {
        ExpnCommand {
            name: self.name.take_ownership(),
        }
    }
}

named!(pub command_expn_args(&[u8]) -> ExpnCommand, do_parse!(
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (ExpnCommand {
        name: res.into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_expn_args() {
        let tests = vec![(
            &b" \t hello.world \t \r\n"[..],
            ExpnCommand {
                name: (&b" \t hello.world \t "[..]).into(),
            },
        )];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_expn_args(s), IResult::Done(&b""[..], r));
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
