use nom::crlf;
use std::io;

use byteslice::ByteSlice;
use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct MailCommand {
    pub from:   Option<Email>,
    pub params: Option<SpParameters>,
}

impl MailCommand {
    pub fn new(from: Option<Email>, params: Option<SpParameters>) -> MailCommand {
        MailCommand { from, params }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"MAIL FROM:<")?;
        w.write_all(&opt_email_repr(&self.from).bytes()[..])?;
        w.write_all(b">\r\n")
        // TODO: also send parameters
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
named!(pub command_mail_args(ByteSlice) -> MailCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("FROM:") >>
        from: alt!(
            map!(tag!("<>"), |_| None) |
            map!(address_in_maybe_bracketed_path, |x| Some(x))
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

    use bytes::Bytes;
    use nom::IResult;

    #[test]
    fn valid_command_mail_args() {
        let tests = vec![
            (
                &b" FROM:<@one,@two:foo@bar.baz>\r\n"[..],
                MailCommand {
                    from:   Some(Email::parse_slice(b"foo@bar.baz").unwrap()),
                    params: None,
                },
            ),
            (
                &b"FrOm: quux@example.net  \t \r\n"[..],
                MailCommand {
                    from:   Some(Email::parse_slice(b"quux@example.net").unwrap()),
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
                    params: Some(SpParameters(
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
            let b = Bytes::from(s);
            match command_mail_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn incomplete_args() {
        let b = Bytes::from(&b" FROM:<foo@bar.com"[..]);
        assert!(command_mail_args(ByteSlice::from(&b)).is_incomplete());
        let b = Bytes::from(&b" FROM:foo@bar.com"[..]);
        assert!(command_mail_args(ByteSlice::from(&b)).is_incomplete());
    }

    // TODO(low): quickcheck parse = generate for all
    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        MailCommand::new(Some(Email::parse_slice(b"foo@bar.baz").unwrap()), None)
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"MAIL FROM:<foo@bar.baz>\r\n");

        let mut v = Vec::new();
        MailCommand::new(None, None).send_to(&mut v).unwrap();
        assert_eq!(v, b"MAIL FROM:<>\r\n");
    }
}
