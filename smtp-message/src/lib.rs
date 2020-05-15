use std::{
    net::{Ipv4Addr, Ipv6Addr},
    str,
};

use lazy_static::lazy_static;
use nom::{
    branch::alt,
    bytes::streaming::tag,
    combinator::{map, map_opt},
    sequence::tuple,
    IResult,
};
use regex::bytes::Regex;

lazy_static! {
    static ref HOSTNAME_ASCII: Regex = Regex::new(
        r#"(?x) ^(
            \[IPv6: [:.[:xdigit:]]+ \] |             # Ipv6
            \[ [.0-9]+ \] |                          # Ipv4
            [[:alnum:]] ([-[:alnum:]]* [[:alnum:]])? # Ascii-only domain
                ( \. [[:alnum:]] ([-[:alnum:]]* [[:alnum:]])? )*
        )"#
    )
    .unwrap();
    static ref HOSTNAME_UTF8: Regex = Regex::new(r#"^([-.[:alnum:]]|[[:^ascii:]])+"#).unwrap();
    // For ascii-only or utf-8 domains, any prefix of such would still
    // match the regex, so there's no need to handle them here.
    static ref HOSTNAME_PREFIX: Regex = Regex::new(
        r#"(?x) ^(
            \[ (
                I ( P ( v ( 6 ( : [0-9a-fA-F:.]* )? )? )? )? |
                [0-9.]+
            )?
        )?"#
    )
    .unwrap();

    // Note: we have to disable the x flag here so that the # in the
    // middle of the character class does not get construed as a
    // comment
    static ref LOCALPART_ASCII: Regex = Regex::new(
        r#"(?x) ^(
            " ( [[:ascii:]&&[^\\"[:cntrl:]]] |       # Quoted-string localpart
                \\ [[:ascii:]&&[:^cntrl:]] )* " |
            (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~]+(?x) # Dot-string localpart
                ( \. (?-x)[a-zA-Z0-9!#$%&'*+/=?^_`{|}~-]+(?x) )*
        )"#
    ).unwrap();

    // Note: we have to disable the x flag here so that the # in the
    // middle of the character class does not get construed as a
    // comment
    static ref LOCALPART_UTF8: Regex = Regex::new(
        r#"(?x) ^(
            " ( [^\\"[:cntrl:]] | \\ [[:^cntrl:]] )* " |                # Quoted-string localpart
            ( (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~](?x) | [[:^ascii:]] )+ # Dot-string localpart
                ( \. ( (?-x)[a-zA-Z0-9!#$%&'*+-/=?^_`{|}~](?x) | [[:^ascii:]] )+ )*
        )"#
    ).unwrap();

    static ref LOCALPART_PREFIX: Regex = Regex::new(
        r#"(?x) ^"#
    ).unwrap(); // TODO: make correct, if we don't move to regex_automata after all
}

// TODO: ideally the regex crate would provide us with a way to know
// whether the match failed due to end of input being reached or due to
// the input not matching -- the DFA most likely knows this already,
// it's just not reported in the API.
fn apply_regex<'a>(
    regex: &'a Regex,
    prefix: &'a Regex,
) -> impl 'a + Fn(&[u8]) -> IResult<&[u8], &[u8]> {
    move |buf: &[u8]| {
        if let Some(res) = regex.find(buf) {
            let r = res.range();
            return Ok((&buf[r.end..], &buf[r]));
        }

        if let Some(res) = prefix.find(buf) {
            if res.range().end == buf.len() {
                return Err(nom::Err::Incomplete(nom::Needed::Unknown));
            }
        }

        Err(nom::Err::Error((buf, nom::error::ErrorKind::Verify)))
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
#[derive(Debug, Eq)]
pub enum Hostname<S = String> {
    Utf8Domain { raw: S, punycode: String },
    AsciiDomain { raw: S },
    Ipv6 { raw: S, ip: Ipv6Addr },
    Ipv4 { raw: S, ip: Ipv4Addr },
}

impl<S> Hostname<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], Hostname<S>>
    where
        S: From<&'a str>,
    {
        alt((
            map_opt(
                apply_regex(&HOSTNAME_ASCII, &HOSTNAME_PREFIX),
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
                apply_regex(&HOSTNAME_UTF8, &HOSTNAME_PREFIX),
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
        ))(buf)
    }
}

