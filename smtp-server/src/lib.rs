#![feature(io_slice_advance)]
#![type_length_limit = "200000000"]

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
    next_crlf, nom, Command, Email, EnhancedReplyCode, EscapedDataReader, MaybeUtf8, NextCrLfState,
    Reply, ReplyCode,
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
    fn handle_mail<'a, 'b, R>(
        &'a self,
        stream: &'a mut EscapedDataReader<'b, R>,
        meta: MailMetadata<Self::MailUserMeta>,
        conn_meta: &'a mut ConnectionMetadata<Self::ConnectionUserMeta>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        'b: 'a,
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

    fn handle_mail_did_not_call_complete(&self) -> Reply<Cow<'static, str>> {
        Reply {
            code: ReplyCode::LOCAL_ERROR,
            ecode: Some(EnhancedReplyCode::TRANSIENT_SYSTEM_INCORRECTLY_CONFIGURED.into()),
            text: vec![MaybeUtf8::Utf8("System incorrectly configured".into())],
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
    let rdbuf = &mut [0; RDBUF_SIZE];
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
            if unhandled.len() == 0 {
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
                    advance_until_crlf(&mut io, rdbuf, &mut unhandled).await?;
                    send_reply!(io, cfg.line_too_long()).await?;
                } else {
                    let read = io.read(&mut rdbuf[unhandled.end..]).await?;
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
                advance_until_crlf(&mut io, rdbuf, &mut unhandled).await?;
                send_reply!(io, cfg.command_unrecognized()).await?;
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
                            if let Some(u) = reader.get_unhandled() {
                                unhandled = u;
                                match decision {
                                    Decision::Accept => {
                                        send_reply!(io, cfg.mail_accepted()).await?;
                                    }
                                    Decision::Reject(r) => {
                                        send_reply!(io, r).await?;
                                        // Other mail systems (at least postfix,
                                        // OpenSMTPD and gmail) appear to drop
                                        // the state on an unsuccessful DATA
                                        // command (eg. too long,
                                        // non-RFC5322-compliant, etc.).
                                        // Couldn't find the RFC reference
                                        // anywhere, though.
                                    }
                                }
                            } else {
                                // handle_mail did not call complete, let's read until the end and
                                // then return an error
                                let ignore_buf = &mut [0u8; 128];
                                while reader.read(ignore_buf).await? != 0 {}
                                if !reader.is_finished() {
                                    // Stream cut mid-connection
                                    return Err(io::Error::new(
                                        io::ErrorKind::ConnectionAborted,
                                        "connection shutdown during email reception",
                                    ));
                                }
                                reader.complete();
                                unhandled = reader.get_unhandled().unwrap();
                                send_reply!(io, cfg.handle_mail_did_not_call_complete()).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        self, str,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use duplexify::Duplex;
    use futures::{executor, io::Cursor};

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

        fn hostname(&self) -> Cow<'static, str> {
            "test.example.org".into()
        }

        async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) {}

        async fn filter_from(
            &self,
            addr: &mut Option<Email<&str>>,
            _meta: &mut MailMetadata<()>,
            _conn_meta: &mut ConnectionMetadata<()>,
        ) -> Decision {
            // TODO: have a helper function for the Email::parse_until that just works(tm)
            // for uses such as this one
            if *addr == Some(Email::parse_bracketed(b"<bad@quux.example.org>").unwrap()) {
                Decision::Reject(Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec!["User 'bad' banned".into()],
                })
            } else {
                Decision::Accept
            }
        }

        async fn filter_to(
            &self,
            email: &mut Email<&str>,
            _meta: &mut MailMetadata<()>,
            _conn_meta: &mut ConnectionMetadata<()>,
        ) -> Decision {
            if *email.localpart.raw() == "baz" {
                Decision::Reject(Reply {
                    code: ReplyCode::MAILBOX_UNAVAILABLE,
                    ecode: None,
                    text: vec!["No user 'baz'".into()],
                })
            } else {
                Decision::Accept
            }
        }

        fn handle_mail<'a, 'b, R>(
            &'a self,
            reader: &'a mut EscapedDataReader<'b, R>,
            meta: MailMetadata<()>,
            _conn_meta: &'a mut ConnectionMetadata<()>,
        ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
        where
            'b: 'a,
            R: 'a + Send + Unpin + AsyncRead,
        {
            Box::pin(async move {
                let mut mail_text = Vec::new();
                let res = reader.read_to_end(&mut mail_text).await;
                reader.complete();
                if res.is_err() {
                    Decision::Reject(Reply {
                        code: ReplyCode::BAD_SEQUENCE,
                        ecode: None,
                        text: vec!["Closed the channel before end of message".into()],
                    })
                } else if mail_text.windows(5).position(|x| x == b"World").is_some() {
                    Decision::Reject(Reply {
                        code: ReplyCode::POLICY_REASON,
                        ecode: None,
                        text: vec!["Don't you dare say 'World'!".into()],
                    })
                } else {
                    self.mails
                        .lock()
                        .expect("failed to load mutex")
                        .push((meta.from, meta.to, mail_text));
                    Decision::Accept
                }
            })
        }
    }

    #[test]
    fn interacts_ok() {
        let tests: &[(&[u8], &[u8], &[(Option<&[u8]>, &[&[u8]], &[u8])])] = &[
            (
                b"MAIL FROM:<>\r\n\
                  RCPT TO:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  RCPT TO:<foo3@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello world\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"220 test.example.org Service ready\r\n\
                  250 2.0.0 Okay\r\n\
                  550 No user 'baz'\r\n\
                  250 2.1.5 Okay\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  250 2.0.0 Okay\r\n\
                  502 5.5.1 Command not implemented\r\n",
                &[(
                    None,
                    &[b"<foo2@bar.example.org>", b"<foo3@bar.example.org>"],
                    b"Hello world\r\n.\r\n",
                )],
            ),
            (
                b"MAIL FROM:<test@example.org>\r\n\
                  RCPT TO:<foo@example.org>\r\n\
                  DATA\r\n\
                  Hello World\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"220 test.example.org Service ready\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  550 Don't you dare say 'World'!\r\n\
                  502 5.5.1 Command not implemented\r\n",
                &[],
            ),
            (
                b"HELP hello\r\n",
                b"220 test.example.org Service ready\r\n\
                  502 5.5.1 Command not implemented\r\n",
                &[],
            ),
            (
                b"MAIL FROM:<bad@quux.example.org>\r\n\
                  MAIL FROM:<foo@bar.example.org>\r\n\
                  MAIL FROM:<baz@quux.example.org>\r\n\
                  RCPT TO:<foo2@bar.example.org>\r\n\
                  DATA\r\n\
                  Hello\r\n\
                  .\r\n\
                  QUIT\r\n",
                b"220 test.example.org Service ready\r\n\
                  550 User 'bad' banned\r\n\
                  250 2.0.0 Okay\r\n\
                  503 5.5.1 Bad sequence of commands\r\n\
                  250 2.1.5 Okay\r\n\
                  354 Start mail input; end with <CRLF>.<CRLF>\r\n\
                  250 2.0.0 Okay\r\n\
                  502 5.5.1 Command not implemented\r\n",
                &[(
                    Some(b"<foo@bar.example.org>"),
                    &[b"<foo2@bar.example.org>"],
                    b"Hello\r\n.\r\n",
                )],
            ),
            (
                b"MAIL FROM:<foo@test.example.com>\r\n\
                  DATA\r\n\
                  QUIT\r\n",
                b"220 test.example.org Service ready\r\n\
                  250 2.0.0 Okay\r\n\
                  503 5.5.1 Bad sequence of commands\r\n\
                  502 5.5.1 Command not implemented\r\n",
                &[],
            ),
            (
                b"MAIL FROM:<foo@test.example.com>\r\n\
                  RCPT TO:<foo@bar.example.org>\r\n",
                b"220 test.example.org Service ready\r\n\
                  250 2.0.0 Okay\r\n\
                  250 2.1.5 Okay\r\n",
                &[],
            ),
            (
                b"MAIL FROM:<foo@test.example.com>\r\n\
                  THISISNOTACOMMAND\r\n\
                  RCPT TO:<foo@bar.example.org>\r\n",
                b"220 test.example.org Service ready\r\n\
                  250 2.0.0 Okay\r\n\
                  500 5.5.1 Command not recognized\r\n\
                  250 2.1.5 Okay\r\n",
                &[],
            ),
        ];
        for &(inp, out, mail) in tests {
            println!("\nSending: {:?}", show_bytes(inp));
            let resp_mail = Arc::new(Mutex::new(Vec::new()));
            let cfg = TestConfig {
                mails: resp_mail.clone(),
            };
            let mut resp = Vec::new();
            let io = Duplex::new(Cursor::new(inp), Cursor::new(&mut resp));
            executor::block_on(interact(io, (), &cfg)).unwrap();

            println!("Expecting: {:?}", show_bytes(out));
            println!("Got      : {:?}", show_bytes(&resp));
            assert_eq!(resp, out);

            println!("Checking mails:");
            drop(cfg);
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
        let txt: &[u8] = b"MAIL FROM:foo\r\n\
                           RCPT TO:bar\r\n\
                           DATA\r\n\
                           hello";
        let cfg = TestConfig {
            mails: Arc::new(Mutex::new(Vec::new())),
        };
        let mut resp = Vec::new();
        let io = Duplex::new(Cursor::new(txt), Cursor::new(&mut resp));
        assert_eq!(
            executor::block_on(interact(io, (), &cfg))
                .unwrap_err()
                .kind(),
            io::ErrorKind::ConnectionAborted,
        );
    }

    // Fuzzer-found
    #[test]
    fn no_stack_overflow() {
        let txt: &[u8] =
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
              \r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r\n\r\n\r\n\r\n\r\n\n\r\n\n\r";
        let cfg = TestConfig {
            mails: Arc::new(Mutex::new(Vec::new())),
        };
        let mut resp = Vec::new();
        let io = Duplex::new(Cursor::new(txt), Cursor::new(&mut resp));
        executor::block_on(interact(io, (), &cfg)).unwrap();
    }
}
