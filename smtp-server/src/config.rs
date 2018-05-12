use bytes::BytesMut;
use smtp_message::{DataStream, Email, Prependable, ReplyCode, SmtpString};
use tokio::prelude::*;

use decision::Decision;
use metadata::{ConnectionMetadata, MailMetadata};

pub trait Config<U> {
    // TODO: (A) return futures to Decision here and in filter_{from,to}
    fn new_mail(&mut self) {}

    fn filter_from(&mut self, from: &Option<Email>, conn_meta: &ConnectionMetadata<U>) -> Decision;

    fn filter_to(
        &mut self,
        to: &Email,
        meta: &MailMetadata,
        conn_meta: &ConnectionMetadata<U>,
    ) -> Decision;

    fn filter_data(
        &mut self,
        _meta: &MailMetadata,
        _conn_meta: &ConnectionMetadata<U>,
    ) -> Decision {
        Decision::Accept
    }

    // TODO: (B) replace this Box by impl Trait syntax hide:impl-trait-in-trait
    fn handle_mail<'a, S>(
        &'a mut self,
        stream: DataStream<S>,
        meta: MailMetadata,
        conn_meta: &ConnectionMetadata<U>,
    ) -> Box<'a + Future<Item = (&'a mut Self, Option<Prependable<S>>, Decision), Error = ()>>
    where
        Self: 'a,
        S: 'a + Stream<Item = BytesMut, Error = ()>;

    fn hostname(&self) -> SmtpString;

    fn banner(&self) -> SmtpString {
        SmtpString::from_static(b"Service ready")
    }

    // TODO: (B) avoid concatenation here id:XIP2
    // Technique: Have Reply take mutliple strings
    fn welcome_banner(&self) -> (ReplyCode, SmtpString) {
        (
            ReplyCode::SERVICE_READY,
            self.hostname() + SmtpString::from_static(b" ") + self.banner(),
        )
    }

    // TODO: (B) return Reply when it is a thing (and same for below) id:E4tJ
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
