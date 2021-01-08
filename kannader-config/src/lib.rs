#![feature(core_intrinsics, destructuring_assignment, never_type)]

use std::{mem, path::PathBuf};

use static_assertions::const_assert_eq;

// Check that we're building on a 32-bit platform
const_assert_eq!(mem::size_of::<usize>(), mem::size_of::<u32>());

// Reexport implementation macros
pub use kannader_config_macros::{implement_guest, server_config_implement_guest_server};

// Reexport useful types
pub mod server {
    pub use smtp_server_types::{HelloInfo, SerializableDecision};

    pub type ConnMeta = smtp_server_types::ConnectionMetadata<Vec<u8>>;
    pub type MailMeta = smtp_server_types::MailMetadata<Vec<u8>>;
}
pub use smtp_server_types::reply;

pub trait Config {
    fn setup(path: PathBuf) -> Self;
}

kannader_config_macros::server_config_implement_trait!();
kannader_config_macros::tracing_implement_guest_client!(tracing);
