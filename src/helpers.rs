use std::str;
use nom;

#[derive(Fail, Debug)]
pub enum ParseError {
    #[fail(display = "Input contains {} trailing characters", _0)]
    DidNotConsumeEverything(usize),

    #[fail(display = "Parse error")]
    ParseError(nom::Err),

    #[fail(display = "Input appears to be incomplete")]
    IncompleteString(nom::Needed),
}

pub fn bytes_to_dbg(b: &[u8]) -> String {
    if let Ok(s) = str::from_utf8(b) {
        format!("b\"{}\"", s.chars().flat_map(|x| x.escape_default()).collect::<String>())
    } else {
        format!("{:?}", b)
    }
}
