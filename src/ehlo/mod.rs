use std::fmt;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct EhloCommand<'a> {
    domain: &'a [u8],
}

impl<'a> EhloCommand<'a> {
    pub fn new<'b>(domain: &'b [u8]) -> EhloCommand<'b> {
        EhloCommand { domain }
    }

    pub fn domain(&self) -> &'a [u8] {
        self.domain
    }

    pub fn build(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(self.domain.len() + 2);
        res.extend_from_slice(self.domain);
        res.extend_from_slice(b"\r\n");
        res
    }
}

impl<'a> fmt::Debug for EhloCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "EhloCommand {{ domain: {} }}", bytes_to_dbg(self.domain))
    }
}

named!(pub command_ehlo_args(&[u8]) -> EhloCommand,
    sep!(eat_spaces, do_parse!(
        domain: hostname >>
        tag!("\r\n") >>
        (EhloCommand {
            domain: domain
        })
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

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
    fn valid_build() {
        assert_eq!(EhloCommand::new(b"test.foo.bar").build(), b"test.foo.bar\r\n");
    }
}
