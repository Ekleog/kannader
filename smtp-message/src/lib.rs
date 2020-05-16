#![type_length_limit = "2663307"]

use std::{
    io::IoSlice,
    iter,
    net::{Ipv4Addr, Ipv6Addr},
    str,
};

use auto_enums::auto_enum;
use lazy_static::lazy_static;
use nom::{
    branch::alt,
    bytes::streaming::{is_a, tag, tag_no_case},
    character::streaming::one_of,
    combinator::{map, map_opt, opt, peek},
    multi::separated_nonempty_list,
    sequence::{pair, preceded, terminated, tuple},
    IResult,
};
use regex_automata::{Regex, RegexBuilder, DFA};

lazy_static! {
    static ref HOSTNAME_ASCII: Regex = RegexBuilder::new().anchored(true).build(
        r#"(?x)
            \[IPv6: [:.[:xdigit:]]+ \] |             # Ipv6
            \[ [.0-9]+ \] |                          # Ipv4
            [[:alnum:]] ([-[:alnum:]]* [[:alnum:]])? # Ascii-only domain
                ( \. [[:alnum:]] ([-[:alnum:]]* [[:alnum:]])? )*
        "#
    ).unwrap();

    static ref HOSTNAME_UTF8: Regex = RegexBuilder::new().anchored(true).build(
        r#"([-.[:alnum:]]|[[:^ascii:]])+"#
    ).unwrap();

    // Note: we have to disable the x flag here so that the # in the
    // middle of the character class does not get construed as a
    // comment
    static ref LOCALPART_ASCII: Regex = RegexBuilder::new().anchored(true).build(
        r#"(?x)
            " ( [[:ascii:]&&[^\\"[:cntrl:]]] |       # Quoted-string localpart
                \\ [[:ascii:]&&[:^cntrl:]] )* " |
            (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~]+(?x) # Dot-string localpart
                ( \. (?-x)[a-zA-Z0-9!#$%&'*+/=?^_`{|}~-]+(?x) )*
        "#
    ).unwrap();

    // Note: we have to disable the x flag here so that the # in the
    // middle of the character class does not get construed as a
    // comment
    static ref LOCALPART_UTF8: Regex = RegexBuilder::new().anchored(true).build(
        r#"(?x)
            " ( [^\\"[:cntrl:]] | \\ [[:^cntrl:]] )* " |                # Quoted-string localpart
            ( (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~](?x) | [[:^ascii:]] )+ # Dot-string localpart
                ( \. ( (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~](?x) | [[:^ascii:]] )+ )*
        "#
    ).unwrap();
}

// Implementation is similar to regex_automata's, but also returns the state
// when a match wasn't found
fn find_dfa<D: DFA>(dfa: &D, buf: &[u8]) -> Result<usize, D::ID> {
    let mut state = dfa.start_state();
    let mut last_match = if dfa.is_dead_state(state) {
        return Err(state);
    } else if dfa.is_match_state(state) {
        Some(0)
    } else {
        None
    };

    for (i, &b) in buf.iter().enumerate() {
        state = unsafe { dfa.next_state_unchecked(state, b) };
        if dfa.is_match_or_dead_state(state) {
            if dfa.is_dead_state(state) {
                return last_match.ok_or(state);
            }
            last_match = Some(i + 1);
        }
    }

    last_match.ok_or(state)
}

fn apply_regex<'a>(regex: &'a Regex) -> impl 'a + Fn(&[u8]) -> IResult<&[u8], &[u8]> {
    move |buf: &[u8]| {
        let dfa = regex.forward();

        let dfa_result = match dfa {
            regex_automata::DenseDFA::Standard(r) => find_dfa(r, buf),
            regex_automata::DenseDFA::ByteClass(r) => find_dfa(r, buf),
            regex_automata::DenseDFA::Premultiplied(r) => find_dfa(r, buf),
            regex_automata::DenseDFA::PremultipliedByteClass(r) => find_dfa(r, buf),
            other => find_dfa(other, buf),
        };

        match dfa_result {
            Ok(end) => Ok((&buf[end..], &buf[..end])),
            Err(s) if dfa.is_dead_state(s) => {
                Err(nom::Err::Error((buf, nom::error::ErrorKind::Verify)))
            }
            Err(_) => Err(nom::Err::Incomplete(nom::Needed::Unknown)),
        }
    }
}

