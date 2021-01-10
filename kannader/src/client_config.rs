use std::{io, pin::Pin};

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};
use tracing::error;

use smtp_message::Hostname;

use crate::WASM_CONFIG;

pub type DynAsyncReadWrite =
    duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>;

pub struct ClientConfig {
    connector: async_tls::TlsConnector,
}

impl ClientConfig {
    pub fn new(connector: async_tls::TlsConnector) -> ClientConfig {
        ClientConfig { connector }
    }
}

// TODO: share across *_config.rs files?
macro_rules! run_hook {
    ($fn:ident($($arg:expr),*) || $res:expr) => {
        WASM_CONFIG.with(|wasm_config| {
            match (wasm_config.client_config.$fn)($($arg),*) {
                Ok(res) => res,
                Err(e) => {
                    error!(error = ?e, "Internal server in ‘client_config_{}’", stringify!($fn));
                    $res
                }
            }
        })
    };
}

#[async_trait]
impl smtp_client::Config for ClientConfig {
    fn ehlo_hostname(&self) -> Hostname {
        run_hook!(ehlo_hostname() || panic!("Error while running the ‘ehlo_hostname’ hook"))
    }

    fn can_do_tls(&self) -> bool {
        run_hook!(can_do_tls() || true)
    }

    fn must_do_tls(&self) -> bool {
        run_hook!(must_do_tls() || false)
    }

    /// Note: If this function can only fail, make can_do_tls return false
    async fn tls_connect<IO>(&self, io: IO) -> io::Result<DynAsyncReadWrite>
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        use kannader_types::TlsHandler;
        let handler = run_hook!(tls_handler() || TlsHandler::Rustls);
        match handler {
            TlsHandler::Rustls => {
                // TODO: what should `nodomainyet` be here? for SNI maybe?
                let io = self.connector.connect("nodomainyet", io).await?;
                let (r, w) = io.split();
                let io = duplexify::Duplex::new(
                    Box::pin(r) as Pin<Box<dyn Send + AsyncRead>>,
                    Box::pin(w) as Pin<Box<dyn Send + AsyncWrite>>,
                );
                Ok(io)
            }
        }
    }

    fn banner_read_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(banner_read_timeout_in_millis() || 5 * 60 * 1000))
    }

    fn command_write_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(
            command_write_timeout_in_millis() || 5 * 60 * 1000
        ))
    }

    fn ehlo_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(ehlo_reply_timeout_in_millis() || 5 * 60 * 1000))
    }

    fn starttls_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(
            starttls_reply_timeout_in_millis() || 2 * 60 * 1000
        ))
    }

    fn mail_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(mail_reply_timeout_in_millis() || 5 * 60 * 1000))
    }

    fn rcpt_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(rcpt_reply_timeout_in_millis() || 5 * 60 * 1000))
    }

    fn data_init_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(
            data_init_reply_timeout_in_millis() || 2 * 60 * 1000
        ))
    }

    fn data_block_write_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(
            data_block_write_timeout_in_millis() || 3 * 60 * 1000
        ))
    }

    fn data_end_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(run_hook!(
            data_end_reply_timeout_in_millis() || 10 * 60 * 1000
        ))
    }
}