impl<S> Hostname<S> {
    pub fn raw(&self) -> &S {
        match self {
            Hostname::Utf8Domain { raw, .. } => raw,
            Hostname::AsciiDomain { raw, .. } => raw,
            Hostname::Ipv4 { raw, .. } => raw,
            Hostname::Ipv6 { raw, .. } => raw,
        }
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
#[derive(Debug, Eq, PartialEq)]
pub enum Localpart<S> {
    Ascii { raw: S },
    Quoted { raw: S },
    Utf8 { raw: S },
    QuotedUtf8 { raw: S },
}

impl<S> Localpart<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], Localpart<S>>
    where
        S: From<&'a str>,
    {
        alt((
            map(
                apply_regex(&LOCALPART_ASCII, &LOCALPART_PREFIX),
                |b: &[u8]| {
                    // The below unsafe is OK, thanks to our regex
                    // validating that `b` is proper ascii (and thus
                    // utf-8)
                    let s = unsafe { str::from_utf8_unchecked(b) };

                    if b[0] != b'"' {
                        return Localpart::Ascii { raw: s.into() };
                    } else {
                        return Localpart::Quoted { raw: s.into() };
                    }
                },
            ),
            map(
                apply_regex(&LOCALPART_UTF8, &LOCALPART_PREFIX),
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
        ))(buf)
    }
}

// TODO: Add tests for localpart!

#[derive(Debug, Eq, PartialEq)]
pub struct Email<S> {
    pub localpart: Localpart<S>,
    pub hostname: Hostname<S>,
}

impl<S> Email<S> {
    pub fn parse<'a>(buf: &'a [u8]) -> IResult<&'a [u8], Email<S>>
    where
        S: From<&'a str>,
    {
        map(
            tuple((Localpart::parse, tag(b"@"), Hostname::parse)),
            |(localpart, _, hostname)| Email {
                localpart,
                hostname,
            },
        )(buf)
    }
}

// TODO: Add tests for email!

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
        let tests: &[(&[u8], usize, Hostname<&str>)] = &[
            (b"foo--bar", 0, Hostname::AsciiDomain { raw: "foo--bar" }),
            (b"foo.bar.baz", 0, Hostname::AsciiDomain {
                raw: "foo.bar.baz",
            }),
            (b"1.2.3.4", 0, Hostname::AsciiDomain { raw: "1.2.3.4" }),
            (b"[123.255.37.2]", 0, Hostname::Ipv4 {
                raw: "[123.255.37.2]",
                ip: "123.255.37.2".parse().unwrap(),
            }),
            (b"[IPv6:0::ffff:8.7.6.5]", 0, Hostname::Ipv6 {
                raw: "[IPv6:0::ffff:8.7.6.5]",
                ip: "0::ffff:8.7.6.5".parse().unwrap(),
            }),
            ("élégance.fr".as_bytes(), 0, Hostname::Utf8Domain {
                raw: "élégance.fr",
                punycode: "xn--lgance-9uab.fr".into(),
            }),
            (b"foo.-bar.baz", 9, Hostname::AsciiDomain { raw: "foo" }),
            (b"foo.bar.-baz", 5, Hostname::AsciiDomain { raw: "foo.bar" }),
            /* TODO: add a test like this once we get proper delimiters
             * ("papier-maché.fr".as_bytes(), Hostname::Utf8Domain {
             * raw: "papier-maché.fr",
             * punycode: "-9uab.fr".into(),
             * }),
             */
        ];
        for (inp, remlen, out) in tests {
            let parsed = Hostname::parse(inp);
            println!(
                "\nTest: {:?}\nParse result: {:?}\nExpected: {:?}",
                show_bytes(inp),
                parsed,
                out
            );
            match parsed {
                Ok((rem, host)) => assert!(rem.len() == *remlen && host.deep_equal(out)),
                x => panic!("Unexpected hostname result: {:?}", x),
            }
        }
    }

    #[test]
    fn hostname_incomplete() {
        let tests: &[&[u8]] = &[b"[1.2", b"[IPv6:0::"];
        for inp in tests {
            let r = Hostname::<&str>::parse(inp);
            println!("{:?}:  {:?}", show_bytes(inp), r);
            assert!(r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn hostname_invalid() {
        let tests: &[&[u8]] = &[
            b"-foo.bar",                 // No sub-domain starting with a dash
            b"\xFF",                     // No invalid utf-8
            "élégance.-fr".as_bytes(), // No dashes in utf-8 either
        ];
        for inp in tests {
            let r = Hostname::<String>::parse(inp);
            println!("{:?}: {:?}", show_bytes(inp), r);
            assert!(!r.unwrap_err().is_incomplete());
        }
    }

    #[test]
    fn localpart_valid() {
        let tests: &[(&[u8], Localpart<&str>)] = &[
            (b"helloooo", Localpart::Ascii { raw: "helloooo" }),
            (b"test.ing", Localpart::Ascii { raw: "test.ing" }),
            (br#""hello""#, Localpart::Quoted { raw: r#""hello""# }),
            (
                br#""hello world. This |$ a g#eat place to experiment !""#,
                Localpart::Quoted {
                    raw: r#""hello world. This |$ a g#eat place to experiment !""#,
                },
            ),
            (
                br#""\"escapes\", useless like h\ere, except for quotes and backslashes\\""#,
                Localpart::Quoted {
                    raw: r#""\"escapes\", useless like h\ere, except for quotes and backslashes\\""#,
                },
            ),
            // TODO: add Utf8 tests
        ];
        for (inp, out) in tests {
            match Localpart::parse(inp) {
                Ok((rem, res)) if rem.len() == 0 && res == *out => (),
                x => panic!("Unexpected dot_string result: {:?}", x),
            }
        }
    }

    // TODO: add incomplete and invalid localpart tests
}
