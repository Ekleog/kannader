use nom::crlf;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RcptCommand<'a> {
    // TO: parameter with the “@ONE,@TWO:” portion removed, as per RFC5321 Appendix C
    to: Email<'a>,
}

impl<'a> RcptCommand<'a> {
    pub fn to(&self) -> Email<'a> {
        self.to
    }
}

named!(pub command_rcpt_args(&[u8]) -> RcptCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("TO:") >>
        to: alt!(
            postmaster_maybe_bracketed_address |
            address_in_maybe_bracketed_path
        ) >>
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
        let tests: Vec<(&[u8], &[u8], &[u8])> = vec![
            (b" TO:<@one,@two:foo@bar.baz>\r\n", b"foo", b"bar.baz"),
            (b"tO: quux@example.net  \t \r\n", b"quux", b"example.net"),
            (b"TO:<Postmaster>\r\n", b"Postmaster", b""),
            (b"TO: \t poStmaster\r\n", b"poStmaster", b""),
        ];
        for (s, l, h) in tests.into_iter() {
            let res = command_rcpt_args(s).unwrap().1;
            assert_eq!(res.to().raw_localpart(), l);
            assert_eq!(res.to().raw_hostname(), h);
        }
    }
}
