use bytes::BytesMut;
use smtp_message::{ReplyCode, SmtpString};
use tokio::prelude::*;

use mail::Mail;

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
pub trait Transport<M>: Sized + Sync + Send + 'static {
    fn send<S: Stream<Item = BytesMut, Error = ()>>(
        &self,
        mail: Mail<S, M>,
    ) -> Box<Future<Item = (), Error = (ReplyCode, SmtpString)> + Send>;
}
