use std::collections::HashMap;

use helpers::*;

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

// TODO: move to helpers.rs
#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct Email {
    localpart: SmtpString,
    hostname:  Option<SmtpString>,
}

impl Email {
    pub fn new<'b>(localpart: SmtpString, hostname: Option<SmtpString>) -> Email {
        Email {
            localpart,
            hostname,
        }
    }

    pub fn raw_localpart(&self) -> &SmtpString {
        &self.localpart
    }

    // Note: this may contain unexpected characters, check RFC5321 / RFC5322 for
    // details.
    // This is a canonicalized version of the potentially quoted localpart, not
    // designed to be sent over the wire as it is no longer correctly quoted
    pub fn localpart(&self) -> SmtpString {
        if self.localpart.byte(0) != b'"' {
            self.localpart.clone()
        } else {
            #[derive(Copy, Clone)]
            enum State {
                Start,
                Backslash,
            }

            let mut res = self.localpart
                .iter_bytes()
                .skip(1)
                .scan(State::Start, |state, &x| match (*state, x) {
                    (State::Backslash, _) => {
                        *state = State::Start;
                        Some(Some(x))
                    }
                    (_, b'\\') => {
                        *state = State::Backslash;
                        Some(None)
                    }
                    (_, _) => {
                        *state = State::Start;
                        Some(Some(x))
                    }
                })
                .filter_map(|x| x)
                .collect::<Vec<u8>>();
            assert_eq!(res.pop().unwrap(), b'"');
            SmtpString::from_bytes(res)
        }
    }

    pub fn hostname(&self) -> &Option<SmtpString> {
        &self.hostname
    }

    pub fn into_smtp_string(self) -> SmtpString {
        let mut res = self.localpart.into_bytes();
        if let Some(host) = self.hostname {
            let mut host = host.into_bytes();
            res.push(b'@');
            res.append(&mut host);
        }
        SmtpString::from_bytes(res)
    }
}

named!(pub hostname(&[u8]) -> &[u8],
    alt!(
        recognize!(preceded!(tag!("["), take_until_and_consume!("]"))) |
        recognize!(separated_nonempty_list_complete!(tag!("."),
                       preceded!(one_of!(alnum!()),
                                 opt!(is_a!(concat!(alnum!(), "-"))))))
    )
);

named!(dot_string(&[u8]) -> &[u8], recognize!(
    separated_nonempty_list_complete!(tag!("."), is_a!(atext!()))
));

// See RFC 5321 § 4.1.2
named!(quoted_string(&[u8]) -> &[u8], recognize!(do_parse!(
    tag!("\"") >>
    many0!(alt!(
        preceded!(tag!("\\"), verify!(take!(1), |x: &[u8]| 32 <= x[0] && x[0] <= 126)) |
        verify!(take!(1), |x: &[u8]| 32 <= x[0] && x[0] != 34 && x[0] != 92 && x[0] <= 126)
    )) >>
    tag!("\"") >>
    ()
)));

named!(localpart(&[u8]) -> &[u8], alt!(quoted_string | dot_string));

named!(pub email(&[u8]) -> Email, do_parse!(
    local: localpart >>
    host: opt!(complete!(preceded!(tag!("@"), hostname))) >>
    (Email {
        localpart: SmtpString::copy_bytes(local),
        hostname: host.map(SmtpString::copy_bytes),
    })
));

named!(address_in_path(&[u8]) -> (Email, &[u8]), do_parse!(
    opt!(do_parse!(
        separated_list!(tag!(","), do_parse!(tag!("@") >> hostname >> ())) >>
        tag!(":") >>
        ()
    )) >>
    res: peek!(email) >>
    s: recognize!(email) >>
    (res, s)
));

named!(pub address_in_maybe_bracketed_path(&[u8]) -> (Email, &[u8]),
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

named!(pub eat_spaces, eat_separator!(" \t"));

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SpParameters<'a>(pub HashMap<&'a [u8], Option<&'a [u8]>>);

named!(pub sp_parameters(&[u8]) -> SpParameters, do_parse!(
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
            (key, value)
        )
    ) >>
    (SpParameters(params.into_iter().collect()))
));

