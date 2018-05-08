use nom::crlf;
use std::io;

use byteslice::ByteSlice;
use email::{address_in_maybe_bracketed_path, Email};
use stupidparsers::eat_spaces;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RcptCommand {
    // TO: parameter with the forward-path (“@ONE,@TWO:” portion)
    // removed, as per RFC5321 Appendix C
    // TODO: support the SP arguments
    pub to: Email,
}

impl RcptCommand {
    pub fn new(to: Email) -> RcptCommand {
        RcptCommand { to }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"RCPT TO:<")?;
        w.write_all(&self.to.as_string().bytes()[..])?;
        w.write_all(b">\r\n")
    }
}

named!(pub command_rcpt_args(ByteSlice) -> RcptCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("TO:") >>
        to: address_in_maybe_bracketed_path >>
        // TODO: support the SP arguments
        crlf >>
        (RcptCommand {
            to,
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    use domain::Domain;

    #[test]
    fn valid_command_rcpt_args() {
        let tests: Vec<(&[u8], &[u8], Option<&[u8]>)> = vec![
            (b" TO:<@one,@two:foo@bar.baz>\r\n", b"foo", Some(b"bar.baz")),
            (
                b"tO: quux@example.net  \t \r\n",
                b"quux",
                Some(b"example.net"),
            ),
            (b"TO:<Postmaster>\r\n", b"Postmaster", None),
            (b"TO: \t poStmaster\r\n", b"poStmaster", None),
        ];
        for (s, l, h) in tests.into_iter() {
            let b = Bytes::from(s);
            let res = command_rcpt_args(ByteSlice::from(&b)).unwrap().1;
            assert_eq!(res.to.raw_localpart().bytes(), l);
            assert_eq!(
                res.to.hostname(),
                &h.map(|x| Domain::parse_slice(x).unwrap())
            );
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        RcptCommand::new(Email::new(
            (&b"foo"[..]).into(),
            Some(Domain::parse_slice(b"bar.com").unwrap()),
        )).send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"RCPT TO:<foo@bar.com>\r\n");

        v = Vec::new();
        RcptCommand::new(Email::new((&b"Postmaster"[..]).into(), None))
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"RCPT TO:<Postmaster>\r\n");
    }
}
