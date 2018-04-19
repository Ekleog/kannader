use std::{fmt, io};

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct ExpnCommand<'a> {
    name: &'a [u8],
}

impl<'a> ExpnCommand<'a> {
    pub fn new(name: &[u8]) -> ExpnCommand {
        ExpnCommand { name }
    }

    pub fn name(&self) -> &'a [u8] {
        self.name
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"EXPN ")?;
        w.write_all(self.name)?;
        w.write_all(b"\r\n")
    }
}

impl<'a> fmt::Debug for ExpnCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "ExpnCommand {{ name: {:?} }}", bytes_to_dbg(self.name))
    }
}

named!(pub command_expn_args(&[u8]) -> ExpnCommand, do_parse!(
    res: take_until!("\r\n") >>
    tag!("\r\n") >>
    (ExpnCommand {
        name: res,
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_expn_args() {
        let tests = vec![(
            &b" \t hello.world \t \r\n"[..],
            ExpnCommand {
                name: &b" \t hello.world \t "[..],
            },
        )];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_expn_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        ExpnCommand::new(b"foobar").send_to(&mut v).unwrap();
        assert_eq!(v, b"EXPN foobar\r\n");
    }
}
