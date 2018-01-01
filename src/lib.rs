#[macro_use]
extern crate nom;

use std::{fmt, str};

mod parser;

// TODO: transform all CR or LF to CRLF
// TODO: return "500 syntax error - invalid character" if receiving a non-ASCII character in
// envelope commands
// TODO: escape initial '.' in DataItem by adding another '.' in front (and opposite when
// receiving)

#[cfg_attr(test, derive(PartialEq))]
pub struct DataCommand<'a> {
    // Still SMTP-escaped (ie. leading ‘.’ doubled) message
    data: &'a [u8],
}

#[cfg_attr(test, derive(PartialEq))]
pub struct MailCommand<'a> {
    from: &'a [u8],
}

#[cfg_attr(test, derive(PartialEq))]
pub struct RcptCommand<'a> {
    // TO: parameter with the “@ONE,@TWO:” portion removed, as per RFC5321 Appendix C
    to: &'a [u8],
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub enum Command<'a> {
    Mail(MailCommand<'a>), // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Rcpt(RcptCommand<'a>), // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
    Data(DataCommand<'a>), // DATA <CRLF>
}

pub struct Reply<'a> {
    code: u16,
    text: &'a [u8],
}

fn bytes_to_dbg(b: &[u8]) -> String {
    if let Ok(s) = str::from_utf8(b) {
        format!("b\"{}\"", s.chars().flat_map(|x| x.escape_default()).collect::<String>())
    } else {
        format!("{:?}", b)
    }
}

impl<'a> fmt::Debug for DataCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "DataCommand {{ data: {} }}", bytes_to_dbg(self.data))
    }
}

impl<'a> fmt::Debug for MailCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "MailCommand {{ from: {} }}", bytes_to_dbg(self.from))
    }
}

impl<'a> fmt::Debug for RcptCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "RcptCommand {{ to: {} }}", bytes_to_dbg(self.to))
    }
}
