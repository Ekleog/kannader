#[macro_use]
extern crate nom;

mod helpers;
mod parse_helpers;

mod data;
mod ehlo;
mod expn;
mod helo;
mod mail;
mod rcpt;
mod rset;
mod vrfy;

mod command;

pub use data::DataCommand;
pub use ehlo::EhloCommand;
pub use expn::ExpnCommand;
pub use helo::HeloCommand;
pub use mail::MailCommand;
pub use rcpt::RcptCommand;
pub use rset::RsetCommand;
pub use vrfy::VrfyCommand;
pub use command::command as parse_command; // TODO: give a nicer interface

// TODO: escape initial '.' in DataItem by adding another '.' in front (and opposite when
// receiving)
