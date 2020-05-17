#![type_length_limit = "109238057"]

use std::{
    convert::TryInto,
    io::IoSlice,
    iter,
    net::{Ipv4Addr, Ipv6Addr},
    str,
};

use auto_enums::auto_enum;
use lazy_static::lazy_static;
use nom::{
    branch::alt,
    bytes::streaming::{is_a, tag, tag_no_case, take_until},
    character::streaming::one_of,
    combinator::{map, map_opt, map_res, opt, peek, value},
    multi::{many0, many1_count, separated_nonempty_list},
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

    static ref PARAMETER_NAME: Regex = RegexBuilder::new().anchored(true).build(
        r#"(?x)
            [[:alnum:]] ( [[:alnum:]-] )*
        "#
    ).unwrap();

    static ref PARAMETER_VALUE_ASCII: Regex = RegexBuilder::new().anchored(true).build(
        r#"[[:ascii:]&&[^= [:cntrl:]]]+"#
    ).unwrap();

    static ref PARAMETER_VALUE_UTF8: Regex = RegexBuilder::new().anchored(true).build(
        r#"[^= [:cntrl:]]+"#
    ).unwrap();

    static ref REPLY_CODE: Regex = RegexBuilder::new().anchored(true).build(
        r#"[2-5][0-9][0-9]"#
    ).unwrap();

    static ref EXTENDED_REPLY_CODE: Regex = RegexBuilder::new().anchored(true).build(
        r#"[245]\.[0-9]{1,3}\.[0-9]{1,3}"#
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

// TODO: find out an AsciiString type, and use it here (and below)
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

impl<'a, S> From<&'a str> for MaybeUtf8<S>
where
    S: From<&'a str>,
{
    #[inline]
    fn from(s: &'a str) -> MaybeUtf8<S> {
        if s.is_ascii() {
            MaybeUtf8::Ascii(s.into())
        } else {
            MaybeUtf8::Utf8(s.into())
        }
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
                Some(Some(x))
            }
            (State::Start, '"') => Some(None),
            (_, '\\') => {
                *state = State::Backslash;
                Some(None)
            }
            (_, _) => {
                *state = State::Start;
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
        self.domains.iter().enumerate().flat_map(|(i, d)| {
            iter::once(match i {
                0 => IoSlice::new(b"@"),
                _ => IoSlice::new(b",@"),
            })
            .chain(d.as_io_slices())
        })
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
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(match self {
            ParameterName::Other(s) => s.as_ref(),
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
    S: AsRef<[u8]>,
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
    S: AsRef<[u8]>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyCodeKind {
    PositiveCompletion,
    PositiveIntermediate,
    TransientNegative,
    PermanentNegative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyCodeCategory {
    Syntax,
    Information,
    Connection,
    ReceiverStatus,
    Unspecified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplyCode(pub [u8; 3]);

#[rustfmt::skip]
impl ReplyCode {
    pub const SYSTEM_STATUS: ReplyCode = ReplyCode(*b"211");
    pub const HELP_MESSAGE: ReplyCode = ReplyCode(*b"214");
    pub const SERVICE_READY: ReplyCode = ReplyCode(*b"220");
    pub const CLOSING_CHANNEL: ReplyCode = ReplyCode(*b"221");
    pub const OKAY: ReplyCode = ReplyCode(*b"250");
    pub const USER_NOT_LOCAL_WILL_FORWARD: ReplyCode = ReplyCode(*b"251");
    pub const CANNOT_VRFY_BUT_PLEASE_TRY: ReplyCode = ReplyCode(*b"252");
    pub const START_MAIL_INPUT: ReplyCode = ReplyCode(*b"354");
    pub const SERVICE_NOT_AVAILABLE: ReplyCode = ReplyCode(*b"421");
    pub const MAILBOX_TEMPORARILY_UNAVAILABLE: ReplyCode = ReplyCode(*b"450");
    pub const LOCAL_ERROR: ReplyCode = ReplyCode(*b"451");
    pub const INSUFFICIENT_STORAGE: ReplyCode = ReplyCode(*b"452");
    pub const UNABLE_TO_ACCEPT_PARAMETERS: ReplyCode = ReplyCode(*b"455");
    pub const COMMAND_UNRECOGNIZED: ReplyCode = ReplyCode(*b"500");
    pub const SYNTAX_ERROR: ReplyCode = ReplyCode(*b"501");
    pub const COMMAND_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"502");
    pub const BAD_SEQUENCE: ReplyCode = ReplyCode(*b"503");
    pub const PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"504");
    pub const SERVER_DOES_NOT_ACCEPT_MAIL: ReplyCode = ReplyCode(*b"521");
    pub const MAILBOX_UNAVAILABLE: ReplyCode = ReplyCode(*b"550");
    pub const POLICY_REASON: ReplyCode = ReplyCode(*b"550");
    pub const USER_NOT_LOCAL: ReplyCode = ReplyCode(*b"551");
    pub const EXCEEDED_STORAGE: ReplyCode = ReplyCode(*b"552");
    pub const MAILBOX_NAME_INCORRECT: ReplyCode = ReplyCode(*b"553");
    pub const TRANSACTION_FAILED: ReplyCode = ReplyCode(*b"554");
    pub const MAIL_OR_RCPT_PARAMETER_UNIMPLEMENTED: ReplyCode = ReplyCode(*b"555");
    pub const DOMAIN_DOES_NOT_ACCEPT_MAIL: ReplyCode = ReplyCode(*b"556");
}

impl ReplyCode {
    #[inline]
    pub fn parse(buf: &[u8]) -> IResult<&[u8], ReplyCode> {
        map(apply_regex(&REPLY_CODE), |b| {
            // The below unwrap is OK, as the regex already validated
            // that there are exactly 3 characters
            ReplyCode(b.try_into().unwrap())
        })(buf)
    }

    #[inline]
    pub fn kind(&self) -> ReplyCodeKind {
        match self.0[0] {
            2 => ReplyCodeKind::PositiveCompletion,
            3 => ReplyCodeKind::PositiveIntermediate,
            4 => ReplyCodeKind::TransientNegative,
            5 => ReplyCodeKind::PermanentNegative,
            _ => panic!("Asked kind of invalid reply code!"),
        }
    }

    #[inline]
    pub fn category(&self) -> ReplyCodeCategory {
        match self.0[1] {
            0 => ReplyCodeCategory::Syntax,
            1 => ReplyCodeCategory::Information,
            2 => ReplyCodeCategory::Connection,
            5 => ReplyCodeCategory::ReceiverStatus,
            _ => ReplyCodeCategory::Unspecified,
        }
    }

    #[inline]
    pub fn code(&self) -> u16 {
        self.0[0] as u16 * 100 + self.0[1] as u16 * 10 + self.0[2] as u16
    }

    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(&self.0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExtendedReplyCodeClass {
    Success = 2,
    PersistentTransient = 4,
    PermanentFailure = 5,
}

pub enum ExtendedReplyCodeSubject {
    Undefined,
    Addressing,
    Mailbox,
    MailSystem,
    Network,
    MailDelivery,
    Content,
    Policy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedReplyCode<S> {
    pub raw: S,
    pub class: ExtendedReplyCodeClass,
    pub raw_subject: u16,
    pub raw_detail: u16,
}

macro_rules! extended_reply_codes {
    ($(($success:tt, $transient:tt, $permanent:tt, $subject:tt, $detail:tt),)*) => {
        $(
            extended_reply_codes!(@, success, $success, $subject, $detail);
            extended_reply_codes!(@, transient, $transient, $subject, $detail);
            extended_reply_codes!(@, permanent, $permanent, $subject, $detail);
        )*
    };

    (@, $any:ident, _, $subject:tt, $detail:tt) => {}; // ignore these

    (@, success, $success:ident, $subject:tt, $detail:tt) => {
        pub const $success: ExtendedReplyCode<&'static str> = ExtendedReplyCode {
            raw: concat!("2.", stringify!($subject), ".", stringify!($detail)),
            class: ExtendedReplyCodeClass::Success,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };

    (@, transient, $transient:ident, $subject:tt, $detail:tt) => {
        pub const $transient: ExtendedReplyCode<&'static str> = ExtendedReplyCode {
            raw: concat!("4.", stringify!($subject), ".", stringify!($detail)),
            class: ExtendedReplyCodeClass::PersistentTransient,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };

    (@, permanent, $permanent:ident, $subject:tt, $detail:tt) => {
        pub const $permanent: ExtendedReplyCode<&'static str> = ExtendedReplyCode {
            raw: concat!("5.", stringify!($subject), ".", stringify!($detail)),
            class: ExtendedReplyCodeClass::PermanentFailure,
            raw_subject: $subject,
            raw_detail: $detail,
        };
    };
}

#[rustfmt::skip]
impl ExtendedReplyCode<&'static str> {
    extended_reply_codes!(
        (SUCCESS_UNDEFINED, TRANSIENT_UNDEFINED, PERMANENT_UNDEFINED, 0, 0),

        (SUCCESS_ADDRESS_OTHER, TRANSIENT_ADDRESS_OTHER, PERMANENT_ADDRESS_OTHER, 1, 0),
        (_, _, PERMANENT_BAD_DEST_MAILBOX, 1, 1),
        (_, _, PERMANENT_BAD_DEST_SYSTEM, 1, 2),
        (_, _, PERMANENT_BAD_DEST_MAILBOX_SYNTAX, 1, 3),
        (SUCCESS_DEST_MAILBOX_AMBIGUOUS, TRANSIENT_DEST_MAILBOX_AMBIGUOUS, PERMANENT_DEST_MAILBOX_AMBIGUOUS, 1, 4),
        (SUCCESS_DEST_VALID, _, _, 1, 5),
        (_, _, PERMANENT_DEST_MAILBOX_HAS_MOVED, 1, 6),
        (_, _, PERMANENT_BAD_SENDER_MAILBOX_SYNTAX, 1, 7),
        (_, TRANSIENT_BAD_SENDER_SYSTEM, PERMANENT_BAD_SENDER_SYSTEM, 1, 8),
        (SUCCESS_MESSAGE_RELAYED_TO_NON_COMPLIANT_MAILER, _, PERMANENT_MESSAGE_RELAYED_TO_NON_COMPLIANT_MAILER, 1, 9),
        (_, _, PERMANENT_RECIPIENT_ADDRESS_HAS_NULL_MX, 1, 10),

        (SUCCESS_MAILBOX_OTHER, TRANSIENT_MAILBOX_OTHER, PERMANENT_MAILBOX_OTHER, 2, 0),
        (_, TRANSIENT_MAILBOX_DISABLED, PERMANENT_MAILBOX_DISABLED, 2, 1),
        (_, TRANSIENT_MAILBOX_FULL, _, 2, 2),
        (_, _, PERMANENT_MESSAGE_TOO_LONG_FOR_MAILBOX, 2, 3),
        (_, TRANSIENT_MAILING_LIST_EXPANSION_ISSUE, PERMANENT_MAILING_LIST_EXPANSION_ISSUE, 2, 4),

        (SUCCESS_SYSTEM_OTHER, TRANSIENT_SYSTEM_OTHER, PERMANENT_SYSTEM_OTHER, 3, 0),
        (_, TRANSIENT_SYSTEM_FULL, _, 3, 1),
        (_, TRANSIENT_SYSTEM_NOT_ACCEPTING_MESSAGES, PERMANENT_SYSTEM_NOT_ACCEPTING_MESSAGES, 3, 2),
        (_, TRANSIENT_SYSTEM_INCAPABLE_OF_FEATURE, PERMANENT_SYSTEM_INCAPABLE_OF_FEATURE, 3, 3),
        (_, _, PERMANENT_MESSAGE_TOO_BIG, 3, 4),
        (_, TRANSIENT_SYSTEM_INCORRECTLY_CONFIGURED, PERMANENT_SYSTEM_INCORRECTLY_CONFIGURED, 3, 5),
        (SUCCESS_REQUESTED_PRIORITY_WAS_CHANGED, _, _, 3, 6),

        (SUCCESS_NETWORK_OTHER, TRANSIENT_NETWORK_OTHER, PERMANENT_NETWORK_OTHER, 4, 0),
        (_, TRANSIENT_NO_ANSWER_FROM_HOST, _, 4, 1),
        (_, TRANSIENT_BAD_CONNECTION, _, 4, 2),
        (_, TRANSIENT_DIRECTORY_SERVER_FAILURE, _, 4, 3),
        (_, TRANSIENT_UNABLE_TO_ROUTE, PERMANENT_UNABLE_TO_ROUTE, 4, 4),
        (_, TRANSIENT_SYSTEM_CONGESTION, _, 4, 5),
        (_, TRANSIENT_ROUTING_LOOP_DETECTED, _, 4, 6),
        (_, TRANSIENT_DELIVERY_TIME_EXPIRED, PERMANENT_DELIVERY_TIME_EXPIRED, 4, 7),

        (SUCCESS_DELIVERY_OTHER, TRANSIENT_DELIVERY_OTHER, PERMANENT_DELIVERY_OTHER, 5, 0),
        (_, _, PERMANENT_INVALID_COMMAND, 5, 1),
        (_, _, PERMANENT_SYNTAX_ERROR, 5, 2),
        (_, TRANSIENT_TOO_MANY_RECIPIENTS, PERMANENT_TOO_MANY_RECIPIENTS, 5, 3),
        (_, _, PERMANENT_INVALID_COMMAND_ARGUMENTS, 5, 4),
        (_, TRANSIENT_WRONG_PROTOCOL_VERSION, PERMANENT_WRONG_PROTOCOL_VERSION, 5, 5),
        (_, TRANSIENT_AUTH_EXCHANGE_LINE_TOO_LONG, PERMANENT_AUTH_EXCHANGE_LINE_TOO_LONG, 5, 6),

        (SUCCESS_CONTENT_OTHER, TRANSIENT_CONTENT_OTHER, PERMANENT_CONTENT_OTHER, 6, 0),
        (_, _, PERMANENT_MEDIA_NOT_SUPPORTED, 6, 1),
        (_, TRANSIENT_CONVERSION_REQUIRED_AND_PROHIBITED, PERMANENT_CONVERSION_REQUIRED_AND_PROHIBITED, 6, 2),
        (_, TRANSIENT_CONVERSION_REQUIRED_BUT_NOT_SUPPORTED, PERMANENT_CONVERSION_REQUIRED_BUT_NOT_SUPPORTED, 6, 3),
        (SUCCESS_CONVERSION_WITH_LOSS_PERFORMED, TRANSIENT_CONVERSION_WITH_LOSS_PERFORMED, PERMANENT_CONVERSION_WITH_LOSS_PERFORMED, 6, 4),
        (_, TRANSIENT_CONVERSION_FAILED, PERMANENT_CONVERSION_FAILED, 6, 5),
        (_, TRANSIENT_MESSAGE_CONTENT_NOT_AVAILABLE, PERMANENT_MESSAGE_CONTENT_NOT_AVAILABLE, 6, 6),
        (_, _, PERMANENT_NON_ASCII_ADDRESSES_NOT_PERMITTED, 6, 7),
        (SUCCESS_UTF8_WOULD_BE_REQUIRED, TRANSIENT_UTF8_WOULD_BE_REQUIRED, PERMANENT_UTF8_WOULD_BE_REQUIRED, 6, 8),
        (_, _, PERMANENT_UTF8_MESSAGE_CANNOT_BE_TRANSMITTED, 6, 9),
        (SUCCESS_UTF8_WOULD_BE_REQUIRED_BIS, TRANSIENT_UTF8_WOULD_BE_REQUIRED_BIS, PERMANENT_UTF8_WOULD_BE_REQUIRED_BIS, 6, 10),

        (SUCCESS_POLICY_OTHER, TRANSIENT_POLICY_OTHER, PERMANENT_POLICY_OTHER, 7, 0),
        (_, _, PERMANENT_DELIVERY_NOT_AUTHORIZED, 7, 1),
        (_, _, PERMANENT_MAILING_LIST_EXPANSION_PROHIBITED, 7, 2),
        (_, _, PERMANENT_SECURITY_CONVERSION_REQUIRED_BUT_NOT_POSSIBLE, 7, 3),
        (_, _, PERMANENT_SECURITY_FEATURES_NOT_SUPPORTED, 7, 4),
        (_, TRANSIENT_CRYPTO_FAILURE, PERMANENT_CRYPTO_FAILURE, 7, 5),
        (_, TRANSIENT_CRYPTO_ALGO_NOT_SUPPORTED, PERMANENT_CRYPTO_ALGO_NOT_SUPPORTED, 7, 6),
        (SUCCESS_MESSAGE_INTEGRITY_FAILURE, TRANSIENT_MESSAGE_INTEGRITY_FAILURE, PERMANENT_MESSAGE_INTEGRITY_FAILURE, 7, 7),
        (_, _, PERMANENT_AUTH_CREDENTIALS_INVALID, 7, 8),
        (_, _, PERMANENT_AUTH_MECHANISM_TOO_WEAK, 7, 9),
        (_, _, PERMANENT_ENCRYPTION_NEEDED, 7, 10),
        (_, _, PERMANENT_ENCRYPTION_REQUIRED_FOR_REQUESTED_AUTH_MECHANISM, 7, 11),
        (_, TRANSIENT_PASSWORD_TRANSITION_NEEDED, _, 7, 12),
        (_, _, PERMANENT_USER_ACCOUNT_DISABLED, 7, 13),
        (_, _, PERMANENT_TRUST_RELATIONSHIP_REQUIRED, 7, 14),
        (_, TRANSIENT_PRIORITY_TOO_LOW, PERMANENT_PRIORITY_TOO_LOW, 7, 15),
        (_, TRANSIENT_MESSAGE_TOO_BIG_FOR_PRIORITY, PERMANENT_MESSAGE_TOO_BIG_FOR_PRIORITY, 7, 16),
        (_, _, PERMANENT_MAILBOX_OWNER_HAS_CHANGED, 7, 17),
        (_, _, PERMANENT_DOMAIN_OWNER_HAS_CHANGED, 7, 18),
        (_, _, PERMANENT_RRVS_CANNOT_BE_COMPLETED, 7, 19),
        (_, _, PERMANENT_NO_PASSING_DKIM_SIGNATURE_FOUND, 7, 20),
        (_, _, PERMANENT_NO_ACCEPTABLE_DKIM_SIGNATURE_FOUND, 7, 21),
        (_, _, PERMANENT_NO_AUTHOR_MATCHED_DKIM_SIGNATURE_FOUND, 7, 22),
        (_, _, PERMANENT_SPF_VALIDATION_FAILED, 7, 23),
        (_, TRANSIENT_SPF_VALIDATION_ERROR, PERMANENT_SPF_VALIDATION_ERROR, 7, 24),
        (_, _, PERMANENT_REVERSE_DNS_VALIDATION_FAILED, 7, 25),
        (_, _, PERMANENT_MULTIPLE_AUTH_CHECKS_FAILED, 7, 26),
        (_, _, PERMANENT_SENDER_ADDRESS_HAS_NULL_MX, 7, 27),
        (SUCCESS_MAIL_FLOOD_DETECTED, TRANSIENT_MAIL_FLOOD_DETECTED, PERMANENT_MAIL_FLOOD_DETECTED, 7, 28),
        (_, _, PERMANENT_ARC_VALIDATION_FAILURE, 7, 29),
        (_, _, PERMANENT_REQUIRETLS_SUPPORT_REQUIRED, 7, 30),
    );
}

impl<S> ExtendedReplyCode<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], ExtendedReplyCode<S>>
    where
        S: From<&'a str>,
    {
        map(apply_regex(&EXTENDED_REPLY_CODE), |raw| {
            let class = raw[0] - b'0';
            let class = match class {
                2 => ExtendedReplyCodeClass::Success,
                4 => ExtendedReplyCodeClass::PersistentTransient,
                5 => ExtendedReplyCodeClass::PermanentFailure,
                _ => panic!("Regex allowed unexpected elements"),
            };
            let after_class = &raw[2..];
            // These unwrap and unsafe are OK thanks to the regex
            // already matching
            let second_dot = after_class.iter().position(|c| *c == b'.').unwrap();
            let raw_subject = unsafe { str::from_utf8_unchecked(&after_class[..second_dot]) }
                .parse()
                .unwrap();
            let raw_detail = unsafe { str::from_utf8_unchecked(&after_class[second_dot + 1..]) }
                .parse()
                .unwrap();
            let raw = unsafe { str::from_utf8_unchecked(raw) };
            ExtendedReplyCode {
                raw: raw.into(),
                class,
                raw_subject,
                raw_detail,
            }
        })(buf)
    }

    #[inline]
    pub fn subject(&self) -> ExtendedReplyCodeSubject {
        match self.raw_subject {
            1 => ExtendedReplyCodeSubject::Addressing,
            2 => ExtendedReplyCodeSubject::Mailbox,
            3 => ExtendedReplyCodeSubject::MailSystem,
            4 => ExtendedReplyCodeSubject::Network,
            5 => ExtendedReplyCodeSubject::MailDelivery,
            6 => ExtendedReplyCodeSubject::Content,
            7 => ExtendedReplyCodeSubject::Policy,
            _ => ExtendedReplyCodeSubject::Undefined,
        }
    }
}

impl<S> ExtendedReplyCode<S>
where
    S: AsRef<[u8]>,
{
    #[inline]
    pub fn as_io_slices(&self) -> impl Iterator<Item = IoSlice> {
        iter::once(IoSlice::new(self.raw.as_ref()))
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

    // TODO: test command with invalid

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
                .flat_map(|s| s.to_owned().into_iter())
                .collect::<Vec<u8>>();
            println!("Result  : {:?}", show_bytes(&res));
            println!("Expected: {:?}", show_bytes(out));
            assert_eq!(&res, out);
        }
    }

    #[test]
    pub fn reply_code_valid() {
        let tests: &[(&[u8], [u8; 3])] = &[(b"523", *b"523"), (b"234", *b"234")];
        for (inp, out) in tests {
            println!("Test: {:?}", show_bytes(inp));
            let r = ReplyCode::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, ReplyCode(*out));
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn reply_code_incomplete() {
        let tests: &[&[u8]] = &[b"3", b"43"];
        for inp in tests {
            let r = ReplyCode::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn reply_code_invalid() {
        let tests: &[&[u8]] = &[b"foo", b"123", b"648"];
        for inp in tests {
            let r = ReplyCode::parse(inp);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: test reply code builder

    #[test]
    pub fn extended_reply_code_valid() {
        let tests: &[(&[u8], (ExtendedReplyCodeClass, u16, u16))] = &[
            (b"2.1.23", (ExtendedReplyCodeClass::Success, 1, 23)),
            (
                b"5.243.567",
                (ExtendedReplyCodeClass::PermanentFailure, 243, 567),
            ),
        ];
        for (inp, (class, raw_subject, raw_detail)) in tests.iter().cloned() {
            println!("Test: {:?}", show_bytes(inp));
            let r = ExtendedReplyCode::parse(inp);
            println!("Result: {:?}", r);
            match r {
                Ok((rest, res)) => {
                    assert_eq!(rest, b"");
                    assert_eq!(res, ExtendedReplyCode {
                        raw: str::from_utf8(inp).unwrap(),
                        class,
                        raw_subject,
                        raw_detail,
                    });
                }
                x => panic!("Unexpected result: {:?}", x),
            }
        }
    }

    #[test]
    fn extended_reply_code_incomplete() {
        let tests: &[&[u8]] = &[b"4.", b"5.23"];
        for inp in tests {
            let r = ExtendedReplyCode::<&str>::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn extended_reply_code_invalid() {
        let tests: &[&[u8]] = &[b"foo", b"3.5.1", b"1.1000.2"];
        for inp in tests {
            let r = ExtendedReplyCode::<String>::parse(inp);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    // TODO: test extended reply code builder
}
