use bytes::Bytes;

use byteslice::ByteSlice;
use domain::Domain;
use parseresult::{nom_to_result, ParseError};
use smtpstring::SmtpString;

use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct Email {
    localpart: SmtpString,
    hostname:  Option<Domain>,
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
            SmtpString::from(Bytes::from(res))
        }
    }

    pub fn hostname(&self) -> &Option<Domain> {
        &self.hostname
    }

    // TODO: actually store just the overall string and a pointer to the @, not two
    // separate fields
    pub fn as_string(&self) -> SmtpString {
        let mut res = self.localpart.bytes().clone();
        if let Some(ref host) = self.hostname {
            res.extend_from_slice(b"@");
            res.extend_from_slice(&host.as_string().bytes()[..]);
        }
        res.into()
    }
}

pub fn opt_email_repr(e: &Option<Email>) -> SmtpString {
    if let &Some(ref e) = e {
        e.as_string()
    } else {
        (&b""[..]).into()
    }
}
