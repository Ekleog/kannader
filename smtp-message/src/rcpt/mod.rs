use nom::crlf;
use std::io;

use crate::{
    byteslice::ByteSlice,
    email::{address_in_maybe_bracketed_path, Email},
    parameters::{parse_parameters, Parameters},
    sendable::Sendable,
    stupidparsers::eat_spaces,
};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct RcptCommand {
    pub to:     Email,
    pub params: Parameters,
}

impl RcptCommand {
    pub fn new(to: Email, params: Parameters) -> RcptCommand {
        RcptCommand { to, params }
    }
}

impl Sendable for RcptCommand {
    fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        w.write_all(b"RCPT TO:<")?;
        self.to.send_to(w)?;
        w.write_all(b">")?;
        self.params.send_to(w)?;
        w.write_all(b"\r\n")
    }
}

named!(pub command_rcpt_args(ByteSlice) -> RcptCommand,
    do_parse!(
        tag_no_case!("RCPT TO:") >> eat_spaces >>
        to: address_in_maybe_bracketed_path >>
        params: parse_parameters >> eat_spaces >>
        crlf >>
        (RcptCommand { to, params })
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    use crate::domain::Domain;

    #[test]
    fn valid_command_rcpt_args() {
        let tests: Vec<(&[u8], &[u8], Option<&[u8]>)> = vec![
            (
                b"RCPT TO:<@one,@two:foo@bar.baz>\r\n",
                b"foo",
                Some(b"bar.baz"),
            ),
            (
                b"Rcpt tO: quux@example.net  \t \r\n",
                b"quux",
                Some(b"example.net"),
            ),
            (b"rcpt TO:<Postmaster>\r\n", b"Postmaster", None),
            (b"RcPt TO: \t poStmaster\r\n", b"poStmaster", None),
        ];
        for (s, l, h) in tests.into_iter() {
            let b = Bytes::from(s);
            println!("About to parse {:?}", b);
            let res = command_rcpt_args(ByteSlice::from(&b)).unwrap().1;
            assert_eq!(res.to.raw_localpart().bytes(), l);
            assert_eq!(
                res.to.hostname(),
                &h.map(|x| Domain::parse_slice(x).unwrap())
            );
        }
    }

    #[test]
    fn valid_build() {
        let mut v = Vec::new();
        RcptCommand::new(
            Email::new(
                (&b"foo"[..]).into(),
                Some(Domain::parse_slice(b"bar.com").unwrap()),
            ),
            Parameters::none(),
        )
        .send_to(&mut v)
        .unwrap();
        assert_eq!(v, b"RCPT TO:<foo@bar.com>\r\n");

        v = Vec::new();
        RcptCommand::new(
            Email::new((&b"Postmaster"[..]).into(), None),
            Parameters::none(),
        )
        .send_to(&mut v)
        .unwrap();
        assert_eq!(v, b"RCPT TO:<Postmaster>\r\n");
    }
}
