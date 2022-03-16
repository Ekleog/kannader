#![cfg_attr(test, feature(negative_impls))]
#![type_length_limit = "200000000"]

use std::{cmp, io, ops::Range, pin::Pin, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use smol::future::FutureExt;
use smtp_message::{
    next_crlf, nom, Command, Email, EscapedDataReader, Hostname, MaybeUtf8, NextCrLfState, Reply,
};

pub use smtp_server_types::{reply, ConnectionMetadata, Decision, HelloInfo, MailMetadata};

pub const RDBUF_SIZE: usize = 16 * 1024;
const MINIMUM_FREE_BUFSPACE: usize = 128;

#[async_trait]
pub trait Config: Send + Sync {
    type ConnectionUserMeta: Send;
    type MailUserMeta: Send;

    /// Note: this function is only ever used for the default implementations of
    /// other functions in this trait. As such, it is OK to leave it
    /// `unimplemented!()` if other functions are implemented.
    fn hostname(&self, conn_meta: &ConnectionMetadata<Self::ConnectionUserMeta>) -> &str;

    /// Note: this function is only ever used for the default implementations of
    /// other functions in this trait. As such, it is OK to leave it
    /// `unimplemented!()` if other functions are implemented.
    #[allow(unused_variables)]
    fn welcome_banner(&self, conn_meta: &ConnectionMetadata<Self::ConnectionUserMeta>) -> &str {
        "Service ready"
    }

    fn welcome_banner_reply(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::welcome_banner(self.hostname(conn_meta), self.welcome_banner(conn_meta))
    }

    /// Note: this function is only ever used for the default implementations of
    /// other functions in this trait. As such, it is OK to leave it
    /// `unimplemented!()` if other functions are implemented.
    #[allow(unused_variables)]
    fn hello_banner(&self, conn_meta: &ConnectionMetadata<Self::ConnectionUserMeta>) -> &str {
        ""
    }

    #[allow(unused_variables)]
    async fn filter_hello(
        &self,
        is_ehlo: bool,
        hostname: Hostname,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<HelloInfo> {
        // Set `conn_meta.hello` early so that can_do_tls can use it below
        conn_meta.hello = Some(HelloInfo {
            is_ehlo,
            hostname: hostname.clone(),
        });
        Decision::Accept {
            reply: reply::okay_hello(
                is_ehlo,
                self.hostname(conn_meta),
                self.hello_banner(conn_meta),
                self.can_do_tls(conn_meta),
            )
            .convert(),
            res: HelloInfo { is_ehlo, hostname },
        }
    }

    #[allow(unused_variables)]
    fn can_do_tls(&self, conn_meta: &ConnectionMetadata<Self::ConnectionUserMeta>) -> bool {
        !conn_meta.is_encrypted && conn_meta.hello.as_ref().map(|h| h.is_ehlo).unwrap_or(false)
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
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> io::Result<
        duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
    >
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite;

    async fn new_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Self::MailUserMeta;

    async fn filter_from(
        &self,
        from: Option<Email>,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<Option<Email>>;

    async fn filter_to(
        &self,
        to: Email,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<Email>;

    #[allow(unused_variables)]
    async fn filter_data(
        &self,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Accept {
            reply: reply::okay_data().convert(),
            res: (),
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
        stream: &mut EscapedDataReader<'a, R>,
        meta: MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()>
    where
        R: Send + Unpin + AsyncRead;

    #[allow(unused_variables)]
    async fn handle_rset(
        &self,
        meta: &mut Option<MailMetadata<Self::MailUserMeta>>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Accept {
            reply: reply::okay_rset().convert(),
            res: (),
        }
    }

    #[allow(unused_variables)]
    async fn handle_starttls(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        if self.can_do_tls(conn_meta) {
            Decision::Accept {
                reply: reply::okay_starttls().convert(),
                res: (),
            }
        } else {
            Decision::Reject {
                reply: reply::command_not_supported().convert(),
            }
        }
    }

    #[allow(unused_variables)]
    async fn handle_expn(
        &self,
        name: MaybeUtf8<&str>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Reject {
            reply: reply::command_unimplemented().convert(),
        }
    }

    #[allow(unused_variables)]
    async fn handle_vrfy(
        &self,
        name: MaybeUtf8<&str>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Accept {
            reply: reply::ignore_vrfy().convert(),
            res: (),
        }
    }

    #[allow(unused_variables)]
    async fn handle_help(
        &self,
        subject: MaybeUtf8<&str>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Accept {
            reply: reply::ignore_help().convert(),
            res: (),
        }
    }

    #[allow(unused_variables)]
    async fn handle_noop(
        &self,
        string: MaybeUtf8<&str>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Accept {
            reply: reply::okay_noop().convert(),
            res: (),
        }
    }

    #[allow(unused_variables)]
    async fn handle_quit(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision<()> {
        Decision::Kill {
            reply: Some(reply::okay_quit().convert()),
            res: Ok(()),
        }
    }

    #[allow(unused_variables)]
    fn already_did_hello(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn mail_before_hello(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn already_in_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn rcpt_before_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn data_before_rcpt(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn data_before_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::bad_sequence().convert()
    }

    #[allow(unused_variables)]
    fn starttls_unsupported(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::command_not_supported().convert()
    }

    #[allow(unused_variables)]
    fn command_unrecognized(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::command_unrecognized().convert()
    }

    #[allow(unused_variables)]
    fn pipeline_forbidden_after_starttls(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::pipeline_forbidden_after_starttls().convert()
    }

    #[allow(unused_variables)]
    fn line_too_long(&self, conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>) -> Reply {
        reply::line_too_long().convert()
    }

    #[allow(unused_variables)]
    fn handle_mail_did_not_call_complete(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Reply {
        reply::handle_mail_did_not_call_complete().convert()
    }

    fn reply_write_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn command_read_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }
}

async fn advance_until_crlf<R>(
    r: &mut R,
    buf: &mut [u8],
    unhandled: &mut Range<usize>,
) -> io::Result<()>
where
    R: Unpin + AsyncRead,
{
    let mut state = NextCrLfState::Start;
    loop {
        if let Some(p) = next_crlf(&buf[unhandled.clone()], &mut state) {
            unhandled.start += p + 1;
            return Ok(());
        } else {
            let read = r.read(buf).await?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "connection shutdown while waiting for crlf after invalid command",
                ));
            }
            *unhandled = 0..read;
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum IsAlreadyTls {
    Yes,
    No,
}

pub async fn interact<IO, Cfg>(
    io: IO,
    is_already_tls: IsAlreadyTls,
    metadata: Cfg::ConnectionUserMeta,
    cfg: Arc<Cfg>,
) -> io::Result<()>
where
    IO: 'static + Send + AsyncRead + AsyncWrite,
    Cfg: Config,
{
    let (io_r, io_w) = io.split();
    let mut io = duplexify::Duplex::new(
        Box::pin(io_r) as Pin<Box<dyn Send + AsyncRead>>,
        Box::pin(io_w) as Pin<Box<dyn Send + AsyncWrite>>,
    );

    let rdbuf = &mut [0; RDBUF_SIZE];
    let mut unhandled = 0..0;
    // TODO: should have a wrslices: Vec<IoSlice> here, so that we don't allocate
    // for each write, but it looks like the API for reusing a Vec's backing
    // allocation isn't ready yet and IoSlice's lifetime is going to make this
    // impossible. Maybe this would require writing a crate that allows such vec
    // storage recycling, as there doesn't appear to be any on crates.io. Having
    // the wrslices would allow us to avoid all the allocations at each
    // .collect() (present in `send_reply()`)
    let mut conn_meta = ConnectionMetadata {
        user: metadata,
        hello: None,
        is_encrypted: is_already_tls == IsAlreadyTls::Yes,
    };
    let mut mail_meta = None;

    let mut waiting_for_command_since = Utc::now();

    macro_rules! read_for_command {
        ($e:expr) => {
            $e.or(async {
                // TODO: this should be smol::Timer::at, but we would need to convert from
                // Chrono::DateTime<Utc> to std::time::Instant and I can't find how right now
                let max_delay: std::time::Duration =
                    (waiting_for_command_since + cfg.command_read_timeout() - Utc::now())
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(0));
                smol::Timer::after(max_delay).await;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out waiting for a command",
                ))
            })
        };
    }

    macro_rules! send_reply {
        ($writer:expr, $reply:expr) => {
            smol::future::or(
                async {
                    $writer
                        .write_all_vectored(&mut $reply.as_io_slices().collect::<Vec<_>>())
                        .await?;
                    waiting_for_command_since = Utc::now();
                    Ok(())
                },
                async {
                    smol::Timer::after(
                        cfg.reply_write_timeout()
                            .to_std()
                            .unwrap_or(std::time::Duration::from_secs(0)),
                    )
                    .await;
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "timed out sending a reply",
                    ))
                },
            )
        };
    }

    macro_rules! dispatch_decision {
        ($e:expr, Accept($reply:pat, $res:pat) => $accept:block) => {
            dispatch_decision!($e,
                Reject(reply) => {
                    send_reply!(io, reply).await?
                }
                Accept($reply, $res) => $accept
            )
        };

        (
            $e:expr,
            Reject($reply_r:pat) => $reject:block
            Accept($reply_a:pat, $res_a:pat) => $accept:block
        ) => {
            match $e {
                Decision::Accept { reply: $reply_a, res: $res_a } => $accept,
                Decision::Reject { reply: $reply_r } => $reject,
                Decision::Kill { reply, res } => {
                    if let Some(r) = reply {
                        send_reply!(io, r).await?;
                    }
                    return res;
                }
            }
        };
    }

    macro_rules! simple_handler {
        ($handler:expr) => {
            dispatch_decision! {
                $handler,
                Accept(reply, ()) => {
                    send_reply!(io, reply).await?;
                }
            }
        };
    }

    send_reply!(io, cfg.welcome_banner_reply(&mut conn_meta)).await?;

    loop {
        if unhandled.is_empty() {
            unhandled = 0..read_for_command!(io.read(rdbuf)).await?;
            if unhandled.is_empty() {
                return Ok(());
            }
        }

        let cmd = match Command::<&str>::parse(&rdbuf[unhandled.clone()]) {
            Err(nom::Err::Incomplete(n)) => {
                // Don't have enough data to handle command, let's fetch more
                if unhandled.start != 0 {
                    // Do we have to copy the data to the beginning of the buffer?
                    let missing = match n {
                        nom::Needed::Unknown => MINIMUM_FREE_BUFSPACE,
                        nom::Needed::Size(s) => cmp::max(MINIMUM_FREE_BUFSPACE, s.into()),
                    };
                    if missing > rdbuf.len() - unhandled.end {
                        rdbuf.copy_within(unhandled.clone(), 0);
                        unhandled.end = unhandled.len();
                        unhandled.start = 0;
                    }
                }
                if unhandled.end == rdbuf.len() {
                    // If we reach here, it means that unhandled is already
                    // basically the full buffer. Which means that we have to
                    // error out that the line is too long.
                    read_for_command!(advance_until_crlf(&mut io, rdbuf, &mut unhandled)).await?;
                    send_reply!(io, cfg.line_too_long(&mut conn_meta)).await?;
                } else {
                    let read = read_for_command!(io.read(&mut rdbuf[unhandled.end..])).await?;
                    if read == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "connection shutdown with partial command",
                        ));
                    }
                    unhandled.end += read;
                }
                None
            }
            Err(_) => {
                // Syntax error
                read_for_command!(advance_until_crlf(&mut io, rdbuf, &mut unhandled)).await?;
                send_reply!(io, cfg.command_unrecognized(&mut conn_meta)).await?;
                None
            }
            Ok((rem, cmd)) => {
                // Got a command
                unhandled.start = unhandled.end - rem.len();
                Some(cmd)
            }
        };

        // This match is really just to avoid too much rightwards drift, otherwise it
        // could have been included directly in the Ok((rem, cmd)) branch above.
        // Unfortunately we can't make it a function, because `cmd` borrows `rdbuf`, and
        // we need to use `rdbuf` in the `Command::Data` branch here
        match cmd {
            None => (),

            // TODO: find some way to unify with the below branch
            Some(Command::Ehlo { hostname }) => match conn_meta.hello {
                Some(_) => {
                    send_reply!(io, cfg.already_did_hello(&mut conn_meta)).await?;
                }
                None => dispatch_decision! {
                    cfg.filter_hello(true, hostname.into_owned(), &mut conn_meta)
                        .await,
                    Accept(reply, res) => {
                        conn_meta.hello = Some(res);
                        send_reply!(io, reply).await?;
                    }
                },
            },

            Some(Command::Helo { hostname }) => match conn_meta.hello {
                Some(_) => {
                    send_reply!(io, cfg.already_did_hello(&mut conn_meta)).await?;
                }
                None => dispatch_decision! {
                    cfg.filter_hello(false, hostname.into_owned(), &mut conn_meta).await,
                    Accept(reply, res) => {
                        conn_meta.hello = Some(res);
                        send_reply!(io, reply).await?;
                    }
                },
            },

            Some(Command::Mail {
                path: _path,
                email,
                params: _params,
            }) => {
                if conn_meta.hello.is_none() {
                    send_reply!(io, cfg.mail_before_hello(&mut conn_meta)).await?;
                } else {
                    match mail_meta {
                        Some(_) => {
                            // Both postfix and OpenSMTPD just return an error and ignore further
                            // MAIL FROM when there is already a MAIL FROM running
                            send_reply!(io, cfg.already_in_mail(&mut conn_meta)).await?;
                        }
                        None => {
                            let mut mail_metadata = MailMetadata {
                                user: cfg.new_mail(&mut conn_meta).await,
                                from: None,
                                to: Vec::with_capacity(4),
                            };
                            dispatch_decision! {
                                cfg.filter_from(
                                    email.as_ref().map(|e| e.clone().into_owned()),
                                    &mut mail_metadata,
                                    &mut conn_meta,
                                )
                                .await,
                                Accept(reply, res) => {
                                    mail_metadata.from = res;
                                    mail_meta = Some(mail_metadata);
                                    send_reply!(io, reply).await?;
                                }
                            }
                        }
                    }
                }
            }

            Some(Command::Rcpt {
                path: _path,
                email,
                params: _params,
            }) => match mail_meta {
                None => {
                    send_reply!(io, cfg.rcpt_before_mail(&mut conn_meta)).await?;
                }
                Some(ref mut mail_meta_unw) => dispatch_decision! {
                    cfg.filter_to(email.into_owned(), mail_meta_unw, &mut conn_meta).await,
                    Accept(reply, res) => {
                        mail_meta_unw.to.push(res);
                        send_reply!(io, reply).await?;
                    }
                },
            },

            Some(Command::Data) => match mail_meta.take() {
                None => {
                    send_reply!(io, cfg.data_before_mail(&mut conn_meta)).await?;
                }
                Some(ref mail_meta_unw) if mail_meta_unw.to.is_empty() => {
                    send_reply!(io, cfg.data_before_rcpt(&mut conn_meta)).await?;
                }
                Some(mut mail_meta_unw) => {
                    dispatch_decision! {
                        cfg.filter_data(&mut mail_meta_unw, &mut conn_meta).await,
                        Reject(reply) => {
                            mail_meta = Some(mail_meta_unw);
                            send_reply!(io, reply).await?;
                        }
                        Accept(reply, ()) => {
                            send_reply!(io, reply).await?;
                            let mut reader =
                                EscapedDataReader::new(rdbuf, unhandled.clone(), &mut io);
                            let decision = cfg
                                .handle_mail(&mut reader, mail_meta_unw, &mut conn_meta)
                                .await;
                            // This variable is a trick because otherwise rustc thinks the `reader`
                            // borrow is still alive across await points and makes `interact: !Send`
                            let reader_was_completed = if let Some(u) = reader.get_unhandled() {
                                unhandled = u;
                                true
                            } else {
                                false
                            };
                            if reader_was_completed {
                                // Other mail systems (at least
                                // postfix, OpenSMTPD and gmail)
                                // appear to drop the state on an
                                // unsuccessful DATA command (eg. too
                                // long, non-RFC5322-compliant, etc.).
                                // Couldn't find the RFC reference
                                // anywhere, though.
                                simple_handler!(decision);
                            } else {
                                // handle_mail did not call complete, let's read until the end and
                                // then return an error
                                // TODO: 128 is probably too small?
                                let ignore_buf = &mut [0u8; 128];
                                // TODO: consider whether it would make sense to have a separate
                                // timeout here... giving as much time for sending the whole DATA
                                // message may be a bit too little? but then it only happens when
                                // handle_mail breaks anyway, so...
                                while read_for_command!(reader.read(ignore_buf)).await? != 0 {}
                                if !reader.is_finished() {
                                    // Stream cut mid-connection
                                    return Err(io::Error::new(
                                        io::ErrorKind::ConnectionAborted,
                                        "connection shutdown during email reception",
                                    ));
                                }
                                reader.complete();
                                unhandled = reader.get_unhandled().unwrap();
                                send_reply!(io, cfg.handle_mail_did_not_call_complete(&mut conn_meta)).await?;
                            };
                        }
                    }
                }
            },

            Some(Command::Rset) => dispatch_decision! {
                cfg.handle_rset(&mut mail_meta, &mut conn_meta).await,
                Accept(reply, ()) => {
                    mail_meta = None;
                    send_reply!(io, reply).await?;
                }
            },

            Some(Command::Starttls) => {
                if !cfg.can_do_tls(&conn_meta) {
                    send_reply!(io, cfg.starttls_unsupported(&mut conn_meta)).await?;
                } else if !unhandled.is_empty() {
                    send_reply!(io, cfg.pipeline_forbidden_after_starttls(&mut conn_meta)).await?;
                } else {
                    dispatch_decision! {
                        cfg.handle_starttls(&mut conn_meta).await,
                        Accept(reply, ()) => {
                            send_reply!(io, reply).await?;
                            io = cfg.tls_accept(io, &mut conn_meta).await?;
                            mail_meta = None;
                            conn_meta.is_encrypted = true;
                            conn_meta.hello = None;
                        }
                    }
                }
            }

            Some(Command::Expn { name }) => {
                simple_handler!(cfg.handle_expn(name, &mut conn_meta).await)
            }
            Some(Command::Vrfy { name }) => {
                simple_handler!(cfg.handle_vrfy(name, &mut conn_meta).await)
            }
            Some(Command::Help { subject }) => {
                simple_handler!(cfg.handle_help(subject, &mut conn_meta).await)
            }
            Some(Command::Noop { string }) => {
                simple_handler!(cfg.handle_noop(string, &mut conn_meta).await)
            }
            Some(Command::Quit) => simple_handler!(cfg.handle_quit(&mut conn_meta).await),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        self, str,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use duplexify::Duplex;
    use futures::executor;

    use smtp_message::ReplyCode;

    /// Used as `println!("{:?}", show_bytes(b))`
    pub fn show_bytes(b: &[u8]) -> String {
        if b.len() > 512 {
            format!("{{too long, size = {}}}", b.len())
        } else if let Ok(s) = str::from_utf8(b) {
            s.into()
        } else {
            format!("{:?}", b)
        }
    }

    struct TestConfig {
        mails: Arc<Mutex<Vec<(Option<Email>, Vec<Email>, Vec<u8>)>>>,
    }

    #[async_trait]
    impl Config for TestConfig {
        type ConnectionUserMeta = ();
        type MailUserMeta = ();

        fn hostname(&self, _conn_meta: &ConnectionMetadata<()>) -> &str {
            "test.example.org".into()
        }

        async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

        async fn tls_accept<IO>(
            &self,
            mut io: IO,
            _conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
        ) -> io::Result<
            duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>,
        >
        where
            IO: 'static + Unpin + Send + AsyncRead + AsyncWrite,
        {
            io.write_all(b"<tls server>").await?;
            let mut buf = [0; 12];
            io.read_exact(&mut buf).await?;
            assert_eq!(
                &buf,
                b"<tls client>",
                "got TLS handshake that is not <tls client>: {:?}",
                show_bytes(&buf)
            );
            let (r, w) = io.split();
            Ok(duplexify::Duplex::new(Box::pin(r), Box::pin(w)))
        }

        async fn filter_from(
            &self,
            addr: Option<Email>,
            _meta: &mut MailMetadata<()>,
            _conn_meta: &mut ConnectionMetadata<()>,
        ) -> Decision<Option<Email>> {
            // TODO: have a helper function for the Email::parse_until that just works(tm)
            // for uses such as this one
            if addr == Some(Email::parse_bracketed(b"<bad@quux.example.org>").unwrap()) {
                Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::POLICY_REASON,
                        ecode: None,
                        text: vec!["User 'bad' banned".into()],
                    },
                }
            } else {
                Decision::Accept {
                    reply: reply::okay_from().convert(),
                    res: addr,
                }
            }
        }

        async fn filter_to(
            &self,
            email: Email,
            _meta: &mut MailMetadata<()>,
            _conn_meta: &mut ConnectionMetadata<()>,
        ) -> Decision<Email> {
            if email.localpart.raw() == "baz" {
                Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::MAILBOX_UNAVAILABLE,
                        ecode: None,
                        text: vec!["No user 'baz'".into()],
                    },
                }
            } else {
                Decision::Accept {
                    reply: reply::okay_to().convert(),
                    res: email,
                }
            }
        }

        async fn handle_mail<'a, R>(
            &self,
            reader: &mut EscapedDataReader<'a, R>,
            meta: MailMetadata<()>,
            _conn_meta: &mut ConnectionMetadata<()>,
        ) -> Decision<()>
        where
            R: Send + Unpin + AsyncRead,
        {
            let mut mail_text = Vec::new();
            let res = reader.read_to_end(&mut mail_text).await;
            if !reader.is_finished() {
                // Note: this is a stupid buggy implementation.
                // But it allows us to test more code in
                // interrupted_data.
                return Decision::Accept {
                    reply: reply::okay_mail().convert(),
                    res: (),
                };
            }
            reader.complete();
            if res.is_err() {
                Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::BAD_SEQUENCE,
                        ecode: None,
                        text: vec!["Closed the channel before end of message".into()],
                    },
                }
            } else if mail_text.windows(5).position(|x| x == b"World").is_some() {
                Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::POLICY_REASON,
                        ecode: None,
                        text: vec!["Don't you dare say 'World'!".into()],
                    },
                }
            } else {
                self.mails
                    .lock()
                    .expect("failed to load mutex")
                    .push((meta.from, meta.to, mail_text));
                Decision::Accept {
                    reply: reply::okay_mail().convert(),
                    res: (),
                }
            }
        }
    }

    #[test]
    fn interacts_ok() {
        let tests: &[(&[&[u8]], &[u8], &[(Option<&[u8]>, &[&[u8]], &[u8])])] = &[
            (
                &[b"EHLO test\r\n\
                    MAIL FROM:<>\r\n\
                    RCPT TO:<baz@quux.example.org>\r\n\
                    RCPT TO:<foo2@bar.example.org>\r\n\
                    RCPT TO:<foo3@bar.example.org>\r\n\
                    DATA\r\n\
                    Hello world\r\n\
                    .\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250-test.example.org\r\n\
                  250-8BITMIME\r\n\
                  250-ENHANCEDSTATUSCODES\r\n\
                  250-PIPELINING\r\n\
                  250-SMTPUTF8\r\n\
                  250 STARTTLS\r\n\
                  250 2.0.0 Okay\r\n\
                  550 No user 'baz'\r\n\
                  250 2.1.5 Okay\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  250 2.0.0 Okay\r\n\
                  221 2.0.0 Bye\r\n",
                &[(
                    None,
                    &[b"<foo2@bar.example.org>", b"<foo3@bar.example.org>"],
                    b"Hello world\r\n.\r\n",
                )],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<test@example.org>\r\n\
                    RCPT TO:<foo@example.org>\r\n\
                    DATA\r\n\
                    Hello World\r\n\
                    .\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  550 Don't you dare say 'World'!\r\n\
                  221 2.0.0 Bye\r\n",
                &[],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<bad@quux.example.org>\r\n\
                    MAIL FROM:<foo@bar.example.org>\r\n\
                    MAIL FROM:<baz@quux.example.org>\r\n\
                    RCPT TO:<foo2@bar.example.org>\r\n\
                    DATA\r\n\
                    Hello\r\n\
                    .\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  550 User 'bad' banned\r\n\
                  250 2.0.0 Okay\r\n\
                  503 5.5.1 Bad sequence of commands\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  250 2.0.0 Okay\r\n\
                  221 2.0.0 Bye\r\n",
                &[(
                    Some(b"<foo@bar.example.org>"),
                    &[b"<foo2@bar.example.org>"],
                    b"Hello\r\n.\r\n",
                )],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<foo@bar.example.org>\r\n\
                    RSET\r\n\
                    MAIL FROM:<baz@quux.example.org>\r\n\
                    RCPT TO:<foo2@bar.example.org>\r\n\
                    DATA\r\n\
                    Hello\r\n\
                    .\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  250 2.0.0 Okay\r\n\
                  221 2.0.0 Bye\r\n",
                &[(
                    Some(b"<baz@quux.example.org>"),
                    &[b"<foo2@bar.example.org>"],
                    b"Hello\r\n.\r\n",
                )],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<foo@test.example.com>\r\n\
                    DATA\r\n\
                    QUIT\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  250 2.0.0 Okay\r\n\
                  503 5.5.1 Bad sequence of commands\r\n\
                  221 2.0.0 Bye\r\n",
                &[],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<foo@test.example.com>\r\n\
                    RCPT TO:<foo@bar.example.org>\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.1.5 Okay\r\n",
                &[],
            ),
            (
                &[b"HELO test\r\n\
                    MAIL FROM:<foo@test.example.com>\r\n\
                    THISISNOTACOMMAND\r\n\
                    RCPT TO:<foo@bar.example.org>\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  250 2.0.0 Okay\r\n\
                  500 5.5.1 Command not recognized\r\n\
                  250 2.1.5 Okay\r\n",
                &[],
            ),
            (
                &[b"MAIL FROM:<foo@test.example.com>\r\n"],
                b"220 test.example.org Service ready\r\n\
                  503 5.5.1 Bad sequence of commands\r\n",
                &[],
            ),
            (
                &[b"HELO test\r\n\
                    EXPN foo\r\n\
                    VRFY bar\r\n\
                    HELP baz\r\n\
                    NOOP\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  502 5.5.1 Command not implemented\r\n\
                  252 2.1.5 Cannot VRFY user, but will accept message and attempt delivery\r\n\
                  214 2.0.0 See https://tools.ietf.org/html/rfc5321\r\n\
                  250 2.0.0 Okay\r\n",
                &[],
            ),
            (
                &[b"HELO test\r\n\
                    EXPN foo\r\n\
                    QUIT\r\n\
                    HELP baz\r\n"],
                b"220 test.example.org Service ready\r\n\
                  250 test.example.org\r\n\
                  502 5.5.1 Command not implemented\r\n\
                  221 2.0.0 Bye\r\n",
                &[],
            ),
            (
                &[
                    b"EHLO test\r\n\
                      STARTTLS\r\n",
                    b"<tls client>",
                    b"EHLO test2\r\n",
                ],
                b"220 test.example.org Service ready\r\n\
                  250-test.example.org\r\n\
                  250-8BITMIME\r\n\
                  250-ENHANCEDSTATUSCODES\r\n\
                  250-PIPELINING\r\n\
                  250-SMTPUTF8\r\n\
                  250 STARTTLS\r\n\
                  220 2.0.0 Ready to start TLS\r\n\
                  <tls server>\
                  250-test.example.org\r\n\
                  250-8BITMIME\r\n\
                  250-ENHANCEDSTATUSCODES\r\n\
                  250-PIPELINING\r\n\
                  250 SMTPUTF8\r\n",
                &[],
            ),
        ];
        for &(inp, out, mail) in tests {
            println!(
                "\nSending: {:?}",
                inp.iter().map(|b| show_bytes(*b)).collect::<Vec<_>>()
            );
            let resp_mail = Arc::new(Mutex::new(Vec::new()));
            let cfg = Arc::new(TestConfig {
                mails: resp_mail.clone(),
            });
            let (inp_pipe_r, mut inp_pipe_w) = piper::pipe(1024 * 1024);
            let (mut out_pipe_r, out_pipe_w) = piper::pipe(1024 * 1024);
            let io = Duplex::new(inp_pipe_r, out_pipe_w);
            let ((), resp) = smol::block_on(futures::future::join(
                async move {
                    for i in inp {
                        // Yield 100 times to be sure the interact process had enough time to
                        // process the data
                        for _ in 0..100usize {
                            smol::future::yield_now().await;
                        }
                        inp_pipe_w
                            .write_all(i)
                            .await
                            .expect("writing to input pipe");
                    }
                },
                async move {
                    interact(io, IsAlreadyTls::No, (), cfg)
                        .await
                        .expect("calling interact");
                    let mut resp = Vec::new();
                    out_pipe_r
                        .read_to_end(&mut resp)
                        .await
                        .expect("reading from output pipe");
                    resp
                },
            ));

            println!("Expecting: {:?}", show_bytes(out));
            println!("Got      : {:?}", show_bytes(&resp));
            assert_eq!(resp, out);

            println!("Checking mails:");
            let resp_mail = Arc::try_unwrap(resp_mail).unwrap().into_inner().unwrap();
            assert_eq!(resp_mail.len(), mail.len());
            for ((fr, tr, cr), &(fo, to, co)) in resp_mail.into_iter().zip(mail) {
                println!("Mail\n---");

                println!("From: expected {:?}, got {:?}", fo, fr);
                assert_eq!(fo.map(|e| Email::parse_bracketed(e).unwrap()), fr);

                let to = to
                    .iter()
                    .map(|e| Email::parse_bracketed(e).unwrap())
                    .collect::<Vec<_>>();
                println!("To: expected {:?}, got {:?}", to, tr);
                assert_eq!(to, tr);

                println!("Expected text: {:?}", show_bytes(co));
                println!("Got text     : {:?}", show_bytes(&cr));
                assert_eq!(co, &cr[..]);
            }
        }
    }

    // Fuzzer-found
    #[test]
    fn interrupted_data() {
        let inp: &[u8] = b"MAIL FROM:foo\r\n\
                           RCPT TO:bar\r\n\
                           DATA\r\n\
                           hello";
        let cfg = Arc::new(TestConfig {
            mails: Arc::new(Mutex::new(Vec::new())),
        });
        let (inp_pipe_r, mut inp_pipe_w) = piper::pipe(1024 * 1024);
        let (_out_pipe_r, out_pipe_w) = piper::pipe(1024 * 1024);
        let io = Duplex::new(inp_pipe_r, out_pipe_w);
        let err_kind = executor::block_on(async move {
            inp_pipe_w
                .write_all(inp)
                .await
                .expect("writing to input pipe");
            std::mem::drop(inp_pipe_w);
            interact(io, IsAlreadyTls::No, (), cfg)
                .await
                .expect_err("calling interact")
                .kind()
        });
        assert_eq!(err_kind, io::ErrorKind::ConnectionAborted,);
    }

    // Fuzzer-found
    #[test]
    fn no_stack_overflow() {
        let inp: &[u8] =
            b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\r\n";
        let cfg = Arc::new(TestConfig {
            mails: Arc::new(Mutex::new(Vec::new())),
        });
        let (inp_pipe_r, mut inp_pipe_w) = piper::pipe(1024 * 1024);
        let (_out_pipe_r, out_pipe_w) = piper::pipe(1024 * 1024);
        let io = Duplex::new(inp_pipe_r, out_pipe_w);
        executor::block_on(async move {
            inp_pipe_w
                .write_all(inp)
                .await
                .expect("writing to input pipe");
            std::mem::drop(inp_pipe_w);
            interact(io, IsAlreadyTls::No, (), cfg)
                .await
                .expect("calling interact");
        });
    }

    struct MinBoundsIo;
    impl !Sync for MinBoundsIo {}
    impl AsyncRead for MinBoundsIo {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut [u8],
        ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
            unimplemented!()
        }
    }
    impl AsyncWrite for MinBoundsIo {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &[u8],
        ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
            unimplemented!()
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
            unimplemented!()
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
            unimplemented!()
        }
    }

    fn assert_send<T: Send>(_: T) {}

    #[test]
    fn interact_is_send() {
        let cfg = Arc::new(TestConfig {
            mails: Arc::new(Mutex::new(Vec::new())),
        });
        assert_send(interact(MinBoundsIo, IsAlreadyTls::No, (), cfg));
    }
}
