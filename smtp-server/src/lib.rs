#![feature(io_slice_advance)]

use std::{
    borrow::Cow,
    cmp,
    future::Future,
    io::{self, IoSlice},
    ops::Range,
    pin::Pin,
};

use async_trait::async_trait;
use futures::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    pin_mut,
};
use smtp_message::{
    nom, Command, Email, EnhancedReplyCode, EscapedDataReader, MaybeUtf8, Reply, ReplyCode,
};

pub const RDBUF_SIZE: usize = 16 * 1024;
const MINIMUM_FREE_BUFSPACE: usize = 128;

#[must_use]
pub enum Decision {
    Accept,
    Reject(Reply<Cow<'static, str>>),
}

pub struct MailMetadata<U> {
    pub user: U,
    pub from: Option<Email>,
    pub to: Vec<Email>,
}

pub struct ConnectionMetadata<U> {
    pub user: U,
}

#[async_trait]
pub trait Config: Send + Sync {
    type ConnectionUserMeta: Send;
    type MailUserMeta: Send;

    // TODO: this could have a default implementation if we were able to have a
    // default type of () for MailUserMeta without requiring unstable
    async fn new_mail(
        &self,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Self::MailUserMeta;

    async fn filter_from(
        &self,
        from: &mut Option<Email<&str>>,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision;

    async fn filter_to(
        &self,
        to: &mut Email<&str>,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision;

    #[allow(unused_variables)]
    async fn filter_data(
        &self,
        meta: &mut MailMetadata<Self::MailUserMeta>,
        conn_meta: &mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Decision {
        Decision::Accept
    }

    // TODO: can this be an async fn?
    // see https://github.com/rust-lang/rust/issues/71058
    /// Note: the EscapedDataReader has an inner buffer size of
    /// [`RDBUF_SIZE`](RDBUF_SIZE), which means that reads should not happen
    /// with more than this buffer size.
    fn handle_mail<'a, R>(
        &'a self,
        stream: &'a mut EscapedDataReader<'a, R>,
        meta: MailMetadata<Self::MailUserMeta>,
        conn_meta: &'a mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        R: 'a + Send + Unpin + AsyncRead;

    fn hostname(&self) -> Cow<'static, str>;

    fn banner(&self) -> Cow<'static, str> {
        "Service ready".into()
    }

    fn welcome_banner(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::SERVICE_READY,
            ecode: None,
            text: vec![MaybeUtf8::Utf8(self.hostname() + " " + self.banner())],
        }
    }

    fn okay(&self, ecode: EnhancedReplyCode<Cow<'static, str>>) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::OKAY,
            ecode: Some(ecode),
            text: vec![MaybeUtf8::Utf8("Okay".into())],
        }
    }

    fn mail_okay(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_UNDEFINED.into())
    }

    fn rcpt_okay(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_DEST_VALID.into())
    }

    fn data_okay(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::START_MAIL_INPUT,
            ecode: None,
            text: vec![MaybeUtf8::Utf8(
                "Start mail input; end with <CRLF>.<CRLF>".into(),
            )],
        }
    }

    fn mail_accepted(&self) -> Reply<Cow<'static, str>> {
        self.okay(EnhancedReplyCode::SUCCESS_UNDEFINED.into())
    }

    fn bad_sequence(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::BAD_SEQUENCE,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND.into()),
            text: vec![MaybeUtf8::Utf8("Bad sequence of commands".into())],
        }
    }

    fn already_in_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn rcpt_before_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn data_before_rcpt(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn data_before_mail(&self) -> Reply<Cow<'static, str>> {
        self.bad_sequence()
    }

    fn command_unimplemented(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::COMMAND_UNIMPLEMENTED,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND.into()),
            text: vec![MaybeUtf8::Utf8("Command not implemented".into())],
        }
    }

    fn command_unrecognized(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::COMMAND_UNRECOGNIZED,
            ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND.into()),
            text: vec![MaybeUtf8::Utf8("Command not recognized".into())],
        }
    }

    fn line_too_long(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::COMMAND_UNRECOGNIZED,
            ecode: Some(EnhancedReplyCode::PERMANENT_UNDEFINED.into()),
            text: vec![MaybeUtf8::Utf8("Line too long".into())],
        }
    }
}

