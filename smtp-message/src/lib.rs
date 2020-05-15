use std::{
    net::{Ipv4Addr, Ipv6Addr},
    str,
};

use lazy_static::lazy_static;
use nom::IResult;
use regex::bytes::Regex;

lazy_static! {
    static ref HOSTNAME_ASCII: Regex = Regex::new(
        r#"(?x) ^(
            \[IPv6: [0-9a-fA-F:.]+ \] |                          # Ipv6
            \[ [0-9.]+ \] |                                      # Ipv4
            [[:alnum:]] ([a-zA-Z0-9-]* [[:alnum:]])?             # Ascii-only domain
                ( \. [[:alnum:]] ([a-zA-Z0-9-]* [[:alnum:]])? )*
        )"#
    )
    .unwrap();
    static ref HOSTNAME_UTF8: Regex = Regex::new(r#"^([a-zA-Z0-9.-]|[[:^ascii:]])+"#).unwrap();
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
        if let Some(res) = HOSTNAME_ASCII.find(buf) {
            let r = res.range();
            let rem = &buf[r.end..];

            // The three below unsafe are OK, thanks to our regex validating that `res` is
            // proper ascii (and thus utf-8)
            let res = unsafe { str::from_utf8_unchecked(res.as_bytes()) };

            if buf[r.start] != b'[' {
                return Ok((rem, Hostname::AsciiDomain { raw: res.into() }));
            } else if buf[r.start + 1] == b'I' {
                let ip = unsafe { str::from_utf8_unchecked(&buf[r.start + 6..r.end - 1]) };
                let ip = ip
                    .parse::<Ipv6Addr>()
                    .map_err(|_| nom::Err::Error((buf, nom::error::ErrorKind::Verify)))?;

                return Ok((rem, Hostname::Ipv6 {
                    raw: res.into(),
                    ip,
                }));
            } else {
                let ip = unsafe { str::from_utf8_unchecked(&buf[r.start + 1..r.end - 1]) };
                let ip = ip
                    .parse::<Ipv4Addr>()
                    .map_err(|_| nom::Err::Error((buf, nom::error::ErrorKind::Verify)))?;

                return Ok((rem, Hostname::Ipv4 {
                    raw: res.into(),
                    ip,
                }));
            }
        }

        // Poor luck, looks like we're having an actual IDNA domain name
        // TODO: looks like idna exposes only an allocating method for validating an
        // IDNA domain name. Maybe it'd be possible to get them to expose a
        // validation-only function? Or maybe not.
        if let Some(res) = HOSTNAME_UTF8.find(buf) {
            // The below unsafe is OK, thanks to our regex never disabling the `u` flag and
            // thus validating that the match is proper utf-8
            let raw = unsafe { str::from_utf8_unchecked(res.as_bytes()) };

            let punycode = idna::Config::default()
                .use_std3_ascii_rules(true)
                .verify_dns_length(true)
                .check_hyphens(true)
                .to_ascii(raw)
                .map_err(|_| nom::Err::Error((buf, nom::error::ErrorKind::Verify)))?;

            return Ok((&buf[res.range().end..], Hostname::Utf8Domain {
                raw: raw.into(),
                punycode,
            }));
        }

        // Found no match in either of HOSTNAME_ASCII or HOSTNAME_UTF8. Either
        // this is due to us not having enough data yet, or it is due to
        // invalid data. Let's use the prefix regex to know it.
        //
        // TODO: ideally the regex crate would provide us with a way to know
        // whether the match failed due to end of input being reached or due to
        // the input not matching -- the DFA most likely knows this already,
        // it's just not reported in the API.
        if let Some(res) = HOSTNAME_PREFIX.find(buf) {
            #[cfg(test)]
            println!(
                "match prefix regex with range {:?}: {:?} in buf {:?} with regex {:?}",
                res.range(),
                tests::show_bytes(res.as_bytes()),
                tests::show_bytes(buf),
                HOSTNAME_PREFIX.as_str()
            );
            if res.range().end == buf.len() {
                return Err(nom::Err::Incomplete(nom::Needed::Unknown));
            }
        }

        // Looks like the current buffer doesn't match the hostname prefix. Let's report
        // an error, as more data won't make it possible to get a match
        return Err(nom::Err::Error((buf, nom::error::ErrorKind::Verify)));
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
        let tests: &[(&[u8], Hostname<&str>)] = &[
            (b"foo--bar", Hostname::AsciiDomain { raw: "foo--bar" }),
            (b"foo.bar.baz", Hostname::AsciiDomain { raw: "foo.bar.baz" }),
            (b"1.2.3.4", Hostname::AsciiDomain { raw: "1.2.3.4" }),
            (b"[123.255.37.2]", Hostname::Ipv4 {
                raw: "[123.255.37.2]",
                ip: "123.255.37.2".parse().unwrap(),
            }),
            (b"[IPv6:0::ffff:8.7.6.5]", Hostname::Ipv6 {
                raw: "[IPv6:0::ffff:8.7.6.5]",
                ip: "0::ffff:8.7.6.5".parse().unwrap(),
            }),
            ("élégance.fr".as_bytes(), Hostname::Utf8Domain {
                raw: "élégance.fr",
                punycode: "xn--lgance-9uab.fr".into(),
            }),
            /* TODO: add a test like this once we get proper delimiters
             * ("papier-maché.fr".as_bytes(), Hostname::Utf8Domain {
             * raw: "papier-maché.fr",
             * punycode: "-9uab.fr".into(),
             * }),
             */
        ];
        for (inp, out) in tests {
            let parsed = Hostname::parse(inp);
            println!(
                "\nTest: {:?}\nParse result: {:?}\nExpected: {:?}",
                show_bytes(inp),
                parsed,
                out
            );
            match parsed {
                Ok((rem, host)) => assert!(rem.len() == 0 && host.deep_equal(out)),
                x => panic!("Unexpected hostname result: {:?}", x),
            }
        }
    }

    #[test]
    fn hostname_partial() {
        let tests: &[(&[u8], &str)] = &[(b"foo.-bar.baz", "foo"), (b"foo.bar.-baz", "foo.bar")];
        for (inp, out) in tests {
            assert_eq!(
                Hostname::<String>::parse(inp).unwrap().1,
                Hostname::AsciiDomain { raw: (*out).into() },
            );
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
}
