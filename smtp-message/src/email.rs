use bytes::Bytes;
use std::io;

use crate::{
    byteslice::ByteSlice,
    domain::{hostname, Domain},
    parseresult::{nom_to_result, ParseError},
    sendable::Sendable,
    smtpstring::SmtpString,
};

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Email {
    localpart: SmtpString,
    hostname: Option<Domain>,
}

named!(pub address_in_maybe_bracketed_path(ByteSlice) -> Email,
    alt!(
        do_parse!(
            tag!("<") >>
            addr: address_in_path >>
            tag!(">") >>
            (addr)
        ) |
        address_in_path
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult;

    #[test]
    fn valid_addresses_in_maybe_bracketed_paths() {
        let tests: &[(&[u8], (&[u8], Option<&[u8]>))] = &[
            (
                b"@foo.bar,@baz.quux:test@example.org",
                (b"test", Some(b"example.org")),
            ),
            (
                b"<@foo.bar,@baz.quux:test@example.org>",
                (b"test", Some(b"example.org")),
            ),
            (b"<foo@bar.baz>", (b"foo", Some(b"bar.baz"))),
            (b"foo@bar.baz", (b"foo", Some(b"bar.baz"))),
            (b"foobar", (b"foobar", None)),
        ];
        for (inp, (local, host)) in tests {
            let b = Bytes::from(*inp);
            let res = match address_in_maybe_bracketed_path(ByteSlice::from(&b)) {
                IResult::Done(rem, res) => {
                    assert!(rem.len() == 0);
                    res
                }
                x => panic!("Didn't parse address_in_maybe_bracketed_path: {:?}", x),
            };
            assert_eq!(res.raw_localpart().bytes(), local);
            assert_eq!(
                res.hostname().clone().map(|h| h.clone()),
                host.map(|x| Domain::parse_slice(x).unwrap())
            );
        }
    }
}
