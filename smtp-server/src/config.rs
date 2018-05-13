use bytes::BytesMut;
use smtp_message::{DataStream, Email, Prependable, ReplyCode, SmtpString};
use tokio::prelude::*;

use decision::Decision;
use metadata::{ConnectionMetadata, MailMetadata};

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
// TODO: (B) for a clean api, the futures should not take ownership and return
// but rather take a reference (when async/await will be done)
pub trait Config<U> {
    fn new_mail<'a>(&'a mut self) -> Box<'a + Future<Item = &'a mut Self, Error = ()>>
    where
        U: 'a,
    {
        Box::new(future::ok(self))
    }

    fn filter_from<'a>(
        &'a mut self,
        from: Option<Email>,
        conn_meta: ConnectionMetadata<U>,
    ) -> Box<
        'a
            + Future<Item = (&'a mut Self, Option<Email>, ConnectionMetadata<U>, Decision), Error = ()>,
    >
    where
        U: 'a;

    fn filter_to<'a>(
        &'a mut self,
        to: Email,
        meta: MailMetadata,
        conn_meta: ConnectionMetadata<U>,
    ) -> Box<
        'a
            + Future<
                Item = (
                    &'a mut Self,
                    Email,
                    MailMetadata,
                    ConnectionMetadata<U>,
                    Decision,
                ),
                Error = (),
            >,
    >
    where
        U: 'a;

    fn filter_data<'a>(
        &'a mut self,
        meta: MailMetadata,
        conn_meta: ConnectionMetadata<U>,
    ) -> Box<
        'a
            + Future<Item = (&'a mut Self, MailMetadata, ConnectionMetadata<U>, Decision), Error = ()>,
    >
    where
        U: 'a,
    {
        Box::new(future::ok((self, meta, conn_meta, Decision::Accept)))
    }

    fn handle_mail<'a, S: 'a>(
        &'a mut self,
        stream: DataStream<S>,
        meta: MailMetadata,
        conn_meta: ConnectionMetadata<U>,
    ) -> Box<
        'a
            + Future<
                Item = (
                    &'a mut Self,
                    Option<Prependable<S>>,
                    ConnectionMetadata<U>,
                    Decision,
                ),
                Error = (),
            >,
    >
    where
        Self: 'a,
        S: 'a + Stream<Item = BytesMut, Error = ()>,
        U: 'a;

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
