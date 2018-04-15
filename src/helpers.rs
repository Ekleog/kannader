use std::fmt;
use nom;
use nom::{Needed, IResult};

#[derive(Fail, Debug, Clone)]
pub enum ParseError {
    DidNotConsumeEverything(usize),
    ParseError(
        #[cause]
        nom::Err
    ),
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
    LineTooLong { length: usize, limit: usize },
    DisallowedByte { b: u8, pos: usize },
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
