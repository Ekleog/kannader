use ::Command;
use data::*;
use ehlo::*;
use helo::*;
use mail::*;
use rcpt::*;

named!(pub command(&[u8]) -> Command, alt!(
    map!(preceded!(tag_no_case!("DATA"), command_data_args), Command::Data) |
    map!(preceded!(tag_no_case!("EHLO "), command_ehlo_args), Command::Ehlo) |
    map!(preceded!(tag_no_case!("HELO "), command_helo_args), Command::Helo) |
    map!(preceded!(tag_no_case!("MAIL "), command_mail_args), Command::Mail) |
    map!(preceded!(tag_no_case!("RCPT "), command_rcpt_args), Command::Rcpt)
));

#[cfg(test)]
mod tests {
    use parser::*;

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
        ];
        for (s, r) in tests.into_iter() {
            assert!(r(command(s).unwrap().1));
        }
    }
}
