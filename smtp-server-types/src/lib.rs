use std::{borrow::Cow, io};

use smtp_message::{Email, Hostname, Reply};

// TODO: make it Accept(Reply<Cow<'static, str>>)
#[must_use]
pub enum Decision {
    Accept,
    Reject(Reply<Cow<'static, str>>),
    Kill {
        reply: Option<Reply<Cow<'static, str>>>,
        res: io::Result<()>,
    },
}

// TODO: make it Accept(Reply<Cow<'static, str>>)
#[must_use]
#[derive(serde::Deserialize, serde::Serialize)]
pub enum SerializableDecision {
    Accept,
    Reject(Reply<Cow<'static, str>>),
    Kill {
        reply: Option<Reply<Cow<'static, str>>>,
        res: Result<(), Cow<'static, str>>,
    },
}

impl From<SerializableDecision> for Decision {
    fn from(d: SerializableDecision) -> Decision {
        match d {
            SerializableDecision::Accept => Decision::Accept,
            SerializableDecision::Reject(r) => Decision::Reject(r),
            SerializableDecision::Kill { reply, res } => Decision::Kill {
                reply,
                res: res.map_err(|msg| io::Error::new(io::ErrorKind::Other, msg)),
            },
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct MailMetadata<U> {
    pub user: U,
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct HelloInfo {
    pub is_ehlo: bool,
    pub hostname: Hostname,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ConnectionMetadata<U> {
    pub user: U,
    pub hello: Option<HelloInfo>,
    pub is_encrypted: bool,
}
