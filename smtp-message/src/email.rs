use bytes::Bytes;
use std::io;

use crate::{
    byteslice::ByteSlice,
    domain::{hostname, Domain},
    parseresult::{nom_to_result, ParseError},
    sendable::Sendable,
    smtpstring::SmtpString,
};

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Email {
    localpart: SmtpString,
    hostname: Option<Domain>,
}

impl Email {
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
}

impl Sendable for Email {
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
