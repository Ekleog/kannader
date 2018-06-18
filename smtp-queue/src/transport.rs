use bytes::BytesMut;
use smtp_message::{ReplyCode, SmtpString};
use tokio::prelude::*;

use mail::Mail;

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
// TODO: (B) for a clean api, the futures should not take ownership and return
// but rather take a reference (when async/await will be done)
pub trait Transport<M>: Sized {
    fn send<S: Stream<Item = BytesMut, Error = ()>>(
        self,
        mail: Mail<S, M>,
    ) -> Box<Future<Item = Self, Error = (Self, ReplyCode, SmtpString)>>;
}
