use nom::{crlf, IResult};
use std::{collections::HashMap, fmt, io};

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct MailCommand<'a> {
    from:   &'a [u8],
    params: Option<SpParameters<'a>>,
}

impl<'a> MailCommand<'a> {
    pub fn new<'b>(
        from: &'b [u8],
        params: Option<SpParameters<'b>>,
    ) -> Result<MailCommand<'b>, ParseError> {
        match email(from) {
            IResult::Done(b"", _) => Ok(MailCommand { from, params }),
            IResult::Done(rem, _) => Err(ParseError::DidNotConsumeEverything(rem.len())),
            IResult::Error(e) => Err(ParseError::ParseError(e)),
            IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
        }
    }

    pub unsafe fn with_raw_from<'b>(
        from: &'b [u8],
        params: Option<SpParameters<'b>>,
    ) -> MailCommand<'b> {
        MailCommand { from, params }
    }

    pub fn raw_from(&self) -> &'a [u8] {
        self.from
    }

    pub fn params(&self) -> &Option<SpParameters> {
        &self.params
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"MAIL FROM:<")?;
        w.write_all(self.from)?;
        w.write_all(b">\r\n")
        // TODO: also send parameters
    }
}

impl<'a> fmt::Debug for MailCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "MailCommand {{ from: {:?}, params: {:?} }}",
            bytes_to_dbg(self.from),
            self.params.as_ref().map(|x| x.0
                .iter()
                .map(|(k, v)| (bytes_to_dbg(k), v.map(bytes_to_dbg)))
                .collect::<HashMap<_, _>>())
        )
    }
}

// This parser actually drops the return-path portion of the parameter. This
// has been chosen after considering that 1/ my ISP answers
// `501 5.1.0 Invalid Sender` when sent a MAIL FROM with a return-path and
// 2/ anyway, clients SHOULD NOT use a hop-full return-path according to the
// RFC.
//
// So this is not in strict compliance with the RFC, but will likely turn out
// to be better for interoperability. If you have a use case for strict
// compliance with the RFC, please by all means submit an issue.
named!(pub command_mail_args(&[u8]) -> MailCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("FROM:") >>
        from: alt!(
            map!(tag!("<>"), |_| &b""[..]) |
            map!(address_in_maybe_bracketed_path, |x| x.1)
        ) >>
        sp: opt!(preceded!(tag!("SP"), sp_parameters)) >>
        crlf >>
        (MailCommand {
            from: from,
            params: sp,
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_command_mail_args() {
        let tests = vec![
            (
                &b" FROM:<@one,@two:foo@bar.baz>\r\n"[..],
                MailCommand {
                    from:   &b"foo@bar.baz"[..],
                    params: None,
                },
            ),
            (
                &b"FrOm: quux@example.net  \t \r\n"[..],
                MailCommand {
                    from:   &b"quux@example.net"[..],
                    params: None,
                },
            ),
            (
                &b"FROM:<>\r\n"[..],
                MailCommand {
                    from:   &b""[..],
                    params: None,
                },
            ),
            (
                &b"FROM:<> SP hello=world SP foo\r\n"[..],
                MailCommand {
                    from:   &b""[..],
                    params: Some(SpParameters(
                        vec![(&b"hello"[..], Some(&b"world"[..])), (b"foo", None)]
                            .into_iter()
                            .collect(),
                    )),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_mail_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn incomplete_args() {
        assert!(command_mail_args(b" FROM:<foo@bar.com").is_incomplete());
        assert!(command_mail_args(b" FROM:foo@bar.com").is_incomplete());
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        MailCommand::new(b"foo@bar.baz", None)
            .unwrap()
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"MAIL FROM:<foo@bar.baz>\r\n");

        assert!(MailCommand::new(b"foo@", None).is_err());
        assert!(MailCommand::new(b"foo@bar.", None).is_err());
        assert!(MailCommand::new(b"foo@.baz", None).is_err());
        assert!(MailCommand::new(b"@bar.baz", None).is_err());
        assert!(MailCommand::new(b"\"foo@bar.baz", None).is_err());

        v = Vec::new();
        unsafe { MailCommand::with_raw_from(b"foo@", None) }
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"MAIL FROM:<foo@>\r\n");
    }
}
