use std::fmt;

use nom::crlf;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct RcptCommand<'a> {
    // TO: parameter with the “@ONE,@TWO:” portion removed, as per RFC5321 Appendix C
    to: &'a [u8],
}

impl<'a> RcptCommand<'a> {
    pub fn to(&self) -> &'a [u8] {
        self.to
    }
}

impl<'a> fmt::Debug for RcptCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "RcptCommand {{ to: {} }}", bytes_to_dbg(self.to))
    }
}

named!(pub command_rcpt_args(&[u8]) -> RcptCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("TO:") >>
        to: alt!(
            tag_no_case!("<postmaster>") | tag_no_case!("postmaster") |
            address_in_maybe_bracketed_path
        ) >>
        // TODO: support the SP arguments
        crlf >>
        (RcptCommand {
            to: to,
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_rcpt_args() {
        let tests = vec![
            (&b" TO:<@one,@two:foo@bar.baz>\r\n"[..], RcptCommand {
                to: &b"foo@bar.baz"[..],
            }),
            (&b"tO: quux@example.net  \t \r\n"[..], RcptCommand {
                to: &b"quux@example.net"[..],
            }),
            (&b"TO:<Postmaster>\r\n"[..], RcptCommand {
                to: &b"<Postmaster>"[..],
            }),
            (&b"TO: \t poStmaster\r\n"[..], RcptCommand {
                to: &b"poStmaster"[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_rcpt_args(s), IResult::Done(&b""[..], r));
        }
    }
}
