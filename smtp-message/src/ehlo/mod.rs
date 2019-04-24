use std::io;

use crate::{
    byteslice::ByteSlice,
    domain::{hostname, Domain},
    sendable::Sendable,
    stupidparsers::eat_spaces,
};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct EhloCommand {
    domain: Domain,
}

impl EhloCommand {
    pub fn new(domain: Domain) -> EhloCommand {
        EhloCommand { domain }
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EHLO ")?;
        self.domain.send_to(w)?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_ehlo_args(ByteSlice) -> EhloCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("EHLO") >>
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

    use bytes::Bytes;
    use nom::IResult;

    use crate::smtpstring::SmtpString;

    #[test]
    fn valid_command_ehlo_args() {
        let tests = vec![
            (&b"eHlO \t hello.world \t \r\n"[..], b"hello.world"),
            (&b"EHLO hello.world\r\n"[..], b"hello.world"),
        ];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_ehlo_args(ByteSlice::from(&b)) {
                IResult::Done(rem, EhloCommand { ref domain })
                    if rem.len() == 0
                        && SmtpString::from_sendable(domain).unwrap().bytes()
                            == &Bytes::from(&r[..]) =>
                {
                    ()
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_builds() {
        let mut v = Vec::new();
        let b = Bytes::from(&b"test.foo.bar"[..]);
        EhloCommand::new(Domain::parse_slice(&b).unwrap())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"EHLO test.foo.bar\r\n");

        let b = Bytes::from(&b"test."[..]);
        assert!(Domain::parse((&b).into()).is_err());
        let b = Bytes::from(&b"test.foo.bar "[..]);
        assert!(Domain::parse((&b).into()).is_err());
        let b = Bytes::from(&b"-test.foo.bar"[..]);
        assert!(Domain::parse((&b).into()).is_err());
    }
}
