use nom::crlf;

use ::{Command, DataCommand, EhloCommand, HeloCommand, RcptCommand};
use mail::*;
use parse_helpers::*;

named!(command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    data: alt!(
        map!(peek!(tag!(".\r\n")), |_| &b""[..]) |
        recognize!(do_parse!(
            take_until!("\r\n.\r\n") >>
            tag!("\r\n") >>
            ()
        ))
    ) >>
    tag!(".\r\n") >>
    (DataCommand {
        data: data,
    })
));

named!(command_ehlo_args(&[u8]) -> EhloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (EhloCommand {
            domain: domain
        })
    ))
);

named!(command_helo_args(&[u8]) -> HeloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (HeloCommand {
            domain: domain
        })
    ))
);

named!(command_rcpt_args(&[u8]) -> RcptCommand,
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

named!(pub command(&[u8]) -> Command, alt!(
    map!(preceded!(tag_no_case!("DATA"), command_data_args), Command::Data) |
    map!(preceded!(tag_no_case!("EHLO "), command_ehlo_args), Command::Ehlo) |
    map!(preceded!(tag_no_case!("HELO "), command_helo_args), Command::Helo) |
    map!(preceded!(tag_no_case!("MAIL "), command_mail_args), Command::Mail) |
    map!(preceded!(tag_no_case!("RCPT "), command_rcpt_args), Command::Rcpt)
));

#[cfg(test)]
mod tests {
    use nom::*;
    use parser::*;

    #[test]
    fn valid_command_data_args() {
        let tests = vec![
            (&b"  \r\nhello\r\nworld\r\n..\r\n.\r\n"[..], DataCommand {
                data: &b"hello\r\nworld\r\n..\r\n"[..],
            }),
            (&b" \t \r\n.\r\n"[..], DataCommand {
                data: &b""[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_data_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_command_ehlo_args() {
        let tests = vec![
            (&b" \t hello.world \t \r\n"[..], EhloCommand {
                domain: &b"hello.world"[..],
            }),
            (&b"hello.world\r\n"[..], EhloCommand {
                domain: &b"hello.world"[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_ehlo_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_command_helo_args() {
        let tests = vec![
            (&b" \t hello.world \t \r\n"[..], HeloCommand {
                domain: &b"hello.world"[..],
            }),
            (&b"hello.world\r\n"[..], HeloCommand {
                domain: &b"hello.world"[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_helo_args(s), IResult::Done(&b""[..], r));
        }
    }

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

    #[test]
    fn valid_command() {
        let tests: Vec<(&[u8], Box<fn(Command) -> bool>)> = vec![
            (&b"DATA\r\nhello world\r\n.. me\r\n.\r\n"[..], Box::new(
                |x| x == Command::Data(DataCommand {
                    data: &b"hello world\r\n.. me\r\n"[..],
                }))
            ),
            (&b"EHLO foo.bar.baz\r\n"[..], Box::new(
                |x| x == Command::Ehlo(EhloCommand {
                    domain: &b"foo.bar.baz"[..],
                }))
            ),
            (&b"HELO foo.bar.baz\r\n"[..], Box::new(
                |x| x == Command::Helo(HeloCommand {
                    domain: &b"foo.bar.baz"[..],
                }))
            ),
            (&b"MAIL FROM:<hello@world.example>\r\n"[..], Box::new(
                |x| if let Command::Mail(r) = x { r.raw_from() == b"<hello@world.example>" }
                    else { false }
            )),
            (&b"rCpT To: foo@bar.baz\r\n"[..], Box::new(
                |x| x == Command::Rcpt(RcptCommand {
                    to: &b"foo@bar.baz"[..],
                }))
            ),
            (&b"RCPT to:<@foo.bar,@bar.baz:baz@quux.foo>\r\n"[..], Box::new(
                |x| x == Command::Rcpt(RcptCommand {
                    to: &b"baz@quux.foo"[..],
                }))
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert!(r(command(s).unwrap().1));
        }
    }
}