fn terminate<'a, 'b>(term: &'b [u8]) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], char>
where
    'a: 'b,
{
    peek(one_of(term))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaybeUtf8<S = String> {
    Ascii(S),
    Utf8(S),
}

impl MaybeUtf8<&str> {
    pub fn to_owned(&self) -> MaybeUtf8<String> {
        match self {
            MaybeUtf8::Ascii(s) => MaybeUtf8::Ascii(s.to_string()),
            MaybeUtf8::Utf8(s) => MaybeUtf8::Utf8(s.to_string()),
        }
    }
}

// TODO: make this a trait once returning existentials from trait methods is a
// thing
impl<S> MaybeUtf8<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(match self {
            MaybeUtf8::Ascii(s) => IoSlice::new(s.as_ref()),
            MaybeUtf8::Utf8(s) => IoSlice::new(s.as_ref()),
        })
    }
}

// TODO: Ideally the ipv6 and ipv4 variants would be parsed in the single regex
// pass. However, that's hard to do, so let's just not do it for now and keep it
// as an optimization. So for now, it's just as well to return the parsed IPs,
// but some day they will probably be removed
/// Note: comparison happens only on the `raw` field, meaning that if you modify
/// or create a `Hostname` yourself it could have surprising results. But such a
/// `Hostname` would then not actually represent a real hostname, so you
/// probably would have had surprising results anyway.
#[derive(Clone, Debug, Eq)]
pub enum Hostname<S = String> {
    Utf8Domain { raw: S, punycode: String },
    AsciiDomain { raw: S },
    Ipv6 { raw: S, ip: Ipv6Addr },
    Ipv4 { raw: S, ip: Ipv4Addr },
}

impl<S> Hostname<S> {
    pub fn parse_until<'a, 'b>(
        term: &'b [u8],
    ) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], Hostname<S>>
    where
        'a: 'b,
        S: 'b + From<&'a str>,
    {
        alt((
            map_opt(
                terminated(apply_regex(&HOSTNAME_ASCII), terminate(term)),
                |b: &[u8]| {
                    // The three below unsafe are OK, thanks to our
                    // regex validating that `b` is proper ascii
                    // (and thus utf-8)
                    let s = unsafe { str::from_utf8_unchecked(b) };

                    if b[0] != b'[' {
                        return Some(Hostname::AsciiDomain { raw: s.into() });
                    } else if b[1] == b'I' {
                        let ip = unsafe { str::from_utf8_unchecked(&b[6..b.len() - 1]) };
                        let ip = ip.parse::<Ipv6Addr>().ok()?;

                        return Some(Hostname::Ipv6 { raw: s.into(), ip });
                    } else {
                        let ip = unsafe { str::from_utf8_unchecked(&b[1..b.len() - 1]) };
                        let ip = ip.parse::<Ipv4Addr>().ok()?;

                        return Some(Hostname::Ipv4 { raw: s.into(), ip });
                    }
                },
            ),
            map_opt(
                terminated(apply_regex(&HOSTNAME_UTF8), terminate(term)),
                |res: &[u8]| {
                    // The below unsafe is OK, thanks to our regex
                    // never disabling the `u` flag and thus
                    // validating that the match is proper utf-8
                    let raw = unsafe { str::from_utf8_unchecked(res) };

                    // TODO: looks like idna exposes only an
                    // allocating method for validating an IDNA domain
                    // name. Maybe it'd be possible to get them to
                    // expose a validation-only function? Or maybe
                    // not.
                    let punycode = idna::Config::default()
                        .use_std3_ascii_rules(true)
                        .verify_dns_length(true)
                        .check_hyphens(true)
                        .to_ascii(raw)
                        .ok()?;

                    return Some(Hostname::Utf8Domain {
                        raw: raw.into(),
                        punycode,
                    });
                },
            ),
        ))
    }
}

