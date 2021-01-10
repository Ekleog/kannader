#![feature(core_intrinsics, destructuring_assignment, never_type)]

use std::{mem, path::PathBuf};

use static_assertions::const_assert_eq;

// Hack: make kannader-config-macros, designed to work outside of
// kannader-config, also work inside kannader-config
use crate as kannader_config;

// Check that we're building on a 32-bit platform
const_assert_eq!(mem::size_of::<usize>(), mem::size_of::<u32>());

// Reexport implementation macros
pub use kannader_config_macros::{
    client_config_implement_guest_server, implement_guest, queue_config_implement_guest_server,
    server_config_implement_guest_server,
};

// Reexport useful types
pub mod queue {
    pub use smtp_queue_types::{QueueId, ScheduleInfo};
}
pub mod server {
    pub use smtp_server_types::{HelloInfo, SerializableDecision};

    pub type ConnMeta = smtp_server_types::ConnectionMetadata<Vec<u8>>;
    pub type MailMeta = smtp_server_types::MailMetadata<Vec<u8>>;
}
pub use smtp_server_types::reply;

pub trait Config {
    fn setup(path: PathBuf) -> Self;
}

kannader_config_macros::client_config_implement_trait!();
kannader_config_macros::queue_config_implement_trait!();
kannader_config_macros::server_config_implement_trait!();
kannader_config_macros::tracing_implement_guest_client!(tracing_impl);

#[macro_export]
macro_rules! trace {
    ($($tt:tt)*) => {
        $crate::log!(trace, $($tt)*);
    };
}

#[macro_export]
macro_rules! debug {
    ($($tt:tt)*) => {
        $crate::log!(debug, $($tt)*);
    };
}

#[macro_export]
macro_rules! info {
    ($($tt:tt)*) => {
        $crate::log!(info, $($tt)*);
    };
}

#[macro_export]
macro_rules! warn {
    ($($tt:tt)*) => {
        $crate::log!(warn, $($tt)*);
    };
}

#[macro_export]
macro_rules! error {
    ($($tt:tt)*) => {
        $crate::log!(error, $($tt)*);
    };
}

#[macro_export]
macro_rules! log {
    ($ltype:ident, { $($k:ident: $vtype:tt $v:expr),* $(,)* }, $msg:expr $(, $arg:expr)* $(,)*) => {
        // Note: there is nothing good to do in case logging fails, so let's ignore the error.
        let _ = $crate::tracing_impl::$ltype(
            // TODO: use a hash map literal when there is one
            vec![$(
                (String::from(stringify!($k)), $crate::__log_format!($vtype $v))
            ),*].into_iter().collect(),
            format!($msg, $($arg),*),
        );
    };

    ($ltype:ident, $msg:expr $(, $arg:expr)* $(,)*) => {
        $crate::log!($ltype, {}, $msg $(, $arg)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_format {
    (? $v:expr) => {
        format!("{:?}", $v);
    };
    (% $v:expr) => {
        format!("{}", $v);
    };
}
