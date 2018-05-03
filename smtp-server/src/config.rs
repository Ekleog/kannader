use bytes::BytesMut;
use tokio::prelude::*;

use smtp_message::*;

use helpers::*;

// TODO: add new_mail called before filter_from
pub trait Config<U> {
    fn filter_from(&mut self, from: &Option<Email>, conn_meta: &ConnectionMetadata<U>) -> Decision;

    fn filter_to(
        &mut self,
        to: &Email,
        meta: &MailMetadata,
        conn_meta: &ConnectionMetadata<U>,
    ) -> Decision;

    // TODO: When Rust allows it, replace this Box by impl Trait syntax
    fn handle_mail<'a, S>(
        &'a mut self,
        stream: DataStream<S>,
        meta: MailMetadata<'static>,
        conn_meta: &ConnectionMetadata<U>,
    ) -> Box<'a + Future<Item = (&'a mut Self, Option<Prependable<S>>, Decision), Error = ()>>
    where
        Self: 'a,
        S: 'a + Stream<Item = BytesMut, Error = ()>;
}
