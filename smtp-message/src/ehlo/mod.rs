use nom::IResult;
use std::io;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct EhloCommand<'a> {
    domain: SmtpString<'a>,
}

impl<'a> EhloCommand<'a> {
    // TODO: add a `Domain` type and use it here
    pub fn new<'b>(domain: SmtpString<'b>) -> Result<EhloCommand<'b>, ParseError> {
        match hostname(domain.as_bytes()) {
            IResult::Done(b"", _) => (),
            IResult::Done(rem, _) => return Err(ParseError::DidNotConsumeEverything(rem.len())),
            IResult::Error(e) => return Err(ParseError::ParseError(e)),
            IResult::Incomplete(n) => return Err(ParseError::IncompleteString(n)),
        }
        Ok(EhloCommand { domain })
    }

    pub fn domain(&self) -> &SmtpString {
        &self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EHLO ")?;
        w.write_all(self.domain.as_bytes())?;
        w.write_all(b"\r\n")
    }

    pub fn take_ownership<'b>(self) -> EhloCommand<'b> {
        EhloCommand {
            domain: self.domain.take_ownership(),
        }
    }
}

named!(pub command_ehlo_args(&[u8]) -> EhloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (EhloCommand {
            domain: domain.into(),
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_command_ehlo_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                EhloCommand {
                    domain: (&b"hello.world"[..]).into(),
                },
            ),
            (
                &b"hello.world\r\n"[..],
                EhloCommand {
                    domain: (&b"hello.world"[..]).into(),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_ehlo_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_builds() {
        let mut v = Vec::new();
        EhloCommand::new((&b"test.foo.bar"[..]).into())
            .unwrap()
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EHLO test.foo.bar\r\n");

        assert!(EhloCommand::new((&b"test."[..]).into()).is_err());
        assert!(EhloCommand::new((&b"test.foo.bar "[..]).into()).is_err());
        assert!(EhloCommand::new((&b"-test.foo.bar"[..]).into()).is_err());
    }
}