impl<S> Hostname<S> {
    #[inline]
    pub fn raw(&self) -> &S {
        match self {
            Hostname::Utf8Domain { raw, .. } => raw,
            Hostname::AsciiDomain { raw, .. } => raw,
            Hostname::Ipv4 { raw, .. } => raw,
            Hostname::Ipv6 { raw, .. } => raw,
        }
    }
}

impl<S> Hostname<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(self.raw().as_ref()))
    }
}

impl<S: PartialEq> std::cmp::PartialEq for Hostname<S> {
    fn eq(&self, o: &Hostname<S>) -> bool {
        self.raw() == o.raw()
    }
}

#[cfg(test)]
impl<S: Eq + PartialEq> Hostname<S> {
    fn deep_equal(&self, o: &Hostname<S>) -> bool {
        match self {
            Hostname::Utf8Domain { raw, punycode } => match o {
                Hostname::Utf8Domain {
                    raw: raw2,
                    punycode: punycode2,
                } => raw == raw2 && punycode == punycode2,
                _ => false,
            },
            Hostname::AsciiDomain { raw } => match o {
                Hostname::AsciiDomain { raw: raw2 } => raw == raw2,
                _ => false,
            },
            Hostname::Ipv4 { raw, ip } => match o {
                Hostname::Ipv4 { raw: raw2, ip: ip2 } => raw == raw2 && ip == ip2,
                _ => false,
            },
            Hostname::Ipv6 { raw, ip } => match o {
                Hostname::Ipv6 { raw: raw2, ip: ip2 } => raw == raw2 && ip == ip2,
                _ => false,
            },
        }
    }
}

// TODO: consider adding `Sane` variant like OpenSMTPD does, that would not be
// matched by weird characters
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Localpart<S = String> {
    Ascii { raw: S },
    QuotedAscii { raw: S },
    Utf8 { raw: S },
    QuotedUtf8 { raw: S },
}

impl<S> Localpart<S> {
    pub fn parse_until<'a, 'b>(
        term: &'b [u8],
    ) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], Localpart<S>>
    where
        'a: 'b,
        S: 'b + From<&'a str>,
    {
        alt((
            map(
                terminated(apply_regex(&LOCALPART_ASCII), terminate(term)),
                |b: &[u8]| {
                    // The below unsafe is OK, thanks to our regex
                    // validating that `b` is proper ascii (and thus
                    // utf-8)
                    let s = unsafe { str::from_utf8_unchecked(b) };

                    if b[0] != b'"' {
                        return Localpart::Ascii { raw: s.into() };
                    } else {
                        return Localpart::QuotedAscii { raw: s.into() };
                    }
                },
            ),
            map(
                terminated(apply_regex(&LOCALPART_UTF8), terminate(term)),
                |b: &[u8]| {
                    // The below unsafe is OK, thanks to our regex
                    // validating that `b` is proper utf-8 by never disabling the `u` flag
                    let s = unsafe { str::from_utf8_unchecked(b) };

                    if b[0] != b'"' {
                        return Localpart::Utf8 { raw: s.into() };
                    } else {
                        return Localpart::QuotedUtf8 { raw: s.into() };
                    }
                },
            ),
        ))
    }
}

impl<S> Localpart<S> {
    #[inline]
    pub fn raw(&self) -> &S {
        match self {
            Localpart::Ascii { raw } => raw,
            Localpart::QuotedAscii { raw } => raw,
            Localpart::Utf8 { raw } => raw,
            Localpart::QuotedUtf8 { raw } => raw,
        }
    }
}

