use ::*;
use data::command_data_args;
use ehlo::command_ehlo_args;
use expn::command_expn_args;
use helo::command_helo_args;
use mail::command_mail_args;
use rcpt::command_rcpt_args;
use rset::command_rset_args;
use vrfy::command_vrfy_args;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub enum Command<'a> {
    Data(DataCommand<'a>), // DATA <CRLF>
    Ehlo(EhloCommand<'a>), // EHLO <domain> <CRLF>
    Expn(ExpnCommand<'a>), // EXPN <name> <CRLF>
    Helo(HeloCommand<'a>), // HELO <domain> <CRLF>
    Mail(MailCommand<'a>), // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Rcpt(RcptCommand<'a>), // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Rset(RsetCommand),     // RSET <CRLF>
    Vrfy(VrfyCommand<'a>), // VRFY <name> <CRLF>
}

named!(pub command(&[u8]) -> Command, alt!(
    map!(preceded!(tag_no_case!("DATA"), command_data_args), Command::Data) |
    map!(preceded!(tag_no_case!("EHLO "), command_ehlo_args), Command::Ehlo) |
    map!(preceded!(tag_no_case!("EXPN "), command_expn_args), Command::Expn) |
    map!(preceded!(tag_no_case!("HELO "), command_helo_args), Command::Helo) |
    map!(preceded!(tag_no_case!("MAIL "), command_mail_args), Command::Mail) |
    map!(preceded!(tag_no_case!("RCPT "), command_rcpt_args), Command::Rcpt) |
    map!(preceded!(tag_no_case!("RSET"), command_rset_args), Command::Rset) |
    map!(preceded!(tag_no_case!("VRFY "), command_vrfy_args), Command::Vrfy)
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_command() {
        let tests: Vec<(&[u8], Box<fn(Command) -> bool>)> = vec![
            (&b"DATA\r\nhello world\r\n.. me\r\n.\r\n"[..], Box::new(
                |x| if let Command::Data(r) = x { r.raw_data() == b"hello world\r\n.. me\r\n" }
                    else { false }
            )),
            (&b"EHLO foo.bar.baz\r\n"[..], Box::new(
                |x| if let Command::Ehlo(r) = x { r.domain() == b"foo.bar.baz" }
                    else { false }
            )),
            (&b"EXPN mailing.list \r\n"[..], Box::new(
                |x| if let Command::Expn(r) = x { r.name() == b"mailing.list " }
                    else { false }
            )),
            (&b"HELO foo.bar.baz\r\n"[..], Box::new(
                |x| if let Command::Helo(r) = x { r.domain() == b"foo.bar.baz" }
                    else { false }
            )),
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
            (&b"RSET\r\n"[..], Box::new(
                |x| if let Command::Rset(_) = x { true }
                    else { false }
            )),
            (&b"RsEt \t \r\n"[..], Box::new(
                |x| if let Command::Rset(_) = x { true }
                    else { false }
            )),
            (&b"VRFY  root\r\n"[..], Box::new(
                |x| if let Command::Vrfy(r) = x { r.name() == b" root" }
                    else { false }
            )),
        ];
        for (s, r) in tests.into_iter() {
            assert!(r(command(s).unwrap().1));
        }
    }
}
