use std::io;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct EhloCommand<'a> {
    domain: Domain<'a>,
}

impl<'a> EhloCommand<'a> {
    pub fn new<'b>(domain: Domain<'b>) -> EhloCommand<'b> {
        EhloCommand { domain }
    }

    pub fn domain(&self) -> &Domain {
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

    use nom::IResult;

    #[test]
    fn valid_command_ehlo_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                EhloCommand {
                    domain: Domain::new((&b"hello.world"[..]).into()).unwrap(),
                },
            ),
            (
                &b"hello.world\r\n"[..],
                EhloCommand {
                    domain: Domain::new((&b"hello.world"[..]).into()).unwrap(),
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
        EhloCommand::new(Domain::new((&b"test.foo.bar"[..]).into()).unwrap())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EHLO test.foo.bar\r\n");

        assert!(Domain::new((&b"test."[..]).into()).is_err());
        assert!(Domain::new((&b"test.foo.bar "[..]).into()).is_err());
        assert!(Domain::new((&b"-test.foo.bar"[..]).into()).is_err());
    }
}