impl<S> Localpart<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(self.raw().as_ref()))
    }
}

fn unquoted<S>(s: &S) -> String
where
    S: AsRef<str>,
{
    #[derive(Clone, Copy)]
    enum State {
        Start,
        Backslash,
    }

    s.as_ref()
        .chars()
        .skip(1)
        .scan(State::Start, |state, x| match (*state, x) {
            (State::Backslash, _) => {
                *state = State::Start;
                println!("backslashed anything");
                Some(Some(x))
            }
            (State::Start, '"') => {
                //
                println!("quoted \"");
                Some(None)
            }
            (_, '\\') => {
                *state = State::Backslash;
                println!("backslash");
                Some(None)
            }
            (_, _) => {
                *state = State::Start;
                println!("anything");
                Some(Some(x))
            }
        })
        .filter_map(|x| x)
        .collect()
}

impl<S> Localpart<S>
where
    S: AsRef<str>,
{
    pub fn unquote(&self) -> MaybeUtf8<String> {
        match self {
            Localpart::Ascii { raw } => MaybeUtf8::Ascii(raw.as_ref().to_owned()),
            Localpart::Utf8 { raw } => MaybeUtf8::Utf8(raw.as_ref().to_owned()),
            Localpart::QuotedAscii { raw } => MaybeUtf8::Ascii(unquoted(raw)),
            Localpart::QuotedUtf8 { raw } => MaybeUtf8::Utf8(unquoted(raw)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Email<S = String> {
    pub localpart: Localpart<S>,
    pub hostname: Option<Hostname<S>>,
}

impl<S> Email<S> {
    /// term_with_atsign must be term + b"@"
    #[inline]
    pub fn parse_until<'a, 'b>(
        term: &'b [u8],
        term_with_atsign: &'b [u8],
    ) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], Email<S>>
    where
        'a: 'b,
        S: 'b + From<&'a str>,
    {
        map(
            pair(
                Localpart::parse_until(term_with_atsign),
                opt(preceded(tag(b"@"), Hostname::parse_until(term))),
            ),
            |(localpart, hostname)| Email {
                localpart,
                hostname,
            },
        )
    }
}

impl<S> Email<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    #[auto_enum]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        #[auto_enum(Iterator)]
        let hostname = match self.hostname {
            Some(ref hostname) => iter::once(IoSlice::new(b"@")).chain(hostname.as_io_slices()),
            None => iter::empty(),
        };
        self.localpart.as_io_slices().chain(hostname)
    }
}

/// Note: for convenience this is not exactly like what is described by RFC5321,
/// and it does not contain the Email. Indeed, paths are *very* rare nowadays.
///
/// `Path` as defined here is what is specified in RFC5321 as `A-d-l`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Path<S = String> {
    pub domains: Vec<Hostname<S>>,
}

impl<S> Path<S> {
    /// term_with_comma must be the wanted terminator, with b"," added
    #[inline]
    pub fn parse_until<'a, 'b>(
        term_with_comma: &'b [u8],
    ) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], Path<S>>
    where
        'a: 'b,
        S: 'b + From<&'a str>,
    {
        map(
            separated_nonempty_list(
                tag(b","),
                preceded(tag(b"@"), Hostname::parse_until(term_with_comma)),
            ),
            |domains| Path { domains },
        )
    }
}

impl<S> Path<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        self.domains.iter().flat_map(|d| d.as_io_slices())
    }
}

// TODO: add valid/incomplete/invalid tests for Path

#[inline]
fn unbracketed_email_with_path<'a, 'b, S>(
    term: &'b [u8],
    term_with_atsign: &'b [u8],
) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], (Option<Path<S>>, Email<S>)>
where
    'a: 'b,
    S: 'b + From<&'a str>,
{
    pair(
        opt(terminated(Path::parse_until(b":,"), tag(b":"))),
        Email::parse_until(term, term_with_atsign),
    )
}

