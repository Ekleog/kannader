use std::io;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct HeloCommand<'a> {
    domain: Domain<'a>,
}

impl<'a> HeloCommand<'a> {
    // TODO: add a Domain<'b> type and use it here
    pub fn new<'b>(domain: Domain<'b>) -> HeloCommand<'b> {
        HeloCommand { domain }
    }

    pub fn domain(&self) -> &Domain {
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

    use nom::IResult;

    #[test]
    fn valid_command_helo_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                HeloCommand {
                    domain: Domain::new((&b"hello.world"[..]).into()).unwrap(),
                },
            ),
            (
                &b"hello.world\r\n"[..],
                HeloCommand {
                    domain: Domain::new((&b"hello.world"[..]).into()).unwrap(),
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
        HeloCommand::new(Domain::new(SmtpString::from(&b"test.example.org"[..])).unwrap())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"HELO test.example.org\r\n");

        assert!(Domain::new((&b"test."[..]).into()).is_err());
    }
}
