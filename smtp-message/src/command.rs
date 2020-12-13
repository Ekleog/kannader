use std::{io::IoSlice, iter, str};

use auto_enums::auto_enum;
use lazy_static::lazy_static;
use nom::{
    branch::alt,
    bytes::streaming::{is_a, tag, tag_no_case, take_until},
    character::streaming::one_of,
    combinator::{map, map_res, opt, value},
    multi::{many0, many1_count},
    sequence::{pair, preceded, terminated, tuple},
    IResult,
};
use regex_automata::{Regex, RegexBuilder};

use crate::*;

lazy_static! {
    static ref PARAMETER_NAME: Regex = RegexBuilder::new()
        .anchored(true)
        .build(
            r#"(?x)
            [[:alnum:]] ( [[:alnum:]-] )*
        "#
        )
        .unwrap();
    static ref PARAMETER_VALUE_ASCII: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[[:ascii:]&&[^= [:cntrl:]]]+"#)
        .unwrap();
    static ref PARAMETER_VALUE_UTF8: Regex = RegexBuilder::new()
        .anchored(true)
        .build(r#"[^= [:cntrl:]]+"#)
        .unwrap();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterName<S> {
    Other(S),
}

impl<S> ParameterName<S> {
    #[inline]
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], ParameterName<S>>
    where
        S: From<&'a str>,
    {
        map(apply_regex(&PARAMETER_NAME), |b: &[u8]| {
            // The below unsafe is OK, thanks to PARAMETER_NAME
            // validating that `b` is proper ascii
            let s = unsafe { str::from_utf8_unchecked(b) };
            ParameterName::Other(s.into())
        })(buf)
    }
}

impl<S> ParameterName<S>
where
    S: AsRef<str>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(match self {
            ParameterName::Other(s) => s.as_ref().as_ref(),
        }))
    }
}

/// Note: This struct includes the leading ' '
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameters<S>(Vec<(ParameterName<S>, Option<MaybeUtf8<S>>)>);

impl<S> Parameters<S> {
    /// If term is the wanted terminator, then
    /// term_with_sp_tab = term + b" \t"
    pub fn parse_until<'a, 'b>(
        term_with_sp_tab: &'b [u8],
    ) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], Parameters<S>>
    where
        'a: 'b,
        S: 'b + From<&'a str>,
    {
        map(
            many0(preceded(
                many1_count(one_of(" \t")),
                pair(
                    ParameterName::parse,
                    opt(preceded(
                        tag(b"="),
                        alt((
                            map(
                                terminated(
                                    apply_regex(&PARAMETER_VALUE_ASCII),
                                    terminate(term_with_sp_tab),
                                ),
                                |b| {
                                    // The below unsafe is OK, thanks
                                    // to the regex having validated
                                    // that it is pure ASCII
                                    let s = unsafe { str::from_utf8_unchecked(b) };
                                    MaybeUtf8::Ascii(s.into())
                                },
                            ),
                            map(
                                terminated(
                                    apply_regex(&PARAMETER_VALUE_UTF8),
                                    terminate(term_with_sp_tab),
                                ),
                                |b| {
                                    // The below unsafe is OK, thanks
                                    // to the regex having validated
                                    // that it is valid UTF-8
                                    let s = unsafe { str::from_utf8_unchecked(b) };
                                    MaybeUtf8::Utf8(s.into())
                                },
                            ),
                        )),
                    )),
                ),
            )),
            |v| Parameters(v),
        )
    }
}

impl<S> Parameters<S>
where
    S: AsRef<str>,
{
    #[inline]
    #[auto_enum]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        self.0.iter().flat_map(|(name, value)| {
            iter::once(IoSlice::new(b" "))
                .chain(name.as_io_slices())
                .chain(
                    #[auto_enum(Iterator)]
                    match value {
                        None => iter::empty(),
                        Some(v) => iter::once(IoSlice::new(b"=")).chain(v.as_io_slices()),
                    },
                )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command<S> {
    /// DATA <CRLF>
    Data,

    /// EHLO <hostname> <CRLF>
    Ehlo { hostname: Hostname<S> },

    /// EXPN <name> <CRLF>
    Expn { name: MaybeUtf8<S> },

    /// HELO <hostname> <CRLF>
    Helo { hostname: Hostname<S> },

    /// HELP [<subject>] <CRLF>
    Help { subject: MaybeUtf8<S> },

    /// MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Mail {
        path: Option<Path<S>>,
        email: Option<Email<S>>,
        params: Parameters<S>,
    },

    /// NOOP [<string>] <CRLF>
    Noop { string: MaybeUtf8<S> },

    /// QUIT <CRLF>
    Quit,

    /// RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Rcpt {
        path: Option<Path<S>>,
        email: Email<S>,
        params: Parameters<S>,
    },

    /// RSET <CRLF>
    Rset,

    /// VRFY <name> <CRLF>
    Vrfy { name: MaybeUtf8<S> },
}

