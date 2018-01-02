use std::fmt;

use nom::crlf;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct MailCommand<'a> {
    from: &'a [u8],
}

impl<'a> MailCommand<'a> {
    pub fn raw_from(&self) -> &'a [u8] {
        self.from
    }
}

impl<'a> fmt::Debug for MailCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "MailCommand {{ from: {} }}", bytes_to_dbg(self.from))
    }
}

// This parser actually drops the return-path portion of the parameter. This has been chosen after
// considering that 1/ my ISP answers `501 5.1.0 Invalid Sender` when sent a MAIL FROM with a
// return-path and 2/ anyway, clients SHOULD NOT use a hop-full return-path according to the RFC.
//
// So this is not in strict compliance with the RFC, but will likely turn out to be better for
// interoperability. If you have a use case for strict compliance with the RFC, please by all means
// submit an issue.
named!(pub command_mail_args(&[u8]) -> MailCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("FROM:") >>
        from: alt!(
            map!(tag!("<>"), |_| &b""[..]) |
            map!(address_in_maybe_bracketed_path, |x| x.1)
        ) >>
        // TODO: support the SP arguments
        crlf >>
        (MailCommand {
            from: from,
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_mail_args() {
        let tests = vec![
            (&b" FROM:<@one,@two:foo@bar.baz>\r\n"[..], MailCommand {
                from: &b"foo@bar.baz"[..],
            }),
            (&b"FrOm: quux@example.net  \t \r\n"[..], MailCommand {
                from: &b"quux@example.net"[..],
            }),
            (&b"FROM:<>\r\n"[..], MailCommand {
                from: &b""[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_mail_args(s), IResult::Done(&b""[..], r));
        }
    }
}
