use std::io;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct VrfyCommand<'a> {
    name: SmtpString<'a>,
}

impl<'a> VrfyCommand<'a> {
    pub fn new(name: SmtpString) -> VrfyCommand {
        VrfyCommand { name }
    }

    pub fn name(&self) -> &SmtpString {
        &self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"VRFY ")?;
        w.write_all(self.name.as_bytes())?;
        w.write_all(b"\r\n")
    }

    pub fn take_ownership<'b>(self) -> VrfyCommand<'b> {
        VrfyCommand {
            name: self.name.take_ownership(),
        }
    }
}

named!(pub command_vrfy_args(&[u8]) -> VrfyCommand, do_parse!(
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (VrfyCommand {
        name: res.into(),
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_vrfy_args() {
        let tests = vec![(
            &b" \t hello.world \t \r\n"[..],
            VrfyCommand {
                name: (&b" \t hello.world \t "[..]).into(),
            },
        )];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_vrfy_args(s), IResult::Done(&b""[..], r));
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
