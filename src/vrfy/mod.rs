use std::fmt;

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

    pub fn build(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(self.name.len() + 2);
        res.extend_from_slice(self.name);
        res.extend_from_slice(b"\r\n");
        res
    }
}

impl<'a> fmt::Debug for VrfyCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "VrfyCommand {{ name: {} }}", bytes_to_dbg(self.name))
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
            (&b" \t hello.world \t \r\n"[..], VrfyCommand {
                name: &b" \t hello.world \t "[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_vrfy_args(s), IResult::Done(&b""[..], r));
        }
    }
}
