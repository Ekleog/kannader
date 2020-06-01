#![type_length_limit = "4194304"]

mod config;
mod crlflines;
mod interact;
mod metadata;
mod sendreply;

pub use config::Config;
pub use interact::interact;
pub use metadata::{ConnectionMetadata, MailMetadata};

#[must_use]
pub enum Decision {
    Accept,
    Reject(Reply<Cow<'static, str>>),
}
