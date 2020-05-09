use nom::crlf;
use std::io;

use crate::{
    byteslice::ByteSlice,
    email::{address_in_maybe_bracketed_path, Email},
    parameters::{parse_parameters, Parameters},
    sendable::Sendable,
    stupidparsers::eat_spaces,
};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct MailCommand {
    pub from: Option<Email>,
    pub params: Parameters,
}

impl MailCommand {
    pub fn new(from: Option<Email>, params: Parameters) -> MailCommand {
        MailCommand { from, params }
    }
}

impl Sendable for MailCommand {
    fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        w.write_all(b"MAIL FROM:<")?;
        self.from.send_to(w)?;
        w.write_all(b">")?;
        self.params.send_to(w)?;
        w.write_all(b"\r\n")
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
    do_parse!(
        tag_no_case!("MAIL FROM:") >> eat_spaces >>
        from: alt!(
            map!(tag!("<>"), |_| None) |
            map!(address_in_maybe_bracketed_path, |x| Some(x))
        ) >>
        params: parse_parameters >> eat_spaces >>
        crlf >>
        (MailCommand { from, params })
    )
);

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::IResult;

    use crate::smtpstring::SmtpString;

    #[test]
    fn valid_command_mail_args() {
        let tests = vec![
            (&b"Mail FROM:<@one,@two:foo@bar.baz>\r\n"[..], MailCommand {
                from: Some(Email::parse_slice(b"foo@bar.baz").unwrap()),
                params: Parameters::none(),
            }),
            (&b"MaiL FrOm: quux@example.net  \t \r\n"[..], MailCommand {
                from: Some(Email::parse_slice(b"quux@example.net").unwrap()),
                params: Parameters::none(),
            }),
            (&b"mail FROM:<>\r\n"[..], MailCommand {
                from: None,
                params: Parameters::none(),
            }),
            (&b"MAIL FROM:<> hello=world foo\r\n"[..], MailCommand {
                from: None,
                params: Parameters(
                    vec![
                        ((&b"hello"[..]).into(), Some((&b"world"[..]).into())),
                        ((b"foo"[..]).into(), None),
                    ]
                    .into_iter()
                    .collect(),
                ),
            }),
        ];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            println!("Trying to parse {:?}", b);
            match command_mail_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 => assert_eq!(res, &r),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn incomplete_args() {
        let b = Bytes::from(&b"MAIL FROM:<foo@bar.com"[..]);
        assert!(command_mail_args(ByteSlice::from(&b)).is_incomplete());
        let b = Bytes::from(&b"mail from:foo@bar.com"[..]);
        assert!(command_mail_args(ByteSlice::from(&b)).is_incomplete());
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        MailCommand::new(
            Some(Email::parse_slice(b"foo@bar.baz").unwrap()),
            Parameters::none(),
        )
        .send_to(&mut v)
        .unwrap();
        assert_eq!(v, b"MAIL FROM:<foo@bar.baz>\r\n");

        let mut v = Vec::new();
        MailCommand::new(None, Parameters::none())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"MAIL FROM:<>\r\n");

        let mut v = Vec::new();
        let c = MailCommand::new(
            Some(Email::parse_slice(b"hello@world.example.org").unwrap()),
            Parameters(
                [
                    (
                        SmtpString::from_static(b"foo"),
                        Some(SmtpString::from_static(b"bar")),
                    ),
                    (SmtpString::from_static(b"baz"), None),
                    (
                        SmtpString::from_static(b"helloworld"),
                        Some(SmtpString::from_static(b"bleh")),
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            ),
        );
        c.send_to(&mut v).unwrap();
        let b = Bytes::from(v);
        let res = command_mail_args(ByteSlice::from(&b));
        println!(
            "Expecting something like \"MAIL FROM:<hello@world.example.org> foo=bar baz \
             helloworld=bleh\""
        );
        println!("Got {:?}", res);
        let r = res.unwrap().1;
        assert_eq!(c, r);
    }
}