#[cfg(test)]
mod tests {
    use nom::*;
    use parse_helpers::*;

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
            assert_eq!(hostname(test), IResult::Done(&b""[..], &test[..]));
        }
    }

    #[test]
    fn partial_hostnames() {
        let tests: &[(&[u8], &[u8])] = &[(b"foo.-bar.baz", b"foo"), (b"foo.bar.-baz", b"foo.bar")];
        for test in tests {
            assert_eq!(hostname(test.0).unwrap().1, test.1);
        }
    }

    #[test]
    fn invalid_hostnames() {
        let tests: &[&[u8]] = &[b"-foo.bar"];
        for test in tests {
            println!("{:?}", hostname(test));
            assert!(hostname(test).is_err());
        }
    }

    #[test]
    fn valid_dot_strings() {
        let tests: &[&[u8]] = &[
            // Adding an '@' so that tests do not return Incomplete
            b"helloooo",
            b"test.ing",
        ];
        for test in tests {
            assert_eq!(dot_string(test), IResult::Done(&b""[..], &test[..]));
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
            assert_eq!(quoted_string(test), IResult::Done(&b""[..], *test));
        }
    }

    #[test]
    fn valid_emails() {
        let tests: Vec<(&[u8], Email)> = vec![
            (
                b"t+e-s.t_i+n-g@foo.bar.baz",
                Email {
                    localpart: SmtpString::copy_bytes(b"t+e-s.t_i+n-g"),
                    hostname:  Some(SmtpString::copy_bytes(b"foo.bar.baz")),
                },
            ),
            (
                br#""quoted\"example"@example.org"#,
                Email {
                    localpart: SmtpString::copy_bytes(br#""quoted\"example""#),
                    hostname:  Some(SmtpString::copy_bytes(b"example.org")),
                },
            ),
            (
                b"postmaster",
                Email {
                    localpart: SmtpString::copy_bytes(b"postmaster"),
                    hostname:  None,
                },
            ),
            (
                b"test",
                Email {
                    localpart: SmtpString::copy_bytes(b"test"),
                    hostname:  None,
                },
            ),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(email(s), IResult::Done(&b""[..], r));
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
            assert_eq!(email(s).unwrap().1.localpart().as_bytes(), r);
        }
    }

    #[test]
    fn invalid_localpart() {
        assert!(email(b"@foo.bar").is_err());
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests: &[(&[u8], (Email, &[u8]))] = &[
            (
                b"@foo.bar,@baz.quux:test@example.org",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"test"),
                        hostname:  Some(SmtpString::copy_bytes(b"example.org")),
                    },
                    b"test@example.org",
                ),
            ),
            (
                b"foo.bar@baz.quux",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"foo.bar"),
                        hostname:  Some(SmtpString::copy_bytes(b"baz.quux")),
                    },
                    b"foo.bar@baz.quux",
                ),
            ),
        ];
        for test in tests {
            assert_eq!(
                address_in_path(test.0),
                IResult::Done(&b""[..], test.1.clone())
            );
        }
    }

    #[test]
    fn valid_addresses_in_maybe_bracketed_paths() {
        let tests: &[(&[u8], (Email, &[u8]))] = &[
            (
                b"@foo.bar,@baz.quux:test@example.org",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"test"),
                        hostname:  Some(SmtpString::copy_bytes(b"example.org")),
                    },
                    b"test@example.org",
                ),
            ),
            (
                b"<@foo.bar,@baz.quux:test@example.org>",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"test"),
                        hostname:  Some(SmtpString::copy_bytes(b"example.org")),
                    },
                    b"test@example.org",
                ),
            ),
            (
                b"<foo@bar.baz>",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"foo"),
                        hostname:  Some(SmtpString::copy_bytes(b"bar.baz")),
                    },
                    b"foo@bar.baz",
                ),
            ),
            (
                b"foo@bar.baz",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"foo"),
                        hostname:  Some(SmtpString::copy_bytes(b"bar.baz")),
                    },
                    b"foo@bar.baz",
                ),
            ),
            (
                b"foobar",
                (
                    Email {
                        localpart: SmtpString::copy_bytes(b"foobar"),
                        hostname:  None,
                    },
                    b"foobar",
                ),
            ),
        ];
        for test in tests {
            assert_eq!(
                address_in_maybe_bracketed_path(test.0),
                IResult::Done(&b""[..], test.1.clone())
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
        for test in tests {
            let res = sp_parameters(test.0);
            let (rem, res) = res.unwrap();
            assert_eq!(rem, b"");
            let res_reference = test.1.iter().map(|&x| x).collect::<HashMap<_, _>>();
            assert_eq!(res.0, res_reference);
        }
    }
}
