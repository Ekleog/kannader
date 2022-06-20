use std::io;

use smtp_message::{Email, Hostname, Reply};

pub mod reply;

// TODO: add sanity checks that Accept is a 2xx reply, and Reject/Kill are not
#[must_use]
#[derive(Debug)]
pub enum Decision<T> {
    Accept {
        reply: Reply,
        res: T,
    },
    Reject {
        reply: Reply,
    },
    Kill {
        reply: Option<Reply>,
        res: io::Result<()>,
    },
}

// TODO: add sanity checks that Accept is a 2xx reply, and Reject/Kill are not
// TODO: merge with Decision (blocked on https://github.com/serde-rs/serde/issues/1940)
#[must_use]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum SerializableDecision<T> {
    Accept {
        reply: Reply,
        res: T,
    },
    Reject {
        reply: Reply,
    },
    Kill {
        reply: Option<Reply>,
        res: Result<(), String>,
    },
}

impl<T> From<SerializableDecision<T>> for Decision<T> {
    fn from(d: SerializableDecision<T>) -> Decision<T> {
        match d {
            SerializableDecision::Accept { reply, res } => Decision::Accept { reply, res },
            SerializableDecision::Reject { reply } => Decision::Reject { reply },
            SerializableDecision::Kill { reply, res } => Decision::Kill {
                reply,
                res: res.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg)),
            },
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MailMetadata<U> {
    pub user: U,
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct HelloInfo {
    /// is_extended: whether we are running Extended SMTP (ESMTP) or LMTP,
    /// rather than plain SMTP
    pub is_extended: bool,
    pub hostname: Hostname,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ConnectionMetadata<U> {
    pub user: U,
    pub hello: Option<HelloInfo>,
    pub is_encrypted: bool,
}