impl<S> Command<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], Command<S>>
    where
        S: From<&'a str>,
    {
        alt((
            map(
                tuple((tag_no_case(b"DATA"), opt(is_a(" \t")), tag(b"\r\n"))),
                |_| Command::Data,
            ),
            map(
                tuple((
                    tag_no_case(b"EHLO"),
                    is_a(" \t"),
                    Hostname::parse_until(b" \t\r"),
                    opt(is_a(" \t")),
                    tag(b"\r\n"),
                )),
                |(_, _, hostname, _, _)| Command::Ehlo { hostname },
            ),
            map_res(
                tuple((
                    tag_no_case(b"EXPN"),
                    one_of(" \t"),
                    take_until("\r\n"),
                    tag(b"\r\n"),
                )),
                |(_, _, name, _)| {
                    str::from_utf8(name).map(|name| Command::Expn {
                        name: MaybeUtf8::from(name),
                    })
                },
            ),
            map(
                tuple((
                    tag_no_case(b"HELO"),
                    is_a(" \t"),
                    Hostname::parse_until(b" \t\r"),
                    opt(is_a(" \t")),
                    tag(b"\r\n"),
                )),
                |(_, _, hostname, _, _)| Command::Helo { hostname },
            ),
            map_res(
                preceded(
                    tag_no_case(b"HELP"),
                    alt((
                        preceded(one_of(" \t"), terminated(take_until("\r\n"), tag(b"\r\n"))),
                        value(&b""[..], tag(b"\r\n")),
                    )),
                ),
                |s| {
                    str::from_utf8(s).map(|s| Command::Help {
                        subject: MaybeUtf8::from(s),
                    })
                },
            ),
            map(
                tuple((
                    tag_no_case(b"MAIL FROM:"),
                    opt(is_a(" \t")),
                    alt((
                        map(tag(b"<>"), |_| None),
                        map(
                            email_with_path(b" \t\r", b" \t\r@", b" \t\r>", b" \t\r@>"),
                            Some,
                        ),
                    )),
                    Parameters::parse_until(b" \t\r"),
                    opt(is_a(" \t")),
                    tag("\r\n"),
                )),
                |(_, _, email, params, _, _)| match email {
                    None => Command::Mail {
                        path: None,
                        email: None,
                        params,
                    },
                    Some((path, email)) => Command::Mail {
                        path,
                        email: Some(email),
                        params,
                    },
                },
            ),
            map_res(
                preceded(
                    tag_no_case(b"NOOP"),
                    alt((
                        preceded(one_of(" \t"), terminated(take_until("\r\n"), tag(b"\r\n"))),
                        value(&b""[..], tag(b"\r\n")),
                    )),
                ),
                |s| {
                    str::from_utf8(s).map(|s| Command::Noop {
                        string: MaybeUtf8::from(s),
                    })
                },
            ),
            map(
                tuple((tag_no_case(b"QUIT"), opt(is_a(" \t")), tag(b"\r\n"))),
                |_| Command::Quit,
            ),
            map(
                tuple((
                    tag_no_case(b"RCPT TO:"),
                    opt(is_a(" \t")),
                    email_with_path(b" \t\r", b" \t\r@", b" \t\r>", b" \t\r@>"),
                    Parameters::parse_until(b" \t\r"),
                    opt(is_a(" \t")),
                    tag("\r\n"),
                )),
                |(_, _, (path, email), params, _, _)| Command::Rcpt {
                    path,
                    email,
                    params,
                },
            ),
            map(
                tuple((tag_no_case(b"RSET"), opt(is_a(" \t")), tag(b"\r\n"))),
                |_| Command::Rset,
            ),
            map_res(
                tuple((
                    tag_no_case(b"VRFY"),
                    one_of(" \t"),
                    take_until("\r\n"),
                    tag(b"\r\n"),
                )),
                |(_, _, s, _)| {
                    str::from_utf8(s).map(|s| Command::Vrfy {
                        name: MaybeUtf8::from(s),
                    })
                },
            ),
        ))(buf)
    }
}

