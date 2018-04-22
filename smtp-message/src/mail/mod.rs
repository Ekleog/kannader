use nom::crlf;
use std::io;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct MailCommand<'a> {
    from:   Option<Email<'a>>,
    params: Option<SpParameters<'a>>,
}

impl<'a> MailCommand<'a> {
    pub fn new<'b>(from: Option<Email<'b>>, params: Option<SpParameters<'b>>) -> MailCommand<'b> {
        MailCommand { from, params }
    }

    pub fn from(&self) -> &Option<Email> {
        &self.from
    }

    pub fn into_from(self) -> Option<Email<'a>> {
        self.from
    }

    pub fn params(&self) -> &Option<SpParameters> {
        &self.params
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        let address = self.from.as_ref().map(|x| x.as_smtp_string());
        w.write_all(b"MAIL FROM:<")?;
        w.write_all(address.as_ref().map(|x| x.as_bytes()).unwrap_or(b""))?;
        w.write_all(b">\r\n")
        // TODO: also send parameters
    }

    pub fn take_ownership<'b>(self) -> MailCommand<'b> {
        MailCommand {
            from:   self.from.map(|x| x.take_ownership()),
            params: self.params.map(|x| x.take_ownership()),
        }
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
            map!(tag!("<>"), |_| None) |
            map!(address_in_maybe_bracketed_path, |x| Some(x.0))
        ) >>
        sp: opt!(preceded!(tag!("SP"), sp_parameters)) >>
        crlf >>
        (MailCommand {
            from: from,
            params: sp.into(),
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;

    use nom::IResult;

    #[test]
    fn valid_command_mail_args() {
        let tests = vec![
            (
                &b" FROM:<@one,@two:foo@bar.baz>\r\n"[..],
                MailCommand {
                    from:   Some(
                        Email::parse(&(&b"foo@bar.baz"[..]).into())
                            .unwrap()
                            .take_ownership(),
                    ),
                    params: None,
                },
            ),
            (
                &b"FrOm: quux@example.net  \t \r\n"[..],
                MailCommand {
                    from:   Some(
                        Email::parse(&(&b"quux@example.net"[..]).into())
                            .unwrap()
                            .take_ownership(),
                    ),
                    params: None,
                },
            ),
            (
                &b"FROM:<>\r\n"[..],
                MailCommand {
                    from:   None,
                    params: None,
                },
            ),
            (
                &b"FROM:<> SP hello=world SP foo\r\n"[..],
                MailCommand {
                    from:   None,
                    params: Some(SpParameters::new(
                        vec![
                            ((&b"hello"[..]).into(), Some((&b"world"[..]).into())),
                            ((b"foo"[..]).into(), None),
                        ].into_iter()
                            .collect(),
                    )),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            let res = command_mail_args(s);
            let exp = IResult::Done(&b""[..], r);
            println!("Parsing {:?}: {:?}, expected {:?}", s, res, exp);
            assert_eq!(res, exp);
        }
    }

    #[test]
    fn incomplete_args() {
        assert!(command_mail_args(b" FROM:<foo@bar.com").is_incomplete());
        assert!(command_mail_args(b" FROM:foo@bar.com").is_incomplete());
    }

    // TODO: quickcheck parse = generate for all
    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        MailCommand::new(
            Some(
                Email::parse(&(&b"foo@bar.baz"[..]).into())
                    .unwrap()
                    .take_ownership(),
            ),
            None,
        ).send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"MAIL FROM:<foo@bar.baz>\r\n");

        let mut v = Vec::new();
        MailCommand::new(None, None).send_to(&mut v).unwrap();
        assert_eq!(v, b"MAIL FROM:<>\r\n");
    }
}
