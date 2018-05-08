#![type_length_limit = "4194304"]

// TODO(low): add in deadlines
extern crate bytes;
extern crate itertools;
extern crate smtp_message;
extern crate tokio;

mod config;
mod crlflines;
mod decision;
mod interact;
mod metadata;
mod sendreply;
mod stupidfut;

pub use config::Config;
pub use decision::{Decision, Refusal};
pub use interact::interact;
pub use metadata::{ConnectionMetadata, MailMetadata};
