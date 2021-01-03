use async_trait::async_trait;
use futures::AsyncRead;
use tracing::{info, warn};

use smtp_message::Hostname;

use crate::{ClientConfig, Meta};

fn transport_error_client_to_queue(
    err: smtp_client::TransportError,
    text: &'static str,
) -> smtp_queue::TransportFailure {
    let severity = err.severity();
    warn!(error = ?anyhow::Error::new(err), "{}", text);
    match severity {
        smtp_client::TransportErrorSeverity::Local => smtp_queue::TransportFailure::Local,
        smtp_client::TransportErrorSeverity::NetworkTransient => {
            smtp_queue::TransportFailure::NetworkTransient
        }
        smtp_client::TransportErrorSeverity::MailTransient => {
            smtp_queue::TransportFailure::MailTransient
        }
        smtp_client::TransportErrorSeverity::MailboxTransient => {
            smtp_queue::TransportFailure::MailboxTransient
        }
        smtp_client::TransportErrorSeverity::MailSystemTransient => {
            smtp_queue::TransportFailure::MailSystemTransient
        }
        smtp_client::TransportErrorSeverity::MailPermanent => {
            smtp_queue::TransportFailure::MailPermanent
        }
        smtp_client::TransportErrorSeverity::MailboxPermanent => {
            smtp_queue::TransportFailure::MailboxPermanent
        }
        smtp_client::TransportErrorSeverity::MailSystemPermanent => {
            smtp_queue::TransportFailure::MailSystemPermanent
        }
    }
}

pub struct QueueTransport<C, P>(smtp_client::Client<C, P, ClientConfig>)
where
    C: trust_dns_resolver::proto::DnsHandle<Error = trust_dns_resolver::error::ResolveError>,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>;

impl<C, P> QueueTransport<C, P>
where
    C: trust_dns_resolver::proto::DnsHandle<Error = trust_dns_resolver::error::ResolveError>,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
{
    pub fn new(client: smtp_client::Client<C, P, ClientConfig>) -> QueueTransport<C, P> {
        QueueTransport(client)
    }
}

#[async_trait]
impl<C, P> smtp_queue::Transport<Meta> for QueueTransport<C, P>
where
    C: trust_dns_resolver::proto::DnsHandle<Error = trust_dns_resolver::error::ResolveError>,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
{
    type Destination = smtp_client::Destination;
    type Sender = QueueTransportSender;

    async fn destination(
        &self,
        meta: &smtp_queue::MailMetadata<Meta>,
    ) -> Result<Self::Destination, smtp_queue::TransportFailure> {
        // TODO: this should most likely be a const or similar; and definitely not
        // recomputed on each call to destination
        let localhost = Hostname::parse(b"localhost")
            .expect("failed to parse constant hostname")
            .1
            .to_owned();
        self.0
            .get_destination(meta.to.hostname.as_ref().unwrap_or(&localhost))
            .await
            .map_err(|e| {
                transport_error_client_to_queue(
                    e,
                    "Transport error while trying to get destination",
                )
            })
    }

    async fn connect(
        &self,
        dest: &Self::Destination,
    ) -> Result<Self::Sender, smtp_queue::TransportFailure> {
        info!(destination = %dest, "Connecting to remote server");
        // TODO: log the IP to which we're connecting
        self.0
            .connect(dest)
            .await
            .map(QueueTransportSender)
            .map_err(|e| {
                transport_error_client_to_queue(
                    e,
                    "Transport error while trying to connect to destination",
                )
            })
    }
}

pub struct QueueTransportSender(smtp_client::Sender<ClientConfig>);

#[async_trait]
impl smtp_queue::TransportSender<Meta> for QueueTransportSender {
    async fn send<Reader>(
        &mut self,
        meta: &smtp_queue::MailMetadata<Meta>,
        mail: Reader,
    ) -> Result<(), smtp_queue::TransportFailure>
    where
        Reader: Send + AsyncRead,
    {
        // TODO: pass through mail id so that it's possible to log it
        self.0
            .send(meta.from.as_ref(), &meta.to, mail)
            .await
            .map_err(|e| {
                transport_error_client_to_queue(e, "Transport error while trying to send email")
            })
    }
}
