use std::{fmt, io};
use nom::IResult;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct HeloCommand<'a> {
    domain: &'a [u8],
}

impl<'a> HeloCommand<'a> {
    pub fn new<'b>(domain: &'b [u8]) -> Result<HeloCommand<'b>, ParseError> {
        match hostname(domain) {
            IResult::Done(b"", domain) => Ok(HeloCommand { domain }),
            IResult::Done(rem, _)      => Err(ParseError::DidNotConsumeEverything(rem.len())),
            IResult::Error(e)          => Err(ParseError::ParseError(e)),
            IResult::Incomplete(n)     => Err(ParseError::IncompleteString(n)),
        }
    }

    pub unsafe fn with_raw_domain<'b>(domain: &'b [u8]) -> HeloCommand<'b> {
        HeloCommand { domain }
    }

    pub fn domain(&self) -> &'a [u8] {
        self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELO ")?;
        w.write_all(self.domain)?;
        w.write_all(b"\r\n")
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

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        HeloCommand::new(b"test.example.org").unwrap().send_to(&mut v).unwrap();
        assert_eq!(v, b"HELO test.example.org\r\n");

        assert!(HeloCommand::new(b"test.").is_err());

        v = Vec::new();
        unsafe { HeloCommand::with_raw_domain(b"test.") }.send_to(&mut v).unwrap();
        assert_eq!(v, b"HELO test.\r\n");
    }
}
