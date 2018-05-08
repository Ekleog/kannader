use std::io;

use byteslice::ByteSlice;
use smtpstring::SmtpString;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct VrfyCommand {
    name: SmtpString,
}

impl VrfyCommand {
    pub fn new(name: SmtpString) -> VrfyCommand {
        VrfyCommand { name }
    }

    pub fn name(&self) -> &SmtpString {
        &self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"VRFY ")?;
        w.write_all(&self.name.bytes()[..])?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_vrfy_args(ByteSlice) -> VrfyCommand, do_parse!(
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (VrfyCommand {
        name: res.promote().into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::IResult;

    #[test]
    fn valid_command_vrfy_args() {
        let tests = vec![(
            &b" \t hello.world \t \r\n"[..],
            VrfyCommand {
                name: (&b" \t hello.world \t "[..]).into(),
            },
        )];
        for (s, r) in tests.into_iter() {
            let b = Bytes::from(s);
            match command_vrfy_args(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        VrfyCommand::new((&b"postmaster"[..]).into())
            .send_to(&mut v)
            .unwrap();
        assert_eq!(v, b"VRFY postmaster\r\n");
    }
}
