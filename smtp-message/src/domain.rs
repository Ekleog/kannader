use bytes::Bytes;
use std::ops::Deref;

use byteslice::ByteSlice;
use parseresult::{nom_to_result, ParseError};
use smtpstring::SmtpString;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct Domain(SmtpString); // TODO: split between IP and DNS

impl Domain {
    pub fn new(domain: ByteSlice) -> Result<Domain, ParseError> {
        nom_to_result(hostname(domain))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Domain, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(hostname(ByteSlice::from(&b)))
    }

    pub fn as_string(&self) -> &SmtpString {
        &self.0
    }
}

impl Deref for Domain {
    type Target = SmtpString;

    fn deref(&self) -> &SmtpString {
        &self.0
    }
}

pub fn new_domain_unchecked(s: SmtpString) -> Domain {
    Domain(s)
}

named!(pub hostname(ByteSlice) -> Domain,
    alt!(
        map!(recognize!(preceded!(tag!("["), take_until_and_consume!("]"))),
             |x| new_domain_unchecked(SmtpString::from(x.promote()))) |
        map!(recognize!(
                separated_nonempty_list_complete!(tag!("."),
                    preceded!(one_of!(alnum!()),
                              opt!(is_a!(concat!(alnum!(), "-")))))),
             |x| new_domain_unchecked(SmtpString::from(x.promote())))
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult;

    #[test]
    fn valid_hostnames() {
        let tests = &[
            &b"foo--bar"[..],
            &b"foo.bar.baz"[..],
            &b"1.2.3.4"[..],
            &b"[123.255.37.2]"[..],
            &b"[IPv6:0::ffff:8.7.6.5]"[..],
        ];
        for test in tests {
            let b = Bytes::from(*test);
            let parsed = hostname(ByteSlice::from(&b)).map(|x| x.as_string().bytes().clone());
            println!("Test: {:?}, parse result: {:?}", b, parsed);
            match parsed {
                IResult::Done(rem, bb) => assert!(rem.len() == 0 && *bb == b),
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
                new_domain_unchecked((*to).into())
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
