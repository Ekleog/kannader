use bytes::BytesMut;
use smtp_message::Email;
use tokio::prelude::*;

pub struct Mail<S: Stream<Item = BytesMut, Error = ()>, M> {
    pub from: Option<Email>,
    pub to: Vec<Email>,
    pub data: S,
    pub metadata: M,
}

pub trait QueuedMail<M>: 'static {}

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
pub trait InflightMail<M>: Send + 'static {
    fn get_mail(
        &self,
    ) -> Box<
        dyn Future<Item = Mail<Box<dyn Stream<Item = BytesMut, Error = ()>>, M>, Error = ()> + Send,
    >;
}

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
pub trait FoundInflightMail<M>: Send + 'static {}
