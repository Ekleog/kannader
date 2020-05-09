use std::pin::Pin;

use bytes::BytesMut;
use futures::{future, Future, Stream};

use smtp_message::{DataStream, Email, ReplyCode, SmtpString};

use crate::{
    decision::Decision,
    metadata::{ConnectionMetadata, MailMetadata},
};

pub trait Config<U> {
    fn new_mail<'a>(&'a mut self) -> Pin<Box<dyn 'a + Send + Future<Output = ()>>> {
        Box::pin(future::ready(()))
    }

    fn filter_from<'a>(
        &'a mut self,
        from: &'a mut Option<Email>,
        conn_meta: &'a mut ConnectionMetadata<U>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Decision>>>;

    fn filter_to<'a>(
        &'a mut self,
        to: &'a mut Email,
        meta: &'a mut MailMetadata,
        conn_meta: &'a mut ConnectionMetadata<U>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Decision>>>;

    fn filter_data<'a>(
        &'a mut self,
        meta: &'a mut MailMetadata,
        conn_meta: &'a mut ConnectionMetadata<U>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Decision>>> {
        let _ = (meta, conn_meta); // Silence unused variable warning to keep nice names in the doc
        Box::pin(future::ready(Decision::Accept))
    }

    // Note: handle_mail *must* consume all of `stream` and call its `complete`
    // method to check that the data stream was properly closed and did not just
    // EOF too early. Things will panic otherwise.
    fn handle_mail<'a, S>(
        &'a mut self,
        stream: &'a mut DataStream<S>,
        meta: MailMetadata,
        conn_meta: &'a mut ConnectionMetadata<U>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        S: 'a + Unpin + Stream<Item = BytesMut>;

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
