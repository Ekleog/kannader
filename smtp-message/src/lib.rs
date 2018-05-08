extern crate bytes;
#[macro_use]
extern crate nom;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate tokio;

#[cfg(test)]
#[macro_use]
extern crate quickcheck;

mod builderror;
mod byteslice;
mod domain;
mod email;
mod parseresult;
mod smtpstring;
mod spparameters;
mod streamext;

mod parse_helpers;

mod data;
mod ehlo;
mod expn;
mod helo;
mod help;
mod mail;
mod noop;
mod quit;
mod rcpt;
mod rset;
mod vrfy;

mod command;
mod reply;

pub use byteslice::ByteSlice;
pub use email::{opt_email_repr, Email}; // TODO(low): opt_email_repr has nothing to do here
pub use parseresult::ParseError;
pub use smtpstring::SmtpString;
pub use streamext::{Prependable, StreamExt};

pub use command::Command;
pub use reply::{IsLastLine, ReplyCode, ReplyLine};

pub use data::{DataCommand, DataSink, DataStream};
pub use ehlo::EhloCommand;
pub use expn::ExpnCommand;
pub use helo::HeloCommand;
pub use help::HelpCommand;
pub use mail::MailCommand;
pub use noop::NoopCommand;
pub use quit::QuitCommand;
pub use rcpt::RcptCommand;
pub use rset::RsetCommand;
pub use vrfy::VrfyCommand;

// TODO: grep for '::*' and try to rationalize imports
