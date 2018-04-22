use std::io;

use nom::crlf;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RcptCommand<'a> {
    // TO: parameter with the forward-path (“@ONE,@TWO:” portion)
    // removed, as per RFC5321 Appendix C
    to: Email<'a>,
}

impl<'a> RcptCommand<'a> {
    pub fn new(to: Email) -> RcptCommand {
        RcptCommand { to }
    }

    pub fn to(&self) -> &Email {
        &self.to
    }

    pub fn into_to(self) -> Email<'a> {
        self.to
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"RCPT TO:<")?;
        w.write_all(self.to.raw_localpart().as_bytes())?;
        if let &Some(ref host) = self.to.hostname() {
            w.write_all(b"@")?;
            w.write_all(host.as_bytes())?;
        }
        w.write_all(b">\r\n")
    }

    pub fn take_ownership<'b>(self) -> RcptCommand<'b> {
        RcptCommand {
            to: self.to.take_ownership(),
        }
    }
}

named!(pub command_rcpt_args(&[u8]) -> RcptCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("TO:") >>
        to: address_in_maybe_bracketed_path >>
        // TODO: support the SP arguments
        crlf >>
        (RcptCommand {
            to: to.0,
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;

    use helpers::*;

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
            let res = command_rcpt_args(s).unwrap().1;
            assert_eq!(res.to().raw_localpart().as_bytes(), l);
            assert_eq!(res.to().hostname(), &h.map(SmtpString::from));
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        RcptCommand::new(Email::new(
            (&b"foo"[..]).into(),
            Some((&b"bar.com"[..]).into()),
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
