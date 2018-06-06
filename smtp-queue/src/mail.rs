use bytes::BytesMut;
use smtp_message::Email;
use tokio::prelude::*;

pub struct Mail<S: Stream<Item = BytesMut, Error = ()>, M> {
    pub from: Option<Email>,
    pub to: Vec<Email>,
    pub data: S,
    pub metadata: M,
}

pub trait QueuedMail<M> {}

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
pub trait InflightMail<M> {
    fn get_mail(&self) -> Mail<Box<Stream<Item = BytesMut, Error = ()>>, M>;
}
