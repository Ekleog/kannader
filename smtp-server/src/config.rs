use std::{future::Future, pin::Pin};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::Stream;

use smtp_message::{DataStream, Email, ReplyCode, SmtpString};

use crate::{
    decision::Decision,
    metadata::{ConnectionMetadata, MailMetadata},
};

#[async_trait]
pub trait Config: Send + Sync {
    type ConnectionUserMeta: Send;
    type MailUserMeta: Send;

    // TODO: this could have a default implementation if we were able to have a
    // default type of () for MailUserMeta without requiring unstable
    async fn new_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Self::MailUserMeta;

    async fn filter_from(
        &self,
        from: &mut Option<Email>,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision;

    async fn filter_to(
        &self,
        to: &mut Email,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision;

    #[allow(unused_variables)]
    async fn filter_data(
        &self,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision {
        Decision::Accept
    }

    // Note: handle_mail *must* consume all of `stream` and call its `complete`
    // method to check that the data stream was properly closed and did not just
    // EOF too early. Things will panic otherwise.
    // TODO: this can't be an async fn due to https://github.com/rust-lang/rust/issues/71058
    fn handle_mail<'a, S>(
        &'a self,
        stream: &'a mut DataStream<S>,
        meta: MailMetadata<Self::MailUserMeta>,
        conn_meta: &'a mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        S: 'a + Send + Unpin + Stream<Item = BytesMut>;

    fn hostname(&self) -> SmtpString;

    fn banner(&self) -> SmtpString {
        SmtpString::from_static(b"Service ready")
    }

    fn welcome_banner(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::SERVICE_READY,
            self.hostname() + SmtpString::from_static(b" ") + self.banner(),
        )
    }

    fn okay(&self) -> (ReplyCode, SmtpString) {
        (ReplyCode::OKAY, SmtpString::from_static(b"Okay"))
    }

    fn mail_okay(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn rcpt_okay(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn data_okay(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::START_MAIL_INPUT,
            SmtpString::from_static(b"Start mail input; end with <CRLF>.<CRLF>"),
        )
    }

    fn mail_accepted(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn bad_sequence(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::BAD_SEQUENCE,
            SmtpString::from_static(b"Bad sequence of commands"),
        )
    }

    fn already_in_mail(&self) -> (ReplyCode, SmtpString) {
        self.bad_sequence()
    }

    fn rcpt_before_mail(&self) -> (ReplyCode, SmtpString) {
        self.bad_sequence()
    }

    fn data_before_rcpt(&self) -> (ReplyCode, SmtpString) {
        self.bad_sequence()
    }

    fn data_before_mail(&self) -> (ReplyCode, SmtpString) {
        self.bad_sequence()
    }

    fn command_unimplemented(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::COMMAND_UNIMPLEMENTED,
            SmtpString::from_static(b"Command not implemented"),
        )
    }

    fn command_unrecognized(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::COMMAND_UNRECOGNIZED,
            SmtpString::from_static(b"Command not recognized"),
        )
    }
}
