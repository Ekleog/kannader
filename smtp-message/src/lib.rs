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

mod helpers;
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

pub use command::Command;
pub use helpers::{opt_email_repr, Email, ParseError, Prependable, SmtpString, StreamExt};
pub use reply::{IsLastLine, Reply, ReplyCode};

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
