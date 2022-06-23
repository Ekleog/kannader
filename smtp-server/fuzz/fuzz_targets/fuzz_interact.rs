#![no_main]
#![type_length_limit = "200000000"]

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use duplexify::Duplex;
use futures::{
    executor,
    io::{self, Cursor},
    AsyncRead, AsyncReadExt, AsyncWrite,
};
use futures_test::io::AsyncReadTestExt;
use libfuzzer_sys::fuzz_target;

use smtp_message::{Email, EscapedDataReader, Reply, ReplyCode};
use smtp_server::{interact, reply, ConnectionMetadata, Decision, IsAlreadyTls, MailMetadata};

struct FuzzConfig;

#[async_trait]
impl smtp_server::Config for FuzzConfig {
    type ConnectionUserMeta = ();
    type MailUserMeta = ();
    type Protocol = smtp_server::protocol::Smtp;

    fn hostname(&self, _conn_meta: &ConnectionMetadata<()>) -> &str {
        "test.example.org"
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
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tls not implemented for fuzzing",
        ))
    }

    async fn filter_from(
        &self,
        from: Option<Email>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Option<Email>> {
        if let Some(from) = from {
            let loc = from.localpart.raw().as_bytes();
            if loc.len() >= 2 && loc[0] > loc[1] {
                Decision::Accept {
                    reply: reply::okay_from().convert(),
                    res: Some(from),
                }
            } else {
                Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::POLICY_REASON,
                        ecode: None,
                        text: vec!["forbidden user".into()],
                    },
                }
            }
        } else {
            Decision::Accept {
                reply: reply::okay_from().convert(),
                res: from,
            }
        }
    }

    async fn filter_to(
        &self,
        to: Email,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Email> {
        let loc = to.localpart.raw().as_bytes();
        if loc.len() >= 2 && loc[0] > loc[1] {
            Decision::Accept {
                reply: reply::okay_to().convert(),
                res: to,
            }
        } else {
            Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["forbidden user".into()],
                },
            }
        }
    }

    #[allow(clippy::needless_lifetimes)] // false-positive
    async fn handle_mail<'contents, 'cfg, 'connmeta, 'resp, R>(
        &'cfg self,
        reader: &mut EscapedDataReader<'contents, R>,
        mail: MailMetadata<()>,
        _conn_meta: &'connmeta mut ConnectionMetadata<()>,
    ) -> Decision<()>
    where
        R: Send + Unpin + AsyncRead,
    {
        let mut ignore = Vec::new();
        if reader.read_to_end(&mut ignore).await.is_err() {
            return Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["io error".into()],
                },
            };
        }
        reader.complete();
        if mail.to.len() > 3 {
            // This is stupid, please use filter_to instead if you're not just willing
            // to fuzz
            Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["too many recipients".into()],
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

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let chunk_size = data[0] as u16 * 256 + data[1] as u16;

    // And send stuff in
    let reader = Cursor::new(data[2..].to_owned()).limited(chunk_size as usize);
    let writer = io::sink();
    let io = Duplex::new(reader, writer);
    let _ignore_errors =
        executor::block_on(interact(io, IsAlreadyTls::No, (), Arc::new(FuzzConfig)));
});
