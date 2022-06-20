#![type_length_limit = "200000000"]

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use duplexify::Duplex;
use futures::{executor, io, AsyncRead, AsyncReadExt, AsyncWrite};

use smtp_message::{Email, EscapedDataReader, Reply, ReplyCode};
use smtp_server::{interact, reply, ConnectionMetadata, Decision, IsAlreadyTls, MailMetadata};

struct SimpleConfig;

#[async_trait]
impl smtp_server::Config for SimpleConfig {
    type ConnectionUserMeta = ();
    type MailUserMeta = ();
    type Protocol = smtp_server::protocol::Smtp;

    fn hostname(&self, _conn_meta: &ConnectionMetadata<()>) -> &str {
        "simple.example.org"
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

    async fn tls_accept<IO>(
        &self,
        _io: IO,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> io::Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
    >
    where
        IO: Send + AsyncRead + AsyncWrite,
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tls not implemented for example",
        ))
    }

    async fn filter_from(
        &self,
        from: Option<Email>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Option<Email>> {
        if let Some(from) = from {
            let loc = from.localpart.raw();
            if loc == "whitelisted" {
                return Decision::Accept {
                    reply: reply::okay_from().convert(),
                    res: Some(from),
                };
            }
        }
        Decision::Reject {
            reply: Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["The sender is not the 'whitelisted' user".into()],
            },
        }
    }

    async fn filter_to(
        &self,
        to: Email,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Email> {
        let loc = to.localpart.raw();
        if loc != "forbidden" {
            Decision::Accept {
                reply: reply::okay_to().convert(),
                res: to,
            }
        } else {
            Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["The 'forbidden' user is forbidden".into()],
                },
            }
        }
    }

    async fn handle_mail<'contents, 'cfg, 'connmeta, 'resp, R>(
        &'cfg self,
        reader: &mut EscapedDataReader<'contents, R>,
        _meta: MailMetadata<Self::MailUserMeta>,
        _conn_meta: &'connmeta mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> <Self::Protocol as smtp_server::Protocol<'resp>>::HandleMailReturnType
    where
        R: Send + Unpin + AsyncRead,
        'cfg: 'resp,
        'connmeta: 'resp,
        Self: 'resp,
    {
        let mut text = Vec::new();
        if reader.read_to_end(&mut text).await.is_err() {
            return Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["io error".into()],
                },
            };
        }
        reader.complete();
        if text.windows(5).find(|s| s == b"swearwords").is_some() {
            Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["No 'swearwords' here".into()],
                },
            }
        } else {
            Decision::Accept {
                reply: reply::okay_mail().convert(),
                res: (),
            }
        }
    }
}

fn main() -> io::Result<()> {
    let reader = io::AllowStdIo::new(std::io::stdin());
    let writer = io::AllowStdIo::new(std::io::stdout());
    let io = Duplex::new(reader, writer);
    executor::block_on(interact(io, IsAlreadyTls::No, (), Arc::new(SimpleConfig)))
}
