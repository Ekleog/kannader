use std::io;
use nom::IResult;

use helpers::*;

use data::*;
use ehlo::*;
use expn::*;
use helo::*;
use help::*;
use mail::*;
use noop::*;
use quit::*;
use rcpt::*;
use rset::*;
use vrfy::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub enum Command<'a> {
    Data(DataCommand<'a>), // DATA <CRLF>
    Ehlo(EhloCommand<'a>), // EHLO <domain> <CRLF>
    Expn(ExpnCommand<'a>), // EXPN <name> <CRLF>
    Helo(HeloCommand<'a>), // HELO <domain> <CRLF>
    Help(HelpCommand<'a>), // HELP [<subject>] <CRLF>
    Mail(MailCommand<'a>), // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Noop(NoopCommand<'a>), // NOOP [<string>] <CRLF>
    Quit(QuitCommand),     // QUIT <CRLF>
    Rcpt(RcptCommand<'a>), // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Rset(RsetCommand),     // RSET <CRLF>
    Vrfy(VrfyCommand<'a>), // VRFY <name> <CRLF>
}

impl<'a> Command<'a> {
    // TODO: think about actually relinquishing borrow over `arg` instead of just returning the
    // remaining part
    pub fn parse(arg: &[u8]) -> Result<(Command, &[u8]), ParseError> {
        match command(arg) {
            IResult::Done(rem, res)  => Ok((res, rem)),
            IResult::Error(e)        => Err(ParseError::ParseError(e)),
            IResult::Incomplete(n)   => Err(ParseError::IncompleteString(n)),
        }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        match self {
            &Command::Data(ref c) => c.send_to(w),
            &Command::Ehlo(ref c) => c.send_to(w),
            &Command::Expn(ref c) => c.send_to(w),
            &Command::Helo(ref c) => c.send_to(w),
            &Command::Help(ref c) => c.send_to(w),
            &Command::Mail(ref c) => c.send_to(w),
            &Command::Noop(ref c) => c.send_to(w),
            &Command::Quit(ref c) => c.send_to(w),
            &Command::Rcpt(ref c) => c.send_to(w),
            &Command::Rset(ref c) => c.send_to(w),
            &Command::Vrfy(ref c) => c.send_to(w),
        }
    }
}

named!(pub command(&[u8]) -> Command, alt!(
    map!(preceded!(tag_no_case!("DATA"), command_data_args), Command::Data) |
    map!(preceded!(tag_no_case!("EHLO "), command_ehlo_args), Command::Ehlo) |
    map!(preceded!(tag_no_case!("EXPN "), command_expn_args), Command::Expn) |
    map!(preceded!(tag_no_case!("HELO "), command_helo_args), Command::Helo) |
    map!(preceded!(tag_no_case!("HELP"), command_help_args), Command::Help) |
    map!(preceded!(tag_no_case!("MAIL "), command_mail_args), Command::Mail) |
    map!(preceded!(tag_no_case!("NOOP"), command_noop_args), Command::Noop) |
    map!(preceded!(tag_no_case!("QUIT"), command_quit_args), Command::Quit) |
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
            (&b"HELP foo.bar.baz\r\n"[..], Box::new(
                |x| if let Command::Help(r) = x { r.subject() == b"foo.bar.baz" }
                    else { false }
            )),
            (&b"HELP \r\n"[..], Box::new(
                |x| if let Command::Help(r) = x { r.subject() == b"" }
                    else { false }
            )),
            (&b"HELP\r\n"[..], Box::new(
                |x| if let Command::Help(r) = x { r.subject() == b"" }
                    else { false }
            )),
            (&b"MAIL FROM:<hello@world.example>\r\n"[..], Box::new(
                |x| if let Command::Mail(r) = x { r.raw_from() == b"hello@world.example" }
                    else { false }
            )),
            (&b"NOOP\r\n"[..], Box::new(
                |x| if let Command::Noop(_) = x { true }
                    else { false }
            )),
            (&b"QUIT\r\n"[..], Box::new(
                |x| if let Command::Quit(_) = x { true }
                    else { false }
            )),
            (&b"rCpT To: foo@bar.baz\r\n"[..], Box::new(
                |x| if let Command::Rcpt(r) = x {
                        r.to().raw_localpart() == b"foo" &&
                        r.to().hostname() == Some(b"bar.baz")
                    } else { false }
            )),
            (&b"RCPT to:<@foo.bar,@bar.baz:baz@quux.foo>\r\n"[..], Box::new(
                |x| if let Command::Rcpt(r) = x {
                        r.to().raw_localpart() == b"baz" &&
                        r.to().hostname() == Some(b"quux.foo")
                    } else { false }
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

    #[test]
    pub fn do_send_ok() {
        let mut v = Vec::new();
        Command::Vrfy(VrfyCommand::new(b"fubar")).send_to(&mut v).unwrap();
        assert_eq!(v, b"VRFY fubar\r\n");
    }
}
