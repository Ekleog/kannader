use std::io;

use byteslice::ByteSlice;
use smtpstring::SmtpString;
use stupidparsers::eat_spaces;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct NoopCommand {
    string: SmtpString,
}

impl NoopCommand {
    pub fn new(string: SmtpString) -> NoopCommand {
        NoopCommand { string }
    }

    pub fn string(&self) -> &SmtpString {
        &self.string
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"NOOP ")?;
        w.write_all(&self.string.bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_noop_args(ByteSlice) -> NoopCommand, do_parse!(
    eat_spaces >>
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (NoopCommand {
        string: res.promote().into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::*;

    #[test]
    fn valid_command_noop_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                NoopCommand {
                    string: (&b"hello.world \t "[..]).into(),
                },
            ),
            (
                &b"\r\n"[..],
                NoopCommand {
                    string: (&b""[..]).into(),
                },
            ),
            (
                &b" \r\n"[..],
                NoopCommand {
                    string: (&b""[..]).into(),
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_noop_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        NoopCommand::new((&b"useless string"[..]).into())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"NOOP useless string\r\n");
    }
}