/// term
/// term_with_atsign = term + b"@"
/// term_with_bracket = term + b">"
/// term_with_bracket_atsign = term + b"@>"
#[inline]
fn email_with_path<'a, 'b, S>(
    term: &'b [u8],
    term_with_atsign: &'b [u8],
    term_with_bracket: &'b [u8],
    term_with_bracket_atsign: &'b [u8],
) -> impl 'b + Fn(&'a [u8]) -> IResult<&'a [u8], (Option<Path<S>>, Email<S>)>
where
    'a: 'b,
    S: 'b + From<&'a str>,
{
    alt((
        preceded(
            tag(b"<"),
            terminated(
                unbracketed_email_with_path(term_with_bracket, term_with_bracket_atsign),
                tag(b">"),
            ),
        ),
        unbracketed_email_with_path(term, term_with_atsign),
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command<S> {
    /// DATA <CRLF>
    Data,

    /// EHLO <domain> <CRLF>
    Ehlo {
        hostname: Hostname<S>,
    },

    Expn, // EXPN <name> <CRLF>
    Helo, // HELO <domain> <CRLF>
    Help, // HELP [<subject>] <CRLF>
    Mail, // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Noop, // NOOP [<string>] <CRLF>
    Quit, // QUIT <CRLF>
    Rcpt, // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Rset, // RSET <CRLF>
    Vrfy, // VRFY <name> <CRLF>
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
        ))(buf)
    }
}

