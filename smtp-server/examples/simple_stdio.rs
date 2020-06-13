#![type_length_limit = "200000000"]

use std::borrow::Cow;

use async_trait::async_trait;
use duplexify::Duplex;
use futures::{executor, io, AsyncRead, AsyncReadExt};

use smtp_message::{Email, EscapedDataReader, Hostname, Reply, ReplyCode};
use smtp_server::{interact, ConnectionMetadata, Decision, MailMetadata};

struct SimpleConfig;

#[async_trait]
impl smtp_server::Config for SimpleConfig {
    type ConnectionUserMeta = ();
    type MailUserMeta = ();

    fn hostname(&self) -> Cow<'static, str> {
        "simple.example.org".into()
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

    async fn filter_hello(
        &self,
        _is_ehlo: bool,
        _hostname: &mut Hostname<&str>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision {
        Decision::Accept
    }

    async fn filter_from(
        &self,
        from: &mut Option<Email<&str>>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision {
        if let Some(from) = from {
            let loc = *from.localpart.raw();
            if loc == "whitelisted" {
                Decision::Accept
            } else {
                Decision::Reject(Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["The sender is not the 'whitelisted' user".into()],
                })
            }
        } else {
            Decision::Accept
        }
    }

    async fn filter_to(
        &self,
        to: &mut Email<&str>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision {
        let loc = *to.localpart.raw();
        if loc != "forbidden" {
            Decision::Accept
        } else {
            Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["The 'forbidden' user is forbidden".into()],
            })
        }
    }

    async fn handle_mail<'a, R>(
        &self,
        reader: &mut EscapedDataReader<'a, R>,
        _mail: MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision
    where
        R: Send + Unpin + AsyncRead,
    {
        let mut text = Vec::new();
        if reader.read_to_end(&mut text).await.is_err() {
            return Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["io error".into()],
            });
        }
        reader.complete();
        if text.windows(5).find(|s| s == b"swearwords").is_some() {
            Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["No 'swearwords' here".into()],
            })
        } else {
            Decision::Accept
        }
    }
}

fn main() -> io::Result<()> {
    let reader = io::AllowStdIo::new(std::io::stdin());
    let writer = io::AllowStdIo::new(std::io::stdout());
    let io = Duplex::new(reader, writer);
    executor::block_on(interact(io, (), &mut SimpleConfig))
}
