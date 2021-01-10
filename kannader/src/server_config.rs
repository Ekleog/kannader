use std::{io, pin::Pin};

use async_trait::async_trait;
use chrono::Utc;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::error;

use smtp_message::{Email, Hostname, MaybeUtf8, Reply};
use smtp_queue_fs::FsStorage;
use smtp_server::{reply, Decision, HelloInfo};

use crate::{Meta, QueueConfig, DATABUF_SIZE, WASM_CONFIG};

pub type ConnMeta = smtp_server::ConnectionMetadata<Vec<u8>>;
pub type MailMeta = smtp_server::MailMetadata<Vec<u8>>;

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

macro_rules! run_hook {
    ($fn:ident($($arg:expr),*)) => {
        run_hook!($fn($($arg),*) ||
            Decision::Reject {
                reply: reply::internal_server_error().convert(),
            }
        )
    };

    ($fn:ident($($arg:expr),*) || $res:expr) => {
        WASM_CONFIG.with(|wasm_config| {
            let res = (wasm_config.server_config.$fn)($($arg),*);
            match res {
                Ok(res) => res.into(),
                Err(e) => {
                    error!(error = ?e, "Internal server error in ‘server_config_{}’", stringify!($fn));
                    $res
                }
            }
        })
    };
}

#[async_trait]
impl<T> smtp_server::Config for ServerConfig<T>
where
    T: smtp_queue::Transport<Meta>,
{
    type ConnectionUserMeta = Vec<u8>;
    type MailUserMeta = Vec<u8>;

    fn hostname(&self, _: &ConnMeta) -> &str {
        unimplemented!()
    }

    fn welcome_banner(&self, _: &ConnMeta) -> &str {
        unimplemented!()
    }

    fn welcome_banner_reply(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(welcome_banner_reply(conn_meta) || reply::internal_server_error().convert())
    }

    fn hello_banner(&self, _: &ConnMeta) -> &str {
        unimplemented!()
    }

    async fn filter_hello(
        &self,
        is_ehlo: bool,
        hostname: Hostname,
        conn_meta: &mut ConnMeta,
    ) -> Decision<HelloInfo> {
        run_hook!(filter_hello(is_ehlo, hostname, conn_meta))
    }

    fn can_do_tls(&self, conn_meta: &ConnMeta) -> bool {
        // Unfortunately, there is no good way to gracefully fail here
        run_hook!(
            // TODO: rust should auto-deref here, report a rust bug
            can_do_tls((*conn_meta).clone()) || panic!("Error while running the ‘can_do_tls’ hook")
        )
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
        _conn_meta: &mut ConnMeta,
    ) -> io::Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
    >
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
    {
        // TODO: figure out a way to cleanly configure this... maybe having the wasm
        // blob return one of “rustls” and “native-tls” and then picking the correct
        // implementation? and then also make the rustls parameters in main.rs
        // configurable... anyway we have to think about having multiple TLS certs for
        // multiple SNI hostnames / multiple IP addresses
        let io = self.acceptor.accept(io).await?;
        let (r, w) = io.split();
        let io = duplexify::Duplex::new(
            Box::pin(r) as Pin<Box<dyn Send + AsyncRead>>,
            Box::pin(w) as Pin<Box<dyn Send + AsyncWrite>>,
        );
        Ok(io)
    }

    async fn new_mail(&self, conn_meta: &mut ConnMeta) -> Vec<u8> {
        // Unfortunately, there is no good way to gracefully fail here
        run_hook!(new_mail(conn_meta) || panic!("Error while running the ‘new_mail’ hook"))
    }

    async fn filter_from(
        &self,
        from: Option<Email>,
        meta: &mut MailMeta,
        conn_meta: &mut ConnMeta,
    ) -> Decision<Option<Email>> {
        run_hook!(filter_from(from, meta, conn_meta))
    }

    async fn filter_to(
        &self,
        to: Email,
        meta: &mut MailMeta,
        conn_meta: &mut ConnMeta,
    ) -> Decision<Email> {
        run_hook!(filter_to(to, meta, conn_meta))
    }

    async fn filter_data(&self, meta: &mut MailMeta, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(filter_data(meta, conn_meta))
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
        meta: MailMeta,
        _conn_meta: &mut ConnMeta,
    ) -> Decision<()>
    where
        R: Send + Unpin + AsyncRead,
    {
        // TODO: figure out how to make this properly configurable, allowing to
        // configure filters, etc.
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

    async fn handle_rset(
        &self,
        meta: &mut Option<MailMeta>,
        conn_meta: &mut ConnMeta,
    ) -> Decision<()> {
        run_hook!(handle_rset(meta, conn_meta))
    }

    async fn handle_starttls(&self, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(handle_starttls(conn_meta))
    }

    async fn handle_expn(&self, name: MaybeUtf8<&str>, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(handle_expn(name.convert(), conn_meta))
    }

    async fn handle_vrfy(&self, name: MaybeUtf8<&str>, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(handle_vrfy(name.convert(), conn_meta))
    }

    async fn handle_help(
        &self,
        subject: MaybeUtf8<&str>,
        conn_meta: &mut ConnMeta,
    ) -> Decision<()> {
        run_hook!(handle_help(subject.convert(), conn_meta))
    }

    async fn handle_noop(&self, string: MaybeUtf8<&str>, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(handle_noop(string.convert(), conn_meta))
    }

    async fn handle_quit(&self, conn_meta: &mut ConnMeta) -> Decision<()> {
        run_hook!(handle_quit(conn_meta))
    }

    fn already_did_hello(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(already_did_hello(conn_meta) || reply::bad_sequence().convert())
    }

    fn mail_before_hello(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(mail_before_hello(conn_meta) || reply::bad_sequence().convert())
    }

    fn already_in_mail(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(already_in_mail(conn_meta) || reply::bad_sequence().convert())
    }

    fn rcpt_before_mail(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(rcpt_before_mail(conn_meta) || reply::bad_sequence().convert())
    }

    fn data_before_rcpt(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(data_before_rcpt(conn_meta) || reply::bad_sequence().convert())
    }

    fn data_before_mail(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(data_before_mail(conn_meta) || reply::bad_sequence().convert())
    }

    fn starttls_unsupported(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(starttls_unsupported(conn_meta) || reply::command_not_supported().convert())
    }

    fn command_unrecognized(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(command_unrecognized(conn_meta) || reply::command_unrecognized().convert())
    }

    fn pipeline_forbidden_after_starttls(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(
            pipeline_forbidden_after_starttls(conn_meta)
                || reply::pipeline_forbidden_after_starttls().convert()
        )
    }

    fn line_too_long(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(line_too_long(conn_meta) || reply::line_too_long().convert())
    }

    fn handle_mail_did_not_call_complete(&self, conn_meta: &mut ConnMeta) -> Reply {
        run_hook!(
            handle_mail_did_not_call_complete(conn_meta)
                || reply::handle_mail_did_not_call_complete().convert()
        )
    }

    fn reply_write_timeout(&self) -> chrono::Duration {
        // Unfortunately, there is no good way to gracefully fail here
        chrono::Duration::milliseconds(run_hook!(
            reply_write_timeout_in_millis()
                || panic!("Error while running the ‘reply_write_timeout’ hook")
        ))
    }

    fn command_read_timeout(&self) -> chrono::Duration {
        // Unfortunately, there is no good way to gracefully fail here
        chrono::Duration::milliseconds(run_hook!(
            command_read_timeout_in_millis()
                || panic!("Error while running the ‘command_read_timeout’ hook")
        ))
    }
}
