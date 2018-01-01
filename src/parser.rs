use nom::crlf;

use ::{Command, DataCommand, EhloCommand, HeloCommand};
use data::*;
use mail::*;
use rcpt::*;
use parse_helpers::*;

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
    fn valid_command() {
        let tests: Vec<(&[u8], Box<fn(Command) -> bool>)> = vec![
            (&b"DATA\r\nhello world\r\n.. me\r\n.\r\n"[..], Box::new(
                |x| if let Command::Data(r) = x { r.raw_data() == b"hello world\r\n.. me\r\n" }
                    else { false }
            )),
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
                |x| if let Command::Rcpt(r) = x { r.to() == b"foo@bar.baz" }
                    else { false }
            )),
            (&b"RCPT to:<@foo.bar,@bar.baz:baz@quux.foo>\r\n"[..], Box::new(
                |x| if let Command::Rcpt(r) = x { r.to() == b"baz@quux.foo" }
                    else { false }
            )),
        ];
        for (s, r) in tests.into_iter() {
            assert!(r(command(s).unwrap().1));
        }
    }
}
