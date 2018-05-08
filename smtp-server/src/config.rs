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

    // TODO(low): When Rust allows it, replace this Box by impl Trait syntax
    fn handle_mail<'a, S>(
        &'a mut self,
        stream: DataStream<S>,
        meta: MailMetadata,
        conn_meta: &ConnectionMetadata<U>,
    ) -> Box<'a + Future<Item = (&'a mut Self, Option<Prependable<S>>, Decision), Error = ()>>
    where
        Self: 'a,
        S: 'a + Stream<Item = BytesMut, Error = ()>;

    // TODO(low): return Reply when it is a thing (and same for everywhere below)
    fn okay(&self) -> (ReplyCode, SmtpString) {
        (ReplyCode::OKAY, SmtpString::from_static(b"Okay"))
    }

    fn mail_okay(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn rcpt_okay(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn mail_accepted(&self) -> (ReplyCode, SmtpString) {
        self.okay()
    }

    fn bad_sequence(&self) -> (ReplyCode, SmtpString) {
        (ReplyCode::BAD_SEQUENCE, SmtpString::from_static(b"Bad sequence of commands"))
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
        (ReplyCode::COMMAND_UNIMPLEMENTED, SmtpString::from_static(b"Command not implemented"))
    }

    fn command_unrecognized(&self) -> (ReplyCode, SmtpString) {
        (ReplyCode::COMMAND_UNRECOGNIZED, SmtpString::from_static(b"Command not recognized"))
    }
}
