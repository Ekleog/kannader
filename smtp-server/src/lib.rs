#![type_length_limit = "4194304"]

// TODO(low): add in deadlines
extern crate bytes;
extern crate itertools;
extern crate smtp_message;
extern crate tokio;

mod config;
mod helpers;
mod interact;

pub use config::Config;
pub use helpers::{ConnectionMetadata, Decision, MailMetadata, Refusal};
pub use interact::interact;
