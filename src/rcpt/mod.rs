use nom::crlf;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RcptCommand<'a> {
    // TO: parameter with the “@ONE,@TWO:” portion removed, as per RFC5321 Appendix C
    to: Email<'a>,
}

impl<'a> RcptCommand<'a> {
    pub fn new(to: Email) -> RcptCommand {
        RcptCommand { to }
    }

    pub fn to(&self) -> Email<'a> {
        self.to
    }

    pub fn build(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(
                          4 + self.to.raw_localpart().len() +
                          self.to.hostname().map(|x| 1 + x.len()).unwrap_or(0)
                          + 3);
        res.extend_from_slice(b"TO:<");
        res.extend_from_slice(self.to.raw_localpart());
        if let Some(x) = self.to.hostname() {
            res.push(b'@');
            res.extend_from_slice(x);
        }
        res.extend_from_slice(b">\r\n");
        res
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

    #[test]
    fn valid_command_rcpt_args() {
        let tests: Vec<(&[u8], &[u8], Option<&[u8]>)> = vec![
            (b" TO:<@one,@two:foo@bar.baz>\r\n", b"foo", Some(b"bar.baz")),
            (b"tO: quux@example.net  \t \r\n", b"quux", Some(b"example.net")),
            (b"TO:<Postmaster>\r\n", b"Postmaster", None),
            (b"TO: \t poStmaster\r\n", b"poStmaster", None),
        ];
        for (s, l, h) in tests.into_iter() {
            let res = command_rcpt_args(s).unwrap().1;
            assert_eq!(res.to().raw_localpart(), l);
            assert_eq!(res.to().hostname(), h);
        }
    }

    #[test]
    fn valid_build() {
        assert_eq!(RcptCommand::new(Email::new(b"foo", Some(b"bar.com"))).build(),
                   b"TO:<foo@bar.com>\r\n");
        assert_eq!(RcptCommand::new(Email::new(b"Postmaster", None)).build(),
                   b"TO:<Postmaster>\r\n");
    }
}
