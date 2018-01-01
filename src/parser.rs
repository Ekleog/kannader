use ::{Command, DataCommand, EhloCommand, MailCommand, RcptCommand};

use nom::crlf;

macro_rules! alpha_lower { () => ("abcdefghijklmnopqrstuvwxyz") }
macro_rules! alpha_upper { () => ("ABCDEFGHIJKLMNOPQRSTUVWXYZ") }
macro_rules! alpha       { () => (concat!(alpha_lower!(), alpha_upper!())) }
macro_rules! digit       { () => ("0123456789") }
macro_rules! alnum       { () => (concat!(alpha!(), digit!())) }

named!(hostname(&[u8]) -> &[u8], recognize!(
    separated_list_complete!(tag!("."), is_a!(concat!(alnum!(), "-")))
));

named!(email(&[u8]) -> &[u8], recognize!(do_parse!(
    take_until_and_consume!("@") >> hostname >> ()
)));

named!(address_in_path(&[u8]) -> &[u8], do_parse!(
    opt!(do_parse!(
        separated_list!(tag!(","), do_parse!(tag!("@") >> hostname >> ())) >>
        tag!(":") >>
        ()
    )) >>
    res: email >>
    (res)
));

named!(address_in_maybe_bracketed_path(&[u8]) -> &[u8],
    alt!(
        do_parse!(
            tag!("<") >>
            addr: address_in_path >>
            tag!(">") >>
            (addr)
        ) |
        address_in_path
    )
);

named!(full_maybe_bracketed_path(&[u8]) -> &[u8], recognize!(address_in_maybe_bracketed_path));

named!(eat_spaces, eat_separator!(" \t"));

named!(command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    data: recognize!(do_parse!(
        take_until!("\r\n.\r\n") >>
        tag!("\r\n") >>
        ()
    )) >>
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

named!(command_mail_args(&[u8]) -> MailCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("FROM:") >> from: full_maybe_bracketed_path >>
        // TODO: support the SP arguments
        crlf >>
        (MailCommand {
            from: from,
        })
    ))
);

named!(command_rcpt_args(&[u8]) -> RcptCommand,
    sep!(eat_spaces, do_parse!(
        tag_no_case!("TO:") >> to: address_in_maybe_bracketed_path >>
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
    map!(preceded!(tag_no_case!("MAIL "), command_mail_args), Command::Mail) |
    map!(preceded!(tag_no_case!("RCPT "), command_rcpt_args), Command::Rcpt)
));

#[cfg(test)]
mod tests {
    use nom::*;
    use parser::*;

    #[test]
    fn valid_hostnames() {
        let tests = &[
            &b"foo--bar"[..],
            &b"foo.bar.baz"[..],
        ];
        for test in tests {
            assert_eq!(hostname(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_emails() {
        let tests = &[
            &b"t+e-s.t_i+n-g@foo.bar.baz"[..],
        ];
        for test in tests {
            assert_eq!(email(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests = &[
            (&b"@foo.bar,@baz.quux:test@example.org"[..], &b"test@example.org"[..]),
            (&b"foo.bar@baz.quux"[..], &b"foo.bar@baz.quux"[..]),
        ];
        for test in tests {
            assert_eq!(address_in_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_addresses_in_maybe_bracketed_paths() {
        let tests = &[
            (&b"@foo.bar,@baz.quux:test@example.org"[..], &b"test@example.org"[..]),
            (&b"<@foo.bar,@baz.quux:test@example.org>"[..], &b"test@example.org"[..]),
            (&b"<foo@bar.baz>"[..], &b"foo@bar.baz"[..]),
            (&b"foo@bar.baz"[..], &b"foo@bar.baz"[..]),
        ];
        for test in tests {
            assert_eq!(address_in_maybe_bracketed_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_full_maybe_bracketed_paths() {
        let tests = &[
            &b"@foo.bar,@baz.quux:test@example.org"[..],
            &b"<@foo.bar,@baz.quux:test@example.org>"[..],
            &b"foo@bar.baz"[..],
            &b"<foo@bar.baz>"[..],
        ];
        for test in tests {
            assert_eq!(full_maybe_bracketed_path(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_command_data_args() {
        assert_eq!(command_data_args(b"  \r\nhello\r\nworld\r\n..\r\n.\r\n"),
                   IResult::Done(&b""[..], DataCommand {
                       data: &b"hello\r\nworld\r\n..\r\n"[..]
                   }));
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
    fn valid_command_mail_args() {
        let tests = vec![
            (&b" FROM:<@one,@two:foo@bar.baz>\r\n"[..], MailCommand {
                from: &b"<@one,@two:foo@bar.baz>"[..],
            }),
            (&b"FrOm: quux@example.net  \t \r\n"[..], MailCommand {
                from: &b"quux@example.net"[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_mail_args(s), IResult::Done(&b""[..], r));
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
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_rcpt_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_command() {
        let tests = vec![
            (&b"DATA\r\nhello world\r\n.. me\r\n.\r\n"[..], Command::Data(DataCommand {
                data: &b"hello world\r\n.. me\r\n"[..],
            })),
            (&b"EHLO foo.bar.baz\r\n"[..], Command::Ehlo(EhloCommand {
                domain: &b"foo.bar.baz"[..],
            })),
            (&b"MAIL FROM:<hello@world.example>\r\n"[..], Command::Mail(MailCommand {
                from: &b"<hello@world.example>"[..],
            })),
            (&b"rCpT To: foo@bar.baz\r\n"[..], Command::Rcpt(RcptCommand {
                to: &b"foo@bar.baz"[..],
            })),
            (&b"RCPT to:<@foo.bar,@bar.baz:baz@quux.foo>\r\n"[..], Command::Rcpt(RcptCommand {
                to: &b"baz@quux.foo"[..],
            })),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command(s), IResult::Done(&b""[..], r));
        }
    }
}
