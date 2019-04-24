use bytes::Bytes;
use std::{
    io,
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use crate::{
    byteslice::ByteSlice,
    parseresult::{nom_to_result, ParseError},
    sendable::Sendable,
    smtpstring::SmtpString,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Domain {
    Host(SmtpString),
    Addr(IpAddr),
}

impl Domain {
    pub fn parse(domain: ByteSlice) -> Result<Domain, ParseError> {
        nom_to_result(hostname(domain))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Domain, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(hostname(ByteSlice::from(&b)))
    }
}

impl Sendable for Domain {
    fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        use self::{Domain::*, IpAddr::*};
        match self {
            Host(s) => w.write_all(&s.bytes()[..]),
            Addr(V4(a)) => write!(w, "[{}]", a),
            Addr(V6(a)) => write!(w, "[IPv6:{}]", a),
        }
    }
}

named!(pub hostname(ByteSlice) -> Domain,
    alt!(
        map!(recognize!(
                separated_nonempty_list_complete!(tag!("."),
                    preceded!(one_of!(alnum!()),
                              opt!(is_a!(concat!(alnum!(), "-")))))),
             |x| Domain::Host(SmtpString::from(x.promote()))) |
        map_res!(map_res!(preceded!(tag!("[IPv6:"), take_until_and_consume!("]")),
                          ByteSlice::into_utf8),
                 |x| -> Result<Domain, AddrParseError> {
                     Ok(Domain::Addr(Ipv6Addr::from_str(x)?.into()))
                 }) |
        map_res!(map_res!(preceded!(tag!("["), take_until_and_consume!("]")),
                          ByteSlice::into_utf8),
                 |x| -> Result<Domain, AddrParseError> {
                     Ok(Domain::Addr(Ipv4Addr::from_str(x)?.into()))
                 })
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult;

    #[test]
    fn valid_hostnames() {
        let tests: &[(&[u8], Domain)] = &[
            (
                b"foo--bar",
                Domain::Host(SmtpString::from_static(b"foo--bar")),
            ),
            (
                b"foo.bar.baz",
                Domain::Host(SmtpString::from_static(b"foo.bar.baz")),
            ),
            (
                b"1.2.3.4",
                Domain::Host(SmtpString::from_static(b"1.2.3.4")),
            ),
            (
                b"[123.255.37.2]",
                Domain::Addr(IpAddr::from_str("123.255.37.2").unwrap()),
            ),
            (
                b"[IPv6:0::ffff:8.7.6.5]",
                Domain::Addr(IpAddr::from_str("0::ffff:8.7.6.5").unwrap()),
            ),
        ];
        for (inp, out) in tests {
            let b = Bytes::from(*inp);
            let parsed = hostname(ByteSlice::from(&b));
            println!(
                "\nTest: {:?}\nParse result: {:?}\nExpected: {:?}",
                b, parsed, out
            );
            match parsed {
                IResult::Done(rem, bb) => assert!(rem.len() == 0 && &bb == out),
                x => panic!("Unexpected hostname result: {:?}", x),
            }
        }
    }

    #[test]
    fn partial_hostnames() {
        let tests: &[(&[u8], &[u8])] = &[(b"foo.-bar.baz", b"foo"), (b"foo.bar.-baz", b"foo.bar")];
        for (from, to) in tests {
            let b = Bytes::from(*from);
            assert_eq!(
                hostname(ByteSlice::from(&b)).unwrap().1,
                Domain::Host((*to).into())
            );
        }
    }

    #[test]
    fn invalid_hostnames() {
        let tests: &[&[u8]] = &[b"-foo.bar"];
        for test in tests {
            let b = Bytes::from(*test);
            let r = hostname(ByteSlice::from(&b));
            println!("{:?}", r);
            assert!(r.is_err());
        }
    }
}
