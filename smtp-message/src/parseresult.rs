use nom::{self, IResult, Needed};
use std::fmt;

use crate::byteslice::ByteSlice;

#[derive(Fail, Debug, Clone)]
pub enum ParseError {
    DidNotConsumeEverything(usize),
    ParseError(#[cause] nom::Err),
    IncompleteString(Needed),
}

pub fn nom_to_result<T>(d: nom::IResult<ByteSlice, T>) -> Result<T, ParseError> {
    match d {
        IResult::Done(rem, res) => {
            if rem.len() == 0 {
                Ok(res)
            } else {
                Err(ParseError::DidNotConsumeEverything(rem.len()))
            }
        }
        IResult::Error(e) => Err(ParseError::ParseError(e)),
        IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ParseError::*;
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
