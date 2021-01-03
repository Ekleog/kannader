use std::{io, pin::Pin};

use async_trait::async_trait;
use chrono::Utc;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::error;

use smtp_message::Email;
use smtp_queue_fs::FsStorage;
use smtp_server::{reply, Decision};

use crate::{Meta, QueueConfig, DATABUF_SIZE, WASM_CONFIG};

pub struct ServerConfig<T> {
    acceptor: async_tls::TlsAcceptor,
    queue: smtp_queue::Queue<Meta, QueueConfig, FsStorage<Meta>, T>,
}

impl<T> ServerConfig<T>
where
    T: smtp_queue::Transport<Meta>,
{
    pub fn new(
        acceptor: async_tls::TlsAcceptor,
        queue: smtp_queue::Queue<Meta, QueueConfig, FsStorage<Meta>, T>,
    ) -> ServerConfig<T> {
        ServerConfig { acceptor, queue }
    }
}

#[async_trait]
impl<T> smtp_server::Config for ServerConfig<T>
where
    T: smtp_queue::Transport<Meta>,
{
    type ConnectionUserMeta = Vec<u8>;
    type MailUserMeta = Vec<u8>;

    fn hostname(&self, _conn_meta: &smtp_server::ConnectionMetadata<Vec<u8>>) -> &str {
        "localhost"
    }

    // TODO: this could have a default implementation if we were able to have a
    // default type of () for MailUserMeta without requiring unstable
    async fn new_mail(
        &self,
        _conn_meta: &mut smtp_server::ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Self::MailUserMeta {
        Vec::new() // TODO
    }

    // TODO: when GATs are here, we can remove the trait object and return
    // Self::TlsStream<IO> (or maybe we should refactor Config to be Config<IO>? but
    // that's ugly). At that time we can probably get rid of all that duplexify
    // mess... or maybe when we can do trait objects with more than one trait
    /// Note: if you don't want to implement TLS, you should override
    /// `can_do_tls` to return `false` so that STARTTLS is not advertized. This
    /// being said, returning an error here should have the same result in
    /// practice, except clients will try STARTTLS and fail
    async fn tls_accept<IO>(
        &self,
        io: IO,
        _conn_meta: &mut smtp_server::ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> io::Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
    >
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        let io = self.acceptor.accept(io).await?;
        let (r, w) = io.split();
        let io = duplexify::Duplex::new(
            Box::pin(r) as Pin<Box<dyn Send + AsyncRead>>,
            Box::pin(w) as Pin<Box<dyn Send + AsyncWrite>>,
        );
        Ok(io)
    }

    async fn filter_from(
        &self,
        from: Option<Email>,
        meta: &mut smtp_server::MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut smtp_server::ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<Option<Email>> {
        // TODO: have this communication schema for all hooks
        WASM_CONFIG.with(|wasm_config| {
            let res = (wasm_config.server_config.filter_from)(from, meta, conn_meta);
            match res {
                Ok(res) => res.into(),
                Err(e) => {
                    error!(error = ?e, "Internal server error in ‘filter_from’");
                    Decision::Reject {
                        reply: reply::internal_server_error().convert(),
                    }
                }
            }
        })
    }

    async fn filter_to(
        &self,
        to: Email,
        _meta: &mut smtp_server::MailMetadata<Self::MailUserMeta>,
        _conn_meta: &mut smtp_server::ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<Email> {
        // TODO: this is BAD
        Decision::Accept {
            reply: reply::okay_to().convert(),
            res: to,
        }
    }

    /// Note: the EscapedDataReader has an inner buffer size of
    /// [`RDBUF_SIZE`](RDBUF_SIZE), which means that reads should not happen
    /// with more than this buffer size.
    ///
    /// Also, note that there is no timeout applied here, so the implementation
    /// of this function is responsible for making sure that the client does not
    /// just stop sending anything to DOS the system.
    async fn handle_mail<'a, R>(
        &self,
        stream: &mut smtp_message::EscapedDataReader<'a, R>,
        meta: smtp_server::MailMetadata<Self::MailUserMeta>,
        _conn_meta: &mut smtp_server::ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()>
    where
        R: Send + Unpin + AsyncRead,
    {
        let mut enqueuer = match self.queue.enqueue().await {
            Ok(enqueuer) => enqueuer,
            Err(e) => {
                error!(error = ?anyhow::Error::new(e), "Internal server error while opening an enqueuer");
                return Decision::Reject {
                    reply: reply::internal_server_error().convert(),
                };
            }
        };
        // TODO: MUST add Received header at least
        // TODO: factor out with the similar logic in smtp-client
        let mut buf = [0; DATABUF_SIZE];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => {
                    // End of stream
                    break;
                }
                Ok(n) => {
                    // Got n bytes
                    if let Err(e) = enqueuer.write_all(&buf[..n]).await {
                        error!(error = ?e, "Internal server error while writing data to queue");
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(_) => (),
                                Err(e) => {
                                    error!(error = ?e, "Internal server error while reading data from network");
                                    break;
                                }
                            }
                        }
                        return Decision::Reject {
                            reply: reply::internal_server_error().convert(),
                        };
                    }
                }
                Err(e) => {
                    error!(error = ?e, "Internal server error while reading data from network");
                    return Decision::Reject {
                        reply: reply::internal_server_error().convert(),
                    };
                }
            }
        }

        if !stream.is_finished() {
            // Stream isn't finished, as we read until end-of-stream it means that there was
            // an error somewhere
            error!("Stream stopped returning any bytes without actually finishing");
            Decision::Reject {
                reply: reply::internal_server_error().convert(),
            }
        } else {
            // Stream is finished, let's complete it then commit the file to the queue and
            // acept
            stream.complete();
            let from = &meta.from;
            let destinations = meta
                .to
                .into_iter()
                .map(move |to| {
                    (
                        smtp_queue::MailMetadata {
                            from: from.clone(),
                            to,
                            metadata: Meta,
                        },
                        smtp_queue::ScheduleInfo {
                            at: Utc::now(),
                            last_attempt: None,
                        },
                    )
                })
                .collect();
            if let Err(e) = enqueuer.commit(destinations).await {
                error!(error = ?e, "Internal server error while committing mail");
                Decision::Reject {
                    reply: reply::internal_server_error().convert(),
                }
            } else {
                Decision::Accept {
                    reply: reply::okay_mail().convert(),
                    res: (),
                }
            }
        }
    }
}
