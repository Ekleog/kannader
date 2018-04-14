use std::{fmt, io};
use nom::IResult;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct EhloCommand<'a> {
    domain: &'a [u8],
}

impl<'a> EhloCommand<'a> {
    pub fn new<'b>(domain: &'b [u8]) -> Result<EhloCommand<'b>, ParseError> {
        match hostname(domain) {
            IResult::Done(b"", domain) => Ok(EhloCommand { domain }),
            IResult::Done(rem, _) => Err(ParseError::DidNotConsumeEverything(rem.len())),
            IResult::Error(e) => Err(ParseError::ParseError(e)),
            IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
        }
    }

    pub unsafe fn with_raw_domain<'b>(domain: &'b [u8]) -> EhloCommand<'b> {
        EhloCommand { domain }
    }

    pub fn domain(&self) -> &'a [u8] {
        self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EHLO ")?;
        w.write_all(self.domain)?;
        w.write_all(b"\r\n")
    }
}

impl<'a> fmt::Debug for EhloCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "EhloCommand {{ domain: {:?} }}",
            bytes_to_dbg(self.domain)
        )
    }
}

named!(pub command_ehlo_args(&[u8]) -> EhloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (EhloCommand {
            domain: domain
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
                EhloCommand { domain: &b"hello.world"[..] }
            ),
            (
                &b"hello.world\r\n"[..],
                EhloCommand { domain: &b"hello.world"[..] }
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_ehlo_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_builds() {
        let mut v = Vec::new();
        EhloCommand::new(b"test.foo.bar")
            .unwrap()
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EHLO test.foo.bar\r\n");

        assert!(EhloCommand::new(b"test.").is_err());
        assert!(EhloCommand::new(b"test.foo.bar ").is_err());
        assert!(EhloCommand::new(b"-test.foo.bar").is_err());

        v = Vec::new();
        unsafe { EhloCommand::with_raw_domain(b"test.") }
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EHLO test.\r\n");
    }
}