impl<S> Command<S>
where
    S: AsRef<[u8]>,
{
    #[auto_enum(Iterator)]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        match self {
            Command::Data => iter::once(IoSlice::new(b"DATA\r\n")),
            Command::Ehlo { hostname } => iter::once(IoSlice::new(b"EHLO "))
                .chain(hostname.as_io_slices())
                .chain(iter::once(IoSlice::new(b"\r\n"))),
            _ => iter::once(unreachable!()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn show_bytes(b: &[u8]) -> String {
        if let Ok(s) = str::from_utf8(b) {
            s.into()
        } else {
            format!("{:?}", b)
        }
    }

    #[test]
    fn hostname_valid() {
        let tests: &[(&[u8], &[u8], Hostname<&str>)] = &[
            (b"foo--bar>", b"", Hostname::AsciiDomain { raw: "foo--bar" }),
            (b"foo.bar.baz>", b"", Hostname::AsciiDomain {
                raw: "foo.bar.baz",
            }),
            (b"1.2.3.4>", b"", Hostname::AsciiDomain { raw: "1.2.3.4" }),
            (b"[123.255.37.2]>", b"", Hostname::Ipv4 {
                raw: "[123.255.37.2]",
                ip: "123.255.37.2".parse().unwrap(),
            }),
            (b"[IPv6:0::ffff:8.7.6.5]>", b"", Hostname::Ipv6 {
                raw: "[IPv6:0::ffff:8.7.6.5]",
                ip: "0::ffff:8.7.6.5".parse().unwrap(),
            }),
            ("élégance.fr>".as_bytes(), b"", Hostname::Utf8Domain {
                raw: "élégance.fr",
                punycode: "xn--lgance-9uab.fr".into(),
            }),
            ("papier-maché.fr>".as_bytes(), b"", Hostname::Utf8Domain {
                raw: "papier-maché.fr",
                punycode: "xn--papier-mach-lbb.fr".into(),
            }),
        ];
        for (inp, rem, out) in tests {
            let parsed = terminated(Hostname::parse_until(b">"), tag(b">"))(inp);
            println!(
                "\nTest: {:?}\nParse result: {:?}\nExpected: {:?}",
                show_bytes(inp),
                parsed,
                out
            );
            match parsed {
                Ok((rest, host)) => assert!(rest == *rem && host.deep_equal(out)),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn hostname_incomplete() {
        let tests: &[&[u8]] = &[b"[1.2", b"[IPv6:0::"];
        for inp in tests {
            let r = Hostname::<&str>::parse_until(b">")(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn hostname_invalid() {
        let tests: &[&[u8]] = &[
            b"-foo.bar>",                 // No sub-domain starting with a dash
            b"\xFF>",                     // No invalid utf-8
            "élégance.-fr>".as_bytes(), // No dashes in utf-8 either
        ];
        for inp in tests {
            let r = Hostname::<String>::parse_until(b">")(inp);
            println!("{:?}: {:?}", show_bytes(inp), r);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: test hostname_build

    #[test]
    fn localpart_valid() {
        let tests: &[(&[u8], &[u8], Localpart<&str>)] = &[
            (b"helloooo@", b"", Localpart::Ascii { raw: "helloooo" }),
            (b"test.ing>", b"", Localpart::Ascii { raw: "test.ing" }),
            (br#""hello"@"#, b"", Localpart::QuotedAscii {
                raw: r#""hello""#,
            }),
            (
                br#""hello world. This |$ a g#eat place to experiment !">"#,
                b"",
                Localpart::QuotedAscii {
                    raw: r#""hello world. This |$ a g#eat place to experiment !""#,
                },
            ),
            (
                br#""\"escapes\", useless like h\ere, except for quotes and backslashes\\"@"#,
                b"",
                Localpart::QuotedAscii {
                    raw: r#""\"escapes\", useless like h\ere, except for quotes and backslashes\\""#,
                },
            ),
            // TODO: add Utf8 tests
        ];
        for (inp, rem, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = terminated(Localpart::parse_until(b"@>"), alt((tag(b"@"), tag(b">"))))(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) if rest == *rem && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: add incomplete, invalid and build localpart tests

    #[test]
    fn localpart_unquoting() {
        let tests: &[(&[u8], MaybeUtf8<&str>)] = &[
            (
                b"t+e-s.t_i+n-g@foo.bar.baz ",
                MaybeUtf8::Ascii("t+e-s.t_i+n-g"),
            ),
            (
                br#""quoted\"example"@example.org "#,
                MaybeUtf8::Ascii(r#"quoted"example"#),
            ),
            (
                br#""escaped\\exa\mple"@example.org "#,
                MaybeUtf8::Ascii(r#"escaped\example"#),
            ),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let res = Email::<&str>::parse_until(b" ", b" @")(inp).unwrap().1;
            println!("Result: {:?}", res);
            assert_eq!(res.localpart.unquote(), out.to_owned());
        }
    }

    #[test]
    fn email_valid() {
        let tests: &[(&[u8], &[u8], Email<&str>)] = &[
            (b"t+e-s.t_i+n-g@foo.bar.baz>", b"", Email {
                localpart: Localpart::Ascii {
                    raw: "t+e-s.t_i+n-g",
                },
                hostname: Some(Hostname::AsciiDomain { raw: "foo.bar.baz" }),
            }),
            (br#""quoted\"example"@example.org>"#, b"", Email {
                localpart: Localpart::QuotedAscii {
                    raw: r#""quoted\"example""#,
                },
                hostname: Some(Hostname::AsciiDomain { raw: "example.org" }),
            }),
            (b"postmaster>", b"", Email {
                localpart: Localpart::Ascii { raw: "postmaster" },
                hostname: None,
            }),
            (b"test>", b"", Email {
                localpart: Localpart::Ascii { raw: "test" },
                hostname: None,
            }),
            (
                r#""quoted\"example"@exámple.org>"#.as_bytes(),
                b"",
                Email {
                    localpart: Localpart::QuotedAscii {
                        raw: r#""quoted\"example""#,
                    },
                    hostname: Some(Hostname::Utf8Domain {
                        raw: "exámple.org",
                        punycode: "foo".into(),
                    }),
                },
            ),
            ("tést>".as_bytes(), b"", Email {
                localpart: Localpart::Utf8 { raw: "tést" },
                hostname: None,
            }),
        ];
        for (inp, rem, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = terminated(Email::parse_until(b">", b">@"), tag(b">"))(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) if rest == *rem && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: add incomplete email tests

    #[test]
    fn email_invalid() {
        let tests: &[&[u8]] = &[b"@foo.bar"];
        for inp in tests {
            let r = Email::<&str>::parse_until(b">", b">@")(inp);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: add build email tests

    #[test]
    fn unbracketed_email_with_path_valid() {
        let tests: &[(&[u8], &[u8], (Option<Path<&str>>, Email<&str>))] = &[
            (
                b"@foo.bar,@baz.quux:test@example.org>",
                b">",
                (
                    Some(Path {
                        domains: vec![
                            Hostname::AsciiDomain { raw: "foo.bar" },
                            Hostname::AsciiDomain { raw: "baz.quux" },
                        ],
                    }),
                    Email {
                        localpart: Localpart::Ascii { raw: "test" },
                        hostname: Some(Hostname::AsciiDomain { raw: "example.org" }),
                    },
                ),
            ),
            (
                b"foo.bar@baz.quux>",
                b">",
                (None, Email {
                    localpart: Localpart::Ascii { raw: "foo.bar" },
                    hostname: Some(Hostname::AsciiDomain { raw: "baz.quux" }),
                }),
            ),
        ];
        for (inp, rem, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            match unbracketed_email_with_path(b">", b">@")(inp) {
                Ok((rest, res)) if rest == *rem && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: test unbracketed_email_with_path with incomplete, invalid and build

    #[test]
    fn email_with_path_valid() {
        let tests: &[(&[u8], (Option<Path<&str>>, Email<&str>))] = &[
            (
                b"@foo.bar,@baz.quux:test@example.org ",
                (
                    Some(Path {
                        domains: vec![
                            Hostname::AsciiDomain { raw: "foo.bar" },
                            Hostname::AsciiDomain { raw: "baz.quux" },
                        ],
                    }),
                    Email {
                        localpart: Localpart::Ascii { raw: "test" },
                        hostname: Some(Hostname::AsciiDomain { raw: "example.org" }),
                    },
                ),
            ),
            (
                b"<@foo.bar,@baz.quux:test@example.org> ",
                (
                    Some(Path {
                        domains: vec![
                            Hostname::AsciiDomain { raw: "foo.bar" },
                            Hostname::AsciiDomain { raw: "baz.quux" },
                        ],
                    }),
                    Email {
                        localpart: Localpart::Ascii { raw: "test" },
                        hostname: Some(Hostname::AsciiDomain { raw: "example.org" }),
                    },
                ),
            ),
            (
                b"<foo@bar.baz> ",
                (None, Email {
                    localpart: Localpart::Ascii { raw: "foo" },
                    hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                }),
            ),
            (
                b"foo@bar.baz ",
                (None, Email {
                    localpart: Localpart::Ascii { raw: "foo" },
                    hostname: Some(Hostname::AsciiDomain { raw: "bar.baz" }),
                }),
            ),
            (
                b"foobar ",
                (None, Email {
                    localpart: Localpart::Ascii { raw: "foobar" },
                    hostname: None,
                }),
            ),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = email_with_path(b" ", b" @", b" >", b" @>")(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) if rest == b" " && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: test unbracketed_email_with_path with incomplete and invalid

    #[test]
    fn command_valid() {
        let tests: &[(&[u8], Command<&str>)] = &[
            (b"DATA \t  \t \r\n", Command::Data),
            (b"daTa\r\n", Command::Data),
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = Command::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) if rest == b"" && res == *out => (),
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    // TODO: test command with incomplete and invalid

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
        ];
        for (inp, out) in tests {
            println!("Test: {:?}", inp);
            let res = inp
                .as_io_slices()
                .flat_map(|s| s.to_owned().into_iter())
                .collect::<Vec<u8>>();
            println!("Result: {:?}", show_bytes(&res));
            assert_eq!(&res, out);
        }
    }
}
