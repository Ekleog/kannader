#![no_main]
#![type_length_limit = "200000000"]

use std::{borrow::Cow, pin::Pin};

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
use smtp_server::{interact, ConnectionMetadata, Decision, IsAlreadyTls, MailMetadata};

struct FuzzConfig;

#[async_trait]
impl smtp_server::Config for FuzzConfig {
    type ConnectionUserMeta = ();
    type MailUserMeta = ();

    fn hostname(&self) -> Cow<'static, str> {
        "test.example.org".into()
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

    async fn tls_accept<IO>(
        &self,
        io: IO,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
        (IO, io::Error),
    >
    where
        IO: Send + AsyncRead + AsyncWrite,
    {
        Err((
            io,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "tls not implemented for fuzzing",
            ),
        ))
    }

    async fn filter_from(
        &self,
        from: &mut Option<Email<&str>>,
        _meta: &mut MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision {
        if let Some(from) = from {
            let loc = from.localpart.raw().as_bytes();
            if loc.len() >= 2 && loc[0] > loc[1] {
                Decision::Accept
            } else {
                Decision::Reject(Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["forbidden user".into()],
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
        let loc = to.localpart.raw().as_bytes();
        if loc.len() >= 2 && loc[0] > loc[1] {
            Decision::Accept
        } else {
            Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["forbidden user".into()],
            })
        }
    }

    async fn handle_mail<'a, R>(
        &self,
        reader: &mut EscapedDataReader<'a, R>,
        mail: MailMetadata<()>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision
    where
        R: Send + Unpin + AsyncRead,
    {
        let mut ignore = Vec::new();
        if reader.read_to_end(&mut ignore).await.is_err() {
            return Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["io error".into()],
            });
        }
        reader.complete();
        if mail.to.len() > 3 {
            // This is stupid, please use filter_to instead if you're not just willing
            // to fuzz
            Decision::Reject(Reply {
                code: ReplyCode::POLICY_REASON,
                ecode: None,
                text: vec!["too many recipients".into()],
            })
        } else {
            Decision::Accept
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let chunk_size = data[0] as u16 * 256 + data[1] as u16;

    // And send stuff in
    let reader = Cursor::new(&data[2..]).limited(chunk_size as usize);
    let writer = io::sink();
    let io = Duplex::new(reader, writer);
    let _ignore_errors = executor::block_on(interact(io, IsAlreadyTls::No, (), &mut FuzzConfig));
});
