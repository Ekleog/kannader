use std::{fmt, io};

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct VrfyCommand<'a> {
    name: &'a [u8],
}

impl<'a> VrfyCommand<'a> {
    pub fn new(name: &[u8]) -> VrfyCommand {
        VrfyCommand { name }
    }

    pub fn name(&self) -> &'a [u8] {
        self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"VRFY ")?;
        w.write_all(self.name)?;
        w.write_all(b"\r\n")
    }
}

impl<'a> fmt::Debug for VrfyCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "VrfyCommand {{ name: {:?} }}", bytes_to_dbg(self.name))
    }
}

named!(pub command_vrfy_args(&[u8]) -> VrfyCommand, do_parse!(
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (VrfyCommand {
        name: res,
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_vrfy_args() {
        let tests = vec![
            (
                &b" \t hello.world \t \r\n"[..],
                VrfyCommand { name: &b" \t hello.world \t "[..] }
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_vrfy_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_send_to() {
        let mut v = Vec::new();
        VrfyCommand::new(b"postmaster").send_to(&mut v).unwrap();
        assert_eq!(v, b"VRFY postmaster\r\n");
    }
}
