use std::collections::HashMap;
use std::fmt;

use helpers::*;

macro_rules! alpha_lower { () => ("abcdefghijklmnopqrstuvwxyz") }
macro_rules! alpha_upper { () => ("ABCDEFGHIJKLMNOPQRSTUVWXYZ") }
macro_rules! alpha       { () => (concat!(alpha_lower!(), alpha_upper!())) }
macro_rules! digit       { () => ("0123456789") }
macro_rules! alnum       { () => (concat!(alpha!(), digit!())) }
macro_rules! atext       { () => (concat!(alnum!(), "!#$%&'*+-/=?^_`{|}~")) }
macro_rules! alnumdash   { () => (concat!(alnum!(), "-")) }
macro_rules! graph_except_equ { () => (concat!(alnum!(), "!\"#$%&'()*+,-./:;<>?@[\\]^_`{|}~")) }

#[cfg_attr(test, derive(PartialEq))]
#[derive(Copy, Clone)]
pub struct Email<'a> {
    localpart: &'a [u8],
    hostname: Option<&'a [u8]>,
}

impl<'a> Email<'a> {
    pub fn new<'b>(localpart: &'b [u8], hostname: Option<&'b [u8]>) -> Email<'b> {
        Email { localpart, hostname }
    }

    pub fn raw_localpart(&self) -> &[u8] {
        self.localpart
    }

    // Note: this may contain unexpected characters, check RFC5321 / RFC5322 for details
    // This is a canonicalized version of the potentially quoted localpart, not designed to be
    // sent over the wire as it is no longer correctly quoted
    pub fn localpart(&self) -> Vec<u8> {
        if self.localpart[0] != b'"' {
            self.localpart.to_owned()
        } else {
            #[derive(Copy, Clone)]
            enum State { Start, Backslash }

            let mut res = self.localpart.iter().skip(1).scan(State::Start, |state, &x| {
                match (*state, x) {
                    (State::Backslash, _) => { *state = State::Start;     Some(Some(x)) },
                    (_, b'\\')            => { *state = State::Backslash; Some(None   ) },
                    (_, _)                => { *state = State::Start;     Some(Some(x)) },
                }
            }).filter_map(|x| x).collect::<Vec<u8>>();
            assert_eq!(res.pop().unwrap(), b'"');
            res
        }
    }

    pub fn hostname(&self) -> Option<&[u8]> {
        self.hostname
    }
}

impl<'a> fmt::Debug for Email<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Email {{ localpart: {:?}, hostname: {:?} }}",
               bytes_to_dbg(self.localpart),
               self.hostname.map(bytes_to_dbg))
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
        localpart: local,
        hostname: host,
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
        let tests: &[(&[u8], &[u8])] = &[
            (b"foo.-bar.baz", b"foo"),
            (b"foo.bar.-baz", b"foo.bar"),
        ];
        for test in tests {
            assert_eq!(hostname(test.0).unwrap().1, test.1);
        }
    }

    #[test]
    fn invalid_hostnames() {
        let tests: &[&[u8]] = &[
            b"-foo.bar",
        ];
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
            (b"t+e-s.t_i+n-g@foo.bar.baz", Email {
                localpart: b"t+e-s.t_i+n-g",
                hostname: Some(b"foo.bar.baz"),
            }),
            (br#""quoted\"example"@example.org"#, Email {
                localpart: br#""quoted\"example""#,
                hostname: Some(b"example.org"),
            }),
            (b"postmaster", Email {
                localpart: b"postmaster",
                hostname: None,
            }),
            (b"test", Email {
                localpart: b"test",
                hostname: None,
            }),
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
            (br#""escaped\\exa\mple"@example.org "#, br#"escaped\example"#),
        ];
        for (s, r) in tests {
            assert_eq!(email(s).unwrap().1.localpart(), r);
        }
    }

    #[test]
    fn invalid_localpart() {
        assert!(email(b"@foo.bar").is_err());
    }

    #[test]
    fn valid_addresses_in_paths() {
        let tests: &[(&[u8], (Email, &[u8]))] = &[
            (b"@foo.bar,@baz.quux:test@example.org", (Email {
                localpart: b"test",
                hostname: Some(b"example.org"),
            }, b"test@example.org")),
            (b"foo.bar@baz.quux", (Email {
                localpart: b"foo.bar",
                hostname: Some(b"baz.quux"),
            }, b"foo.bar@baz.quux")),
        ];
        for test in tests {
            assert_eq!(address_in_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_addresses_in_maybe_bracketed_paths() {
        let tests: &[(&[u8], (Email, &[u8]))] = &[
            (b"@foo.bar,@baz.quux:test@example.org", (Email {
                localpart: b"test",
                hostname: Some(b"example.org"),
            }, b"test@example.org")),
            (b"<@foo.bar,@baz.quux:test@example.org>", (Email {
                localpart: b"test",
                hostname: Some(b"example.org"),
            }, b"test@example.org")),
            (b"<foo@bar.baz>", (Email {
                localpart: b"foo",
                hostname: Some(b"bar.baz"),
            }, b"foo@bar.baz")),
            (b"foo@bar.baz", (Email {
                localpart: b"foo",
                hostname: Some(b"bar.baz"),
            }, b"foo@bar.baz")),
            (b"foobar", (Email {
                localpart: b"foobar",
                hostname: None,
            }, b"foobar")),
        ];
        for test in tests {
            assert_eq!(address_in_maybe_bracketed_path(test.0), IResult::Done(&b""[..], test.1));
        }
    }

    #[test]
    fn valid_sp_parameters() {
        let tests: &[(&[u8], &[(&[u8], Option<&[u8]>)])] = &[
            (b"key=value", &[(b"key", Some(b"value"))]),
            (b"key=value SP key2=value2", &[(b"key", Some(b"value")), (b"key2", Some(b"value2"))]),
            (b"KeY2=V4\"l\\u@e.z\tSP\t0tterkeyz=very_muchWh4t3ver",
                &[(b"KeY2", Some(b"V4\"l\\u@e.z")), (b"0tterkeyz", Some(b"very_muchWh4t3ver"))]),
            (b"NoValueKey", &[(b"NoValueKey", None)]),
            (b"A SP B", &[(b"A", None), (b"B", None)]),
            (b"A=B SP C SP D=SP", &[(b"A", Some(b"B")), (b"C", None), (b"D", Some(b"SP"))]),
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
