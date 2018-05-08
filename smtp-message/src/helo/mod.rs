use std::io;

use byteslice::ByteSlice;
use domain::{hostname, Domain};
use stupidparsers::eat_spaces;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct HeloCommand {
    domain: Domain,
}

impl HeloCommand {
    pub fn new(domain: Domain) -> HeloCommand {
        HeloCommand { domain }
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"HELO ")?;
        w.write_all(&self.domain.as_string().bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_helo_args(ByteSlice) -> HeloCommand,
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

    use bytes::Bytes;
    use nom::IResult;

    // TODO: merge implementation and tests for EHLO/HELO, NOOP/VRFY, etc.

    #[test]
    fn valid_command_helo_args() {
        let tests = vec![
            (&b" \t hello.world \t \r\n"[..], &b"hello.world"[..]),
            (&b"hello.world\r\n"[..], &b"hello.world"[..]),
        ];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_helo_args(ByteSlice::from(&b)) {
                IResult::Done(rem, HeloCommand { ref domain })
                    if rem.len() == 0 && domain.as_string().bytes() == &Bytes::from(&r[..]) =>
                {
                    ()
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        let b = Bytes::from(&b"test.example.org"[..]);
        HeloCommand::new(Domain::new(ByteSlice::from(&b)).unwrap())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"HELO test.example.org\r\n");

        let b = Bytes::from(&b"test."[..]);
        assert!(Domain::new((&b).into()).is_err());
    }
}
