use std::{mem, path::PathBuf};

use static_assertions::const_assert_eq;

// Check that we're building on a 32-bit platform
const_assert_eq!(mem::size_of::<usize>(), mem::size_of::<u32>());

// Reexport implementation macros
pub use kannader_config_types::{implement_guest, server_config_implement_guest_server};

// Reexport useful types
pub use kannader_config_types::server;
pub use smtp_server_types::reply;

pub trait Config {
    fn setup(path: PathBuf) -> Self;
}

kannader_config_types::server_config_implement_trait!();
