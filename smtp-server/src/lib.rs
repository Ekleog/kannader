#![type_length_limit = "4194304"]

mod crlflines;
mod interact;
mod sendreply;

pub use interact::interact;

use std::{borrow::Cow, future::Future, pin::Pin};

use async_trait::async_trait;
use futures::io::AsyncRead;
use smtp_message::{Email, EnhancedReplyCode, EscapedDataReader, Reply, ReplyCode};

#[must_use]
pub enum Decision {
    Accept,
    Reject(Reply<Cow<'static, str>>),
}

pub struct MailMetadata<U> {
    pub user: U,
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

pub struct ConnectionMetadata<U> {
    pub user: U,
}

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

    // TODO: can this be an async fn?
    // see https://github.com/rust-lang/rust/issues/71058
    fn handle_mail<'a, R>(
        &'a self,
        stream: &'a mut EscapedDataReader<'a, R>,
        meta: MailMetadata<Self::MailUserMeta>,
        conn_meta: &'a mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        R: 'a + Send + Unpin + AsyncRead;

    fn hostname(&self) -> Cow<'static, str>;

    fn banner(&self) -> Cow<'static, str> {
        "Service ready".into()
    }

    fn welcome_banner(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::SERVICE_READY,
            ecode: None,
            text: vec![self.hostname() + " " + self.banner()],
        }
    }

    fn okay(&self, ecode: EnhancedReplyCode<Cow<'static, str>>) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::OKAY,
            ecode,
            text: vec!["Okay"],
        }
    }

    fn mail_okay(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
    }

    fn rcpt_okay(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_DEST_VALID)
    }

    fn data_okay(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::START_MAIL_INPUT,
            ecode: None,
            text: vec!["Start mail input; end with <CRLF>.<CRLF>"],
        }
    }

    fn mail_accepted(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
    }

    fn bad_sequence(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::BAD_SEQUENCE,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
            text: vec!["Bad sequence of commands"],
        }
    }

    fn already_in_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn rcpt_before_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn data_before_rcpt(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn data_before_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn command_unimplemented(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::COMMAND_UNIMPLEMENTED,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
            text: vec!["Command not implemented"],
        }
    }

    fn command_unrecognized(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::COMMAND_UNRECOGNIZED,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
            text: vec!["Command not recognized"],
        }
    }
}
