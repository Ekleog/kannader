use nom::{self, IResult, Needed};
use std::{borrow::Cow, fmt, slice};

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
