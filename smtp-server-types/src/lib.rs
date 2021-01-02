use std::{borrow::Cow, io};

use smtp_message::{Email, Hostname, Reply};

// TODO: add sanity checks that Accept is a 2xx reply, and Reject/Kill are not
#[must_use]
#[derive(Debug)]
pub enum Decision<T> {
    Accept {
        reply: Reply<Cow<'static, str>>,
        res: T,
    },
    Reject {
        reply: Reply<Cow<'static, str>>,
    },
    Kill {
        reply: Option<Reply<Cow<'static, str>>>,
        res: io::Result<()>,
    },
}

// TODO: add sanity checks that Accept is a 2xx reply, and Reject/Kill are not
// TODO: merge with Decision (blocked on https://github.com/serde-rs/serde/issues/1940)
#[must_use]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum SerializableDecision<T> {
    Accept {
        reply: Reply<Cow<'static, str>>,
        res: T,
    },
    Reject {
        reply: Reply<Cow<'static, str>>,
    },
    Kill {
        reply: Option<Reply<Cow<'static, str>>>,
        res: Result<(), Cow<'static, str>>,
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct HelloInfo {
    pub is_ehlo: bool,
    pub hostname: Hostname,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ConnectionMetadata<U> {
    pub user: U,
    pub hello: Option<HelloInfo>,
    pub is_encrypted: bool,
}