impl<S> Command<S>
where
    S: AsRef<str>,
{
    #[auto_enum(Iterator)]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        match self {
            Command::Data => iter::once(IoSlice::new(b"DATA\r\n")),

            Command::Ehlo { hostname } => iter::once(IoSlice::new(b"EHLO "))
                .chain(hostname.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Expn { name } => iter::once(IoSlice::new(b"EXPN "))
                .chain(name.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Helo { hostname } => iter::once(IoSlice::new(b"HELO "))
                .chain(hostname.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Help { subject } => iter::once(IoSlice::new(b"HELP "))
                .chain(subject.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Mail {
                path,
                email,
                params,
            } => iter::once(IoSlice::new(b"MAIL FROM:<"))
                .chain(
                    #[auto_enum(Iterator)]
                    match path {
                        Some(path) => path.as_io_slices().chain(iter::once(IoSlice::new(b":"))),
                        None => iter::empty(),
                    },
                )
                .chain(
                    #[auto_enum(Iterator)]
                    match email {
                        Some(email) => email.as_io_slices(),
                        None => iter::empty(),
                    },
                )
                .chain(iter::once(IoSlice::new(b">")))
                .chain(params.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Noop { string } => iter::once(IoSlice::new(b"NOOP "))
                .chain(string.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Quit => iter::once(IoSlice::new(b"QUIT\r\n")),

            Command::Rcpt {
                path,
                email,
                params,
            } => iter::once(IoSlice::new(b"RCPT TO:<"))
                .chain(
                    #[auto_enum(Iterator)]
                    match path {
                        Some(path) => path.as_io_slices().chain(iter::once(IoSlice::new(b":"))),
                        None => iter::empty(),
                    },
                )
                .chain(email.as_io_slices())
                .chain(iter::once(IoSlice::new(b">")))
                .chain(params.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),

            Command::Rset => iter::once(IoSlice::new(b"RSET\r\n")),

            Command::Vrfy { name } => iter::once(IoSlice::new(b"VRFY "))
                .chain(name.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: test parameter (without an s) valid, incomplete, invalid and build

    #[test]
    fn parameters_valid() {
        let tests: &[(&[u8], Parameters<&str>)] = &[
            (
                b" key=value\r\n",
                Parameters(vec![(
                    ParameterName::Other("key"),
                    Some(MaybeUtf8::Ascii("value")),
                )]),
            ),
            (
                b"\tkey=value\tkey2=value2\r\n",
                Parameters(vec![
                    (ParameterName::Other("key"), Some(MaybeUtf8::Ascii("value"))),
                    (
                        ParameterName::Other("key2"),
                        Some(MaybeUtf8::Ascii("value2")),
                    ),
                ]),
            ),
            (
                b" KeY2=V4\"l\\u@e.z\t0tterkeyz=very_muchWh4t3ver\r\n",
                Parameters(vec![
                    (
                        ParameterName::Other("KeY2"),
                        Some(MaybeUtf8::Ascii("V4\"l\\u@e.z")),
                    ),
                    (
                        ParameterName::Other("0tterkeyz"),
                        Some(MaybeUtf8::Ascii("very_muchWh4t3ver")),
                    ),
                ]),
            ),
            (
                b" NoValueKey\r\n",
                Parameters(vec![(ParameterName::Other("NoValueKey"), None)]),
            ),
            (
                b" A B\r\n",
                Parameters(vec![
                    (ParameterName::Other("A"), None),
                    (ParameterName::Other("B"), None),
                ]),
            ),
            (
                b" A=B C D=SP\r\n",
                Parameters(vec![
                    (ParameterName::Other("A"), Some(MaybeUtf8::Ascii("B"))),
                    (ParameterName::Other("C"), None),
                    (ParameterName::Other("D"), Some(MaybeUtf8::Ascii("SP"))),
                ]),
            ),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = Parameters::parse_until(b" \t\r\n")(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) if rest == b"\r\n" && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: test parameter incomplete, invalid and build

    #[test]
    fn command_valid() {
        let tests: &[(&[u8], Command<&str>)] = &[
            (b"DATA \t  \t \r\n", Command::Data),
            (b"daTa\r\n", Command::Data),
            (b"eHlO \t hello.world \t \r\n", Command::Ehlo {
                hostname: Hostname::AsciiDomain { raw: "hello.world" },
            }),
            (b"EHLO hello.world\r\n", Command::Ehlo {
                hostname: Hostname::AsciiDomain { raw: "hello.world" },
            }),
            (b"EXpN \t hello.world \t \r\n", Command::Expn {
                name: MaybeUtf8::Ascii("\t hello.world \t "),
            }),
            (b"hElO\t hello.world \t \r\n", Command::Helo {
                hostname: Hostname::AsciiDomain { raw: "hello.world" },
            }),
            (b"HELO hello.world\r\n", Command::Helo {
                hostname: Hostname::AsciiDomain { raw: "hello.world" },
            }),
            (b"help \t hello.world \t \r\n", Command::Help {
                subject: MaybeUtf8::Ascii("\t hello.world \t "),
            }),
            (b"HELP\r\n", Command::Help {
                subject: MaybeUtf8::Ascii(""),
            }),
            (b"hElP \r\n", Command::Help {
                subject: MaybeUtf8::Ascii(""),
            }),
            (b"Mail FROM:<@one,@two:foo@bar.baz>\r\n", Command::Mail {
                path: Some(Path {
                    domains: vec![
                        Hostname::AsciiDomain { raw: "one" },
                        Hostname::AsciiDomain { raw: "two" },
                    ],
                }),
                email: Some(Email {
                    localpart: Localpart::Ascii { raw: "foo" },
                    hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                }),
                params: Parameters(vec![]),
            }),
            (b"MaiL FrOm: quux@example.net  \t \r\n", Command::Mail {
                path: None,
                email: Some(Email {
                    localpart: Localpart::Ascii { raw: "quux" },
                    hostname: Some(Hostname::AsciiDomain { raw: "example.net" }),
                }),
                params: Parameters(vec![]),
            }),
            (b"MaiL FrOm: quux@example.net\r\n", Command::Mail {
                path: None,
                email: Some(Email {
                    localpart: Localpart::Ascii { raw: "quux" },
                    hostname: Some(Hostname::AsciiDomain { raw: "example.net" }),
                }),
                params: Parameters(vec![]),
            }),
            (b"mail FROM:<>\r\n", Command::Mail {
                path: None,
                email: None,
                params: Parameters(vec![]),
            }),
            (b"MAIL FROM:<> hello=world foo\r\n", Command::Mail {
                path: None,
                email: None,
                params: Parameters(vec![
                    (
                        ParameterName::Other("hello"),
                        Some(MaybeUtf8::Ascii("world")),
                    ),
                    (ParameterName::Other("foo"), None),
                ]),
            }),
            (b"NOOP \t hello.world \t \r\n", Command::Noop {
                string: MaybeUtf8::Ascii("\t hello.world \t "),
            }),
            (b"nOoP\r\n", Command::Noop {
                string: MaybeUtf8::Ascii(""),
            }),
            (b"noop \r\n", Command::Noop {
                string: MaybeUtf8::Ascii(""),
            }),
            (b"QUIT \t  \t \r\n", Command::Quit),
            (b"quit\r\n", Command::Quit),
            (b"RCPT TO:<@one,@two:foo@bar.baz>\r\n", Command::Rcpt {
                path: Some(Path {
                    domains: vec![
                        Hostname::AsciiDomain { raw: "one" },
                        Hostname::AsciiDomain { raw: "two" },
                    ],
                }),
                email: Email {
                    localpart: Localpart::Ascii { raw: "foo" },
                    hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                },
                params: Parameters(vec![]),
            }),
            (b"Rcpt tO: quux@example.net  \t \r\n", Command::Rcpt {
                path: None,
                email: Email {
                    localpart: Localpart::Ascii { raw: "quux" },
                    hostname: Some(Hostname::AsciiDomain { raw: "example.net" }),
                },
                params: Parameters(vec![]),
            }),
            (b"rcpt TO:<Postmaster>\r\n", Command::Rcpt {
                path: None,
                email: Email {
                    localpart: Localpart::Ascii { raw: "Postmaster" },
                    hostname: None,
                },
                params: Parameters(vec![]),
            }),
            (b"RcPt TO: \t poStmaster\r\n", Command::Rcpt {
                path: None,
                email: Email {
                    localpart: Localpart::Ascii { raw: "poStmaster" },
                    hostname: None,
                },
                params: Parameters(vec![]),
            }),
            (b"RSET \t  \t \r\n", Command::Rset),
            (b"rSet\r\n", Command::Rset),
            (b"VrFY \t hello.world \t \r\n", Command::Vrfy {
                name: MaybeUtf8::Ascii("\t hello.world \t "),
            }),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = Command::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, *out);
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn command_incomplete() {
        // TODO: add tests for all the variants (that could)
        let tests: &[&[u8]] = &[b"MAIL FROM:<foo@bar.com", b"mail from:foo@bar.com"];
        for inp in tests {
            let r = Command::<&str>::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn command_invalid() {
        let tests: &[&[u8]] = &[b"HELPfoo"];
        for inp in tests {
            let r = Command::<&str>::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn command_build() {
        let tests: &[(Command<&str>, &[u8])] = &[
            (Command::Data, b"DATA\r\n"),
            (
                Command::Ehlo {
                    hostname: Hostname::AsciiDomain {
                        raw: "test.foo.bar",
                    },
                },
                b"EHLO test.foo.bar\r\n",
            ),
            (
                Command::Expn {
                    name: MaybeUtf8::Ascii("foobar"),
                },
                b"EXPN foobar\r\n",
            ),
            (
                Command::Helo {
                    hostname: Hostname::AsciiDomain {
                        raw: "test.example.org",
                    },
                },
                b"HELO test.example.org\r\n",
            ),
            (
                Command::Help {
                    subject: MaybeUtf8::Ascii("topic"),
                },
                b"HELP topic\r\n",
            ),
            (
                Command::Mail {
                    path: None,
                    email: Some(Email {
                        localpart: Localpart::Ascii { raw: "foo" },
                        hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                    }),
                    params: Parameters(vec![]),
                },
                b"MAIL FROM:<foo@bar.baz>\r\n",
            ),
            (
                Command::Mail {
                    path: Some(Path {
                        domains: vec![
                            Hostname::AsciiDomain { raw: "test" },
                            Hostname::AsciiDomain { raw: "foo.bar" },
                        ],
                    }),
                    email: Some(Email {
                        localpart: Localpart::Ascii { raw: "foo" },
                        hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                    }),
                    params: Parameters(vec![]),
                },
                b"MAIL FROM:<@test,@foo.bar:foo@bar.baz>\r\n",
            ),
            (
                Command::Mail {
                    path: None,
                    email: None,
                    params: Parameters(vec![]),
                },
                b"MAIL FROM:<>\r\n",
            ),
            (
                Command::Mail {
                    path: None,
                    email: Some(Email {
                        localpart: Localpart::Ascii { raw: "hello" },
                        hostname: Some(Hostname::AsciiDomain {
                            raw: "world.example.org",
                        }),
                    }),
                    params: Parameters(vec![
                        (ParameterName::Other("foo"), Some(MaybeUtf8::Ascii("bar"))),
                        (ParameterName::Other("baz"), None),
                        (
                            ParameterName::Other("helloworld"),
                            Some(MaybeUtf8::Ascii("bleh")),
                        ),
                    ]),
                },
                b"MAIL FROM:<hello@world.example.org> foo=bar baz helloworld=bleh\r\n",
            ),
            (
                Command::Noop {
                    string: MaybeUtf8::Ascii("useless string"),
                },
                b"NOOP useless string\r\n",
            ),
            (Command::Quit, b"QUIT\r\n"),
            (
                Command::Rcpt {
                    path: None,
                    email: Email {
                        localpart: Localpart::Ascii { raw: "foo" },
                        hostname: Some(Hostname::AsciiDomain { raw: "bar.com" }),
                    },
                    params: Parameters(vec![]),
                },
                b"RCPT TO:<foo@bar.com>\r\n",
            ),
            (
                Command::Rcpt {
                    path: None,
                    email: Email {
                        localpart: Localpart::Ascii { raw: "Postmaster" },
                        hostname: None,
                    },
                    params: Parameters(vec![]),
                },
                b"RCPT TO:<Postmaster>\r\n",
            ),
            (Command::Rset, b"RSET\r\n"),
            (
                Command::Vrfy {
                    name: MaybeUtf8::Ascii("postmaster"),
                },
                b"VRFY postmaster\r\n",
            ),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", inp);
            let res = inp
                .as_io_slices()
                .flat_map(|s| s.iter().cloned().collect::<Vec<_>>().into_iter())
                .collect::<Vec<u8>>();
            println!("Result  : {:?}", show_bytes(&res));
            println!("Expected: {:?}", show_bytes(out));
            assert_eq!(&res, out);
        }
    }
}
