use std::{io, pin::Pin};

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite};

use smtp_message::Hostname;

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

#[async_trait]
impl smtp_client::Config for ClientConfig {
    fn ehlo_hostname(&self) -> Hostname<&str> {
        // TODO: this is ugly
        Hostname::parse(b"localhost")
            .expect("failed parsing static str")
            .1
    }

    async fn tls_connect<IO>(&self, io: IO) -> io::Result<DynAsyncReadWrite>
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        let io = self.connector.connect("nodomainyet", io).await?;
        let (r, w) = io.split();
        let io = duplexify::Duplex::new(
            Box::pin(r) as Pin<Box<dyn Send + AsyncRead>>,
            Box::pin(w) as Pin<Box<dyn Send + AsyncWrite>>,
        );
        Ok(io)
    }
}
