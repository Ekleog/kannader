use bytes::BytesMut;
use smtp_message::Email;
use tokio::prelude::*;

pub struct Mail<S: Stream<Item = BytesMut, Error = ()>, M> {
    pub from: Option<Email>,
    pub to: Vec<Email>,
    pub data: S,
    pub metadata: M,
}

pub trait QueuedMail {}

pub trait InflightMail {}
