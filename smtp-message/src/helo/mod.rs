use nom::IResult;
use std::io;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct HeloCommand<'a> {
    domain: SmtpString<'a>,
}

impl<'a> HeloCommand<'a> {
    // TODO: add a Domain<'b> type and use it here
    pub fn new<'b>(domain: SmtpString<'b>) -> Result<HeloCommand<'b>, ParseError> {
        match hostname(domain.as_bytes()) {
            IResult::Done(b"", _) => (),
            IResult::Done(rem, _) => return Err(ParseError::DidNotConsumeEverything(rem.len())),
            IResult::Error(e) => return Err(ParseError::ParseError(e)),
            IResult::Incomplete(n) => return Err(ParseError::IncompleteString(n)),
        }
        Ok(HeloCommand { domain })
    }

    pub fn domain(&self) -> &SmtpString {
        &self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELO ")?;
        w.write_all(self.domain.as_bytes())?;
        w.write_all(b"\r\n")
    }

    pub fn take_ownership<'b>(self) -> HeloCommand<'b> {
        HeloCommand {
            domain: self.domain.take_ownership(),
        }
    }
}

named!(pub command_helo_args(&[u8]) -> HeloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (HeloCommand {
            domain: domain.into(),
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_command_helo_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                HeloCommand {
                    domain: (&b"hello.world"[..]).into(),
                },
            ),
            (
                &b"hello.world\r\n"[..],
                HeloCommand {
                    domain: (&b"hello.world"[..]).into(),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_helo_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        HeloCommand::new(SmtpString::from(&b"test.example.org"[..]))
            .unwrap()
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"HELO test.example.org\r\n");

        assert!(HeloCommand::new((&b"test."[..]).into()).is_err());
    }
}
