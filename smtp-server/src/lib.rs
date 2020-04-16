#![type_length_limit = "4194304"]

// TODO: (B) add deadlines
extern crate bytes;
extern crate futures;
extern crate itertools;
extern crate smtp_message;

mod config;
mod crlflines;
mod decision;
mod interact;
mod metadata;
mod sendreply;

pub use config::Config;
pub use decision::{Decision, Refusal};
pub use interact::interact;
pub use metadata::{ConnectionMetadata, MailMetadata};
