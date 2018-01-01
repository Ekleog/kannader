#[macro_use]
extern crate nom;

mod helpers;
mod parse_helpers;

mod data;
mod ehlo;
mod helo;
mod mail;
mod rcpt;

mod parser;

pub use data::DataCommand;
pub use ehlo::EhloCommand;
pub use helo::HeloCommand;
pub use mail::MailCommand;
pub use rcpt::RcptCommand;
pub use parser::command as parse_command; // TODO: give a nicer interface

// TODO: escape initial '.' in DataItem by adding another '.' in front (and opposite when
// receiving)

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub enum Command<'a> {
    Data(DataCommand<'a>), // DATA <CRLF>
    Ehlo(EhloCommand<'a>), // EHLO <domain> <CRLF>
    Helo(HeloCommand<'a>), // HELO <domain> <CRLF>
    Mail(MailCommand<'a>), // MAIL FROM:<@ONE,@TWO:JOE@THREE> [SP <mail-parameters>] <CRLF>
    Rcpt(RcptCommand<'a>), // RCPT TO:<@ONE,@TWO:JOE@THREE> [SP <rcpt-parameters] <CRLF>
}
