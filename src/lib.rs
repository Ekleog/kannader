#[macro_use]
extern crate nom;

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

pub use data::DataCommand;
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
pub use command::command as parse_command; // TODO: give a nicer interface

// TODO: escape initial '.' in DataItem by adding another '.' in front (and opposite when
// receiving)