// TODO: upstream in AsyncWriteExt?
async fn write_vectored_all<W>(w: &mut W, bufs: &mut [IoSlice<'_>]) -> io::Result<()>
where
    W: Unpin + AsyncWrite,
{
    let mut bufs = bufs;
    let mut len = bufs.iter().map(|b| b.len()).sum::<usize>();
    while len > 0 {
        let toskip = w.write_vectored(bufs).await?;
        bufs = IoSlice::advance(bufs, toskip);
        len -= toskip;
    }
    Ok(())
}

macro_rules! send_reply {
    ($writer:expr, $reply:expr) => {
        write_vectored_all(&mut $writer, &mut $reply.as_io_slices().collect::<Vec<_>>())
    };
}

pub async fn interact<IO, Cfg>(
    io: IO,
    metadata: Cfg::ConnectionUserMeta,
    cfg: &Cfg,
) -> io::Result<()>
where
    IO: Send + AsyncRead + AsyncWrite,
    Cfg: Config,
{
    pin_mut!(io);
    let mut rdbuf = &mut [0; RDBUF_SIZE];
    let mut unhandled = 0..0;
    // TODO: should have a wrslices: Vec<IoSlice> here, so that we don't allocate
    // for each write, but it looks like the API for reusing a Vec's backing
    // allocation isn't ready yet and IoSlice's lifetime is going to make this
    // impossible. Maybe this would require writing a crate that allows such vec
    // storage recycling, as there doesn't appear to be any on crates.io. Having
    // the wrslices would allow us to avoid all the allocations at each
    // .collect() (present in `send_reply()`)
    let mut conn_meta = ConnectionMetadata { user: metadata };
    let mut mail_meta = None;

    send_reply!(io, cfg.welcome_banner()).await?;

    loop {
        if unhandled.len() == 0 {
            unhandled = 0..io.read(rdbuf).await?;
        }

        let cmd = match Command::<&str>::parse(&rdbuf[unhandled.clone()]) {
            Err(nom::Err::Incomplete(n)) => {
                // Don't have enough data to handle command, let's fetch more
                if unhandled.start != 0 {
                    // Do we have to copy the data to the beginning of the buffer?
                    let missing = match n {
                        nom::Needed::Unknown => MINIMUM_FREE_BUFSPACE,
                        nom::Needed::Size(s) => cmp::max(MINIMUM_FREE_BUFSPACE, s),
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
                    // TODO: error out only when the \r\n is received, and allow the communication
                    // to continue
                    send_reply!(io, cfg.line_too_long()).await?;
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "received too long line",
                    ));
                }
                unhandled.end += io.read(&mut rdbuf[unhandled.end..]).await?;
                None
            }
            Err(_) => {
                // Syntax error
                // TODO: error out only when the \r\n is received, and allow the
                // communication to continue
                send_reply!(io, cfg.command_unrecognized()).await?;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "received invalid command",
                ));
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

            Some(Command::Mail {
                path: _path,
                mut email,
                params: _params,
            }) => match mail_meta {
                Some(_) => {
                    send_reply!(io, cfg.already_in_mail()).await?;
                }
                None => {
                    let mut mail_metadata = MailMetadata {
                        user: cfg.new_mail(&mut conn_meta).await,
                        from: None,
                        to: Vec::with_capacity(4),
                    };
                    match cfg
                        .filter_from(&mut email, &mut mail_metadata, &mut conn_meta)
                        .await
                    {
                        Decision::Reject(r) => {
                            send_reply!(io, r).await?;
                        }
                        Decision::Accept => {
                            mail_metadata.from = email.map(|e| e.to_owned());
                            mail_meta = Some(mail_metadata);
                            send_reply!(io, cfg.mail_okay()).await?;
                        }
                    }
                }
            },

            Some(Command::Rcpt {
                path: _path,
                mut email,
                params: _params,
            }) => match mail_meta {
                None => {
                    send_reply!(io, cfg.rcpt_before_mail()).await?;
                }
                Some(ref mut mail_meta_unw) => {
                    match cfg
                        .filter_to(&mut email, mail_meta_unw, &mut conn_meta)
                        .await
                    {
                        Decision::Reject(r) => {
                            send_reply!(io, r).await?;
                        }
                        Decision::Accept => {
                            mail_meta_unw.to.push(email.to_owned());
                            send_reply!(io, cfg.rcpt_okay()).await?;
                        }
                    }
                }
            },

            Some(Command::Data) => match mail_meta.take() {
                None => {
                    send_reply!(io, cfg.data_before_mail()).await?;
                }
                Some(ref mail_meta_unw) if mail_meta_unw.to.is_empty() => {
                    send_reply!(io, cfg.data_before_rcpt()).await?;
                }
                Some(mut mail_meta_unw) => {
                    match cfg.filter_data(&mut mail_meta_unw, &mut conn_meta).await {
                        Decision::Reject(r) => {
                            mail_meta = Some(mail_meta_unw);
                            send_reply!(io, r).await?;
                        }
                        Decision::Accept => {
                            send_reply!(io, cfg.data_okay()).await?;
                            let mut reader =
                                EscapedDataReader::new(rdbuf, unhandled.clone(), &mut io);
                            let decision = cfg
                                .handle_mail(&mut reader, mail_meta_unw, &mut conn_meta)
                                .await;
                            // TODO: unhandled = reader.complete();
                            match decision {
                                Decision::Accept => {
                                    send_reply!(io, cfg.mail_accepted()).await?;
                                }
                                Decision::Reject(r) => {
                                    send_reply!(io, r).await?;
                                    // Other mail systems (at least postfix,
                                    // OpenSMTPD and gmail) appear to drop the
                                    // state on an unsuccessful DATA command
                                    // (eg. too long, non-RFC5322-compliant,
                                    // etc.). Couldn't find the RFC reference
                                    // anywhere, though.
                                }
                            }
                        }
                    }
                }
            },

            Some(_) => {
                // TODO: this probably shouldn't be required
                send_reply!(io, cfg.command_unimplemented()).await?;
            }
        }
    }
}
