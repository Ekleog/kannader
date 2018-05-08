use byteslice::ByteSlice;
use helpers::*;

// TODO: This file should not exist. Every function should find a better home.

macro_rules! alpha_lower {
    () => {
        "abcdefghijklmnopqrstuvwxyz"
    };
}
macro_rules! alpha_upper {
    () => {
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    };
}
macro_rules! alpha {
    () => {
        concat!(alpha_lower!(), alpha_upper!())
    };
}
macro_rules! digit {
    () => {
        "0123456789"
    };
}
macro_rules! alnum {
    () => {
        concat!(alpha!(), digit!())
    };
}
macro_rules! atext {
    () => {
        concat!(alnum!(), "!#$%&'*+-/=?^_`{|}~")
    };
}
macro_rules! alnumdash {
    () => {
        concat!(alnum!(), "-")
    };
}
macro_rules! graph_except_equ {
    () => {
        concat!(alnum!(), "!\"#$%&'()*+,-./:;<>?@[\\]^_`{|}~")
    };
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

named!(dot_string(ByteSlice) -> ByteSlice, recognize!(
    separated_nonempty_list_complete!(tag!("."), is_a!(atext!()))
));

// See RFC 5321 § 4.1.2
named!(quoted_string(ByteSlice) -> ByteSlice, recognize!(do_parse!(
    tag!("\"") >>
    many0!(alt!(
        preceded!(tag!("\\"), verify!(take!(1), |x: ByteSlice| 32 <= x[0] && x[0] <= 126)) |
        verify!(take!(1), |x: ByteSlice| 32 <= x[0] && x[0] != 34 && x[0] != 92 && x[0] <= 126)
    )) >>
    tag!("\"") >>
    ()
)));

named!(localpart(ByteSlice) -> ByteSlice, alt!(quoted_string | dot_string));

named!(pub email(ByteSlice) -> Email, do_parse!(
    local: localpart >>
    host: opt!(complete!(preceded!(tag!("@"), hostname))) >>
    (Email::new(local.promote().into(), host))
));

named!(address_in_path(ByteSlice) -> Email, preceded!(
    opt!(do_parse!(
        separated_list!(tag!(","), do_parse!(tag!("@") >> hostname >> ())) >>
        tag!(":") >>
        ()
    )),
    email
));

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

named!(pub eat_spaces(ByteSlice) -> ByteSlice, eat_separator!(" \t"));

named!(pub sp_parameters(ByteSlice) -> SpParameters, do_parse!(
    params: separated_nonempty_list_complete!(
        do_parse!(
            many1!(one_of!(" \t")) >>
            tag!("SP") >>
            many1!(one_of!(" \t")) >>
            ()
        ),
        do_parse!(
            eat_spaces >>
            key: recognize!(preceded!(one_of!(alnum!()), opt!(is_a!(alnumdash!())))) >>
            value: opt!(complete!(preceded!(tag!("="), is_a!(graph_except_equ!())))) >>
            (key.promote().into(), value.map(|x| x.promote().into()))
        )
    ) >>
    (SpParameters(params.into_iter().collect()))
));

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use nom::*;
    use std::collections::HashMap;

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

    #[test]
    fn valid_dot_strings() {
        let tests: &[&[u8]] = &[b"helloooo", b"test.ing"];
        for test in tests {
            let b = Bytes::from(*test);
            match dot_string(ByteSlice::from(&b)) {
                IResult::Done(rem, res) if rem.len() == 0 && res.promote() == b => (),
                x => panic!("Unexpected dot_string result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_quoted_strings() {
        let tests: &[&[u8]] = &[
            br#""hello""#,
            br#""hello world. This |$ a g#eat place to experiment !""#,
            br#""\"escapes\", useless like h\ere, except for quotes and \\backslashes""#,
        ];
        for test in tests {
            let b = Bytes::from(*test);
            match quoted_string(ByteSlice::from(&b)) {
                IResult::Done(rem, res) if rem.len() == 0 && res.promote() == b => (),
                x => panic!("Unexpected quoted_string result: {:?}", x),
            }
        }
    }

    #[test]
    fn valid_emails() {
        let tests: Vec<(&[u8], (&[u8], Option<&[u8]>))> = vec![
            (
                b"t+e-s.t_i+n-g@foo.bar.baz",
                (b"t+e-s.t_i+n-g", Some(b"foo.bar.baz")),
            ),
            (
                br#""quoted\"example"@example.org"#,
                (br#""quoted\"example""#, Some(b"example.org")),
            ),
            (b"postmaster", (b"postmaster", None)),
            (b"test", (b"test", None)),
        ];
        for (s, (l, h)) in tests.into_iter() {
            let b = Bytes::from(s);
            let r = Email::new(
                SmtpString::from(l),
                h.map(|x| new_domain_unchecked(SmtpString::from(x))),
            );
            match email(ByteSlice::from(&b)) {
                IResult::Done(rem, ref res) if rem.len() == 0 && res == &r => (),
                x => panic!("Unexpected quoted_string result: {:?}", x),
            }
        }
    }

    #[test]
    fn nice_localpart() {
        let tests: Vec<(&[u8], &[u8])> = vec![
            (b"t+e-s.t_i+n-g@foo.bar.baz ", b"t+e-s.t_i+n-g"),
            (br#""quoted\"example"@example.org "#, br#"quoted"example"#),
            (
                br#""escaped\\exa\mple"@example.org "#,
                br#"escaped\example"#,
            ),
        ];
        for (s, r) in tests {
            let b = Bytes::from(s);
            assert_eq!(email(ByteSlice::from(&b)).unwrap().1.localpart().bytes(), r);
        }
    }

    #[test]
    fn invalid_localpart() {
        let b = Bytes::from_static(b"@foo.bar");
        assert!(email(ByteSlice::from(&b)).is_err());
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests: Vec<(&[u8], (&[u8], &[u8]))> = vec![
            (
                b"@foo.bar,@baz.quux:test@example.org",
                (b"test", b"example.org"),
            ),
            (b"foo.bar@baz.quux", (b"foo.bar", b"baz.quux")),
        ];
        for (inp, (local, host)) in tests.into_iter() {
            let b = Bytes::from(inp);
            match address_in_path(ByteSlice::from(&b)) {
                IResult::Done(rem, res) => assert!(
                    rem.len() == 0 && res.raw_localpart().bytes() == local
                        && res.hostname().clone().unwrap().as_string().bytes().clone() == host
                ),
                x => panic!("Unexpected address_in_path result: {:?}", x),
            }
        }
    }

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
                host.map(|x| new_domain_unchecked(SmtpString::from(Bytes::from(x))))
            );
        }
    }

    #[test]
    fn valid_sp_parameters() {
        let tests: &[(&[u8], &[(&[u8], Option<&[u8]>)])] = &[
            (b"key=value", &[(b"key", Some(b"value"))]),
            (
                b"key=value SP key2=value2",
                &[(b"key", Some(b"value")), (b"key2", Some(b"value2"))],
            ),
            (
                b"KeY2=V4\"l\\u@e.z\tSP\t0tterkeyz=very_muchWh4t3ver",
                &[
                    (b"KeY2", Some(b"V4\"l\\u@e.z")),
                    (b"0tterkeyz", Some(b"very_muchWh4t3ver")),
                ],
            ),
            (b"NoValueKey", &[(b"NoValueKey", None)]),
            (b"A SP B", &[(b"A", None), (b"B", None)]),
            (
                b"A=B SP C SP D=SP",
                &[(b"A", Some(b"B")), (b"C", None), (b"D", Some(b"SP"))],
            ),
        ];
        for (inp, out) in tests {
            let b = Bytes::from(*inp);
            let res = sp_parameters(ByteSlice::from(&b));
            let (rem, res) = res.unwrap();
            assert_eq!(&rem[..], b"");
            let res_reference = out.iter()
                .map(|(a, b)| ((*a).into(), b.map(|x| x.into())))
                .collect::<HashMap<_, _>>();
            assert_eq!(res.0, res_reference);
        }
    }
}
