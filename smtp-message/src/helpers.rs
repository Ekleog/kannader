use nom::{self, IResult, Needed};
use std::{fmt, slice};

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

#[derive(Clone, PartialEq, Eq)]
pub struct SmtpString(Vec<u8>);

impl SmtpString {
    pub fn from_bytes(b: Vec<u8>) -> SmtpString {
        SmtpString(b)
    }

    pub fn copy_bytes(b: &[u8]) -> SmtpString {
        SmtpString::from_bytes(b.to_vec())
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
        self.0
    }

    pub fn copy_chunks(&self, bytes: usize) -> Vec<SmtpString> {
        let mut res = Vec::with_capacity((self.byte_len() + bytes - 1) / bytes);
        let mut it = self.0.iter().cloned();
        for _ in 0..((self.byte_len() + bytes - 1) / bytes) {
            res.push(SmtpString::from_bytes(it.by_ref().take(bytes).collect()));
        }
        res
    }
}

impl fmt::Debug for SmtpString {
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

#[derive(Hash, PartialEq, Eq)]
pub struct DbgBytes<'a>(&'a [u8]);

impl<'a> fmt::Debug for DbgBytes<'a> {
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

pub fn bytes_to_dbg(b: &[u8]) -> DbgBytes {
    DbgBytes(b)
}
