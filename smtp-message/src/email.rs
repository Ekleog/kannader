use bytes::Bytes;
use std::io;

use crate::{
    byteslice::ByteSlice,
    domain::{hostname, Domain},
    parseresult::{nom_to_result, ParseError},
    sendable::Sendable,
    smtpstring::SmtpString,
};

// TODO: (C) Make equivalent emails (modulo escaping) be equal?
#[derive(Debug, Eq, PartialEq)]
pub struct Email {
    localpart: SmtpString,
    hostname: Option<Domain>,
}

impl Email {
    pub fn new(localpart: SmtpString, hostname: Option<Domain>) -> Email {
        Email {
            localpart,
            hostname,
        }
    }

    pub fn parse(b: ByteSlice) -> Result<Email, ParseError> {
        nom_to_result(email(b))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Email, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(email(ByteSlice::from(&b)))
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

            let mut res = self
                .localpart
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
            SmtpString::from(Bytes::from(res))
        }
    }

    pub fn hostname(&self) -> &Option<Domain> {
        &self.hostname
    }
}

impl Sendable for Email {
    // TODO: (B) only store the overall string and a pointer to the @
    fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        w.write_all(&self.localpart.bytes()[..])?;
        if let Some(ref host) = self.hostname {
            w.write_all(b"@")?;
            host.send_to(w)?;
        }
        Ok(())
    }
}

impl Sendable for Option<Email> {
    fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        if let Some(e) = self {
            e.send_to(w)
        } else {
            Ok(())
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult;

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
                h.map(|x| Domain::parse_slice(x).unwrap()),
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
        let tests: Vec<(&[u8], (&[u8], Option<&[u8]>))> = vec![
            (
                b"@foo.bar,@baz.quux:test@example.org",
                (b"test", Some(b"example.org")),
            ),
            (b"foo.bar@baz.quux", (b"foo.bar", Some(b"baz.quux"))),
        ];
        for (inp, (local, host)) in tests.into_iter() {
            let b = Bytes::from(inp);
            match address_in_path(ByteSlice::from(&b)) {
                IResult::Done(rem, res) => assert!(
                    rem.len() == 0
                        && res.raw_localpart().bytes() == local
                        && res.hostname() == &host.map(|h| Domain::parse_slice(h).unwrap())
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
                host.map(|x| Domain::parse_slice(x).unwrap())
            );
        }
    }
}
