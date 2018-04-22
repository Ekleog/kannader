use nom::{self, IResult, Needed};
use std::{borrow::Cow, fmt, slice};

use parse_helpers::*;

#[derive(Fail, Debug, Clone)]
pub enum ParseError {
    DidNotConsumeEverything(usize),
    ParseError(#[cause] nom::Err),
    IncompleteString(Needed),
}

pub fn nom_to_result<'a, T>(d: nom::IResult<&'a [u8], T>) -> Result<T, ParseError> {
    match d {
        IResult::Done(rem, res) => {
            assert_eq!(rem, b"");
            Ok(res)
        }
        IResult::Error(e) => Err(ParseError::ParseError(e)),
        IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use ParseError::*;
        // TODO: make error display nicer with nom
        match self {
            &DidNotConsumeEverything(rem) => {
                write!(f, "Input contains {} trailing characters", rem)
            }
            &ParseError(ref err) => write!(f, "Parse error: {}", err),
            &IncompleteString(Needed::Unknown) => write!(f, "Input appears to be incomplete"),
            &IncompleteString(Needed::Size(sz)) => {
                write!(f, "Input appears to be missing {} characters", sz)
            }
        }
    }
}

#[derive(Fail, Debug, Clone)]
pub enum BuildError {
    LineTooLong { length: usize, limit:  usize },
    DisallowedByte { b:   u8, pos: usize },
    ContainsNewLine { pos: usize },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &BuildError::LineTooLong { length, limit } => {
                write!(f, "Line is too long: {} (max: {})", length, limit)
            }
            &BuildError::DisallowedByte { b, pos } => {
                write!(f, "Disallowed byte found at position {}: {}", pos, b)
            }
            &BuildError::ContainsNewLine { pos } => write!(f, "New line found at position {}", pos),
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
pub struct SmtpString<'a>(Cow<'a, [u8]>);

impl<'a> From<&'a [u8]> for SmtpString<'a> {
    fn from(t: &'a [u8]) -> SmtpString<'a> {
        SmtpString(Cow::from(t))
    }
}

impl<'a> From<Vec<u8>> for SmtpString<'a> {
    fn from(t: Vec<u8>) -> SmtpString<'a> {
        SmtpString(Cow::from(t))
    }
}

impl<'a> From<&'a str> for SmtpString<'a> {
    fn from(s: &'a str) -> SmtpString<'a> {
        SmtpString(Cow::from(s.as_bytes()))
    }
}

impl<'a> From<String> for SmtpString<'a> {
    fn from(s: String) -> SmtpString<'a> {
        SmtpString(Cow::from(s.into_bytes()))
    }
}

impl<'a> SmtpString<'a> {
    pub fn take_ownership<'b>(self) -> SmtpString<'b> {
        SmtpString(Cow::from(self.0.into_owned()))
    }

    pub fn borrow<'b>(&'b self) -> SmtpString<'b>
    where
        'a: 'b,
    {
        SmtpString(Cow::from(&*self.0))
    }

    pub fn iter_bytes(&self) -> slice::Iter<u8> {
        self.0.iter()
    }

    pub fn byte_len(&self) -> usize {
        self.0.len()
    }

    pub fn byte(&self, pos: usize) -> u8 {
        self.0[pos]
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_owned()
    }

    pub fn copy_chunks<'b>(&self, bytes: usize) -> Vec<SmtpString<'b>> {
        let mut res = Vec::with_capacity((self.byte_len() + bytes - 1) / bytes);
        let mut it = self.0.iter().cloned();
        for _ in 0..((self.byte_len() + bytes - 1) / bytes) {
            res.push(it.by_ref().take(bytes).collect::<Vec<_>>().into());
        }
        res
    }
}

impl<'a> fmt::Debug for SmtpString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "b\"{}\"",
            self.0
                .iter()
                .flat_map(|x| char::from(*x).escape_default())
                .collect::<String>()
        )
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct Email<'a> {
    localpart: SmtpString<'a>,
    hostname:  Option<SmtpString<'a>>,
}

impl<'a> Email<'a> {
    pub fn new<'b>(localpart: SmtpString<'b>, hostname: Option<SmtpString<'b>>) -> Email<'b> {
        Email {
            localpart,
            hostname,
        }
    }

    pub fn parse<'b>(s: &'b SmtpString<'b>) -> Result<Email<'b>, ParseError> {
        nom_to_result(email(s.as_bytes()))
    }

    pub fn take_ownership<'b>(self) -> Email<'b> {
        Email {
            localpart: self.localpart.take_ownership(),
            hostname:  self.hostname.map(|x| x.take_ownership()),
        }
    }

    pub fn borrow<'b>(&'b self) -> Email<'b>
    where
        'a: 'b,
    {
        Email {
            localpart: self.localpart.borrow(),
            hostname:  self.hostname.as_ref().map(|x| x.borrow()),
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
            self.localpart.borrow()
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
            res.into()
        }
    }

    pub fn hostname(&self) -> &Option<SmtpString> {
        &self.hostname
    }

    pub fn into_smtp_string<'b>(self) -> SmtpString<'b> {
        let mut res = self.localpart.into_bytes();
        if let Some(host) = self.hostname {
            res.push(b'@');
            res.extend_from_slice(host.as_bytes());
        }
        res.into()
    }

    pub fn as_smtp_string<'b>(&self) -> SmtpString<'b> {
        let mut res = self.localpart.borrow().into_bytes();
        if let Some(host) = self.hostname.as_ref().map(|x| x.borrow()) {
            res.push(b'@');
            res.extend_from_slice(host.as_bytes());
        }
        res.into()
    }
}

pub fn opt_email_repr<'a>(e: &Option<Email>) -> SmtpString<'a> {
    if let &Some(ref e) = e {
        e.as_smtp_string()
    } else {
        (&b""[..]).into()
    }
}
