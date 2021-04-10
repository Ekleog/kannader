use std::{
    cmp, collections::BTreeMap, fmt, future::Future, io, net::IpAddr, ops::Range, pin::Pin,
    sync::Arc,
};

use async_trait::async_trait;
use bitflags::bitflags;
use chrono::Utc;
use futures::{pin_mut, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use rand::prelude::SliceRandom;
use smol::net::TcpStream;
use tracing::trace;
use trust_dns_resolver::{
    error::{ResolveError, ResolveErrorKind},
    proto::error::ProtoError,
    AsyncResolver, IntoName,
};

use smtp_message::{
    nom, Command, Email, EnhancedReplyCodeSubject, Hostname, Parameters, Reply, ReplyCodeKind,
};

const SMTP_PORT: u16 = 25;

const RDBUF_SIZE: usize = 16 * 1024;
const DATABUF_SIZE: usize = 16 * 1024;
const MINIMUM_FREE_BUFSPACE: usize = 128;

const ZERO_DURATION: std::time::Duration = std::time::Duration::from_secs(0);

pub type DynAsyncReadWrite =
    duplexify::Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>;

#[derive(Eq, Hash, PartialEq)]
pub struct Destination {
    host: Hostname,
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.host.fmt(f)
    }
}

#[async_trait]
pub trait Config {
    fn ehlo_hostname(&self) -> Hostname<String>;

    fn can_do_tls(&self) -> bool {
        true
    }

    // TODO: make this parameterized on the destination
    fn must_do_tls(&self) -> bool {
        false
    }

    /// Note: If this function can only fail, make can_do_tls return false
    async fn tls_connect<IO>(&self, io: IO) -> io::Result<DynAsyncReadWrite>
    where
        IO: 'static + Unpin + Send + AsyncRead + AsyncWrite;

    fn banner_read_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn command_write_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn ehlo_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn starttls_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(2)
    }

    fn mail_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn rcpt_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(5)
    }

    fn data_init_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(2)
    }

    fn data_block_write_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(3)
    }

    fn data_end_reply_timeout(&self) -> chrono::Duration {
        chrono::Duration::minutes(10)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Retrieving MX DNS records for ‘{0}’")]
    DnsMx(String, #[source] ResolveError),

    #[error("Converting hostname ‘{0}’ to to-be-resolved name")]
    HostToTrustDns(String, #[source] ProtoError),

    #[error("Retrieving IP DNS records for ‘{1}’")]
    DnsIp(trust_dns_resolver::Name, #[source] ResolveError),

    #[error("Connecting to ‘{0}’ port ‘{1}’")]
    Connecting(IpAddr, u16, #[source] io::Error),

    #[error("Receiving reply bytes")]
    ReceivingReplyBytes(#[source] io::Error),

    #[error("Timed out while waiting for a reply")]
    TimedOutWaitingForReply,

    #[error("Connection aborted")]
    ConnectionAborted,

    #[error("Reply does not fit in buffer: ‘{0}’")]
    TooLongReply(String),

    #[error("Syntax error parsing as a reply: ‘{0}’")]
    SyntaxError(String),

    #[error("Timed out while sending a command")]
    TimedOutSendingCommand,

    #[error("Sending command")]
    SendingCommand(#[source] io::Error),

    #[error("Negotiating TLS")]
    NegotiatingTls(#[source] io::Error),

    #[error("Cannot do TLS with remote server")]
    CannotDoTls,

    // TODO: add the command as error context
    #[error("Mail-level transient issue: {0}")]
    TransientMail(Reply),

    #[error("Mailbox-level transient issue: {0}")]
    TransientMailbox(Reply),

    #[error("Mail system-level transient issue: {0}")]
    TransientMailSystem(Reply),

    #[error("Mail-level permanent issue: {0}")]
    PermanentMail(Reply),

    #[error("Mailbox-level permanent issue: {0}")]
    PermanentMailbox(Reply),

    #[error("Mail system-level permanent issue: {0}")]
    PermanentMailSystem(Reply),

    #[error("Unexpected reply code: {0}")]
    UnexpectedReplyCode(Reply),

    #[error("Timed out while sending data")]
    TimedOutSendingData,

    #[error("Sending data")]
    SendingData(#[source] io::Error),

    #[error("Reading the mail from the provided reader")]
    ReadingMail(#[source] io::Error),
}

pub enum TransportErrorSeverity {
    Local,
    NetworkTransient,
    MailTransient,
    MailboxTransient,
    MailSystemTransient,
    MailPermanent,
    MailboxPermanent,
    MailSystemPermanent,
}

impl TransportError {
    pub fn severity(&self) -> TransportErrorSeverity {
        // TODO: Re-run over all these failure modes and check that the kind assignment
        // is correct. Maybe add categories like ProtocolPermanent for invalid
        // hostnames, or LocalTransient for local errors like “too many sockets opened”?
        match self {
            TransportError::DnsMx(_, _) => TransportErrorSeverity::NetworkTransient,
            TransportError::HostToTrustDns(_, _) => TransportErrorSeverity::Local,
            TransportError::DnsIp(_, _) => TransportErrorSeverity::NetworkTransient,
            TransportError::Connecting(_, _, _) => TransportErrorSeverity::NetworkTransient,
            TransportError::ReceivingReplyBytes(_) => TransportErrorSeverity::NetworkTransient,
            TransportError::TimedOutWaitingForReply => TransportErrorSeverity::NetworkTransient,
            TransportError::ConnectionAborted => TransportErrorSeverity::NetworkTransient,
            TransportError::TooLongReply(_) => TransportErrorSeverity::NetworkTransient,
            TransportError::SyntaxError(_) => TransportErrorSeverity::MailSystemTransient,
            TransportError::TimedOutSendingCommand => TransportErrorSeverity::NetworkTransient,
            TransportError::SendingCommand(_) => TransportErrorSeverity::NetworkTransient,
            TransportError::NegotiatingTls(_) => TransportErrorSeverity::NetworkTransient, /* TODO: MailSystemPermanent? */
            TransportError::CannotDoTls => TransportErrorSeverity::NetworkTransient, /* TODO: MailSystemPermanent? */
            TransportError::TransientMail(_) => TransportErrorSeverity::MailTransient,
            TransportError::TransientMailbox(_) => TransportErrorSeverity::MailboxTransient,
            TransportError::TransientMailSystem(_) => TransportErrorSeverity::MailSystemTransient,
            TransportError::PermanentMail(_) => TransportErrorSeverity::MailPermanent,
            TransportError::PermanentMailbox(_) => TransportErrorSeverity::MailboxPermanent,
            TransportError::PermanentMailSystem(_) => TransportErrorSeverity::MailSystemPermanent,
            TransportError::UnexpectedReplyCode(_) => TransportErrorSeverity::NetworkTransient,
            TransportError::TimedOutSendingData => TransportErrorSeverity::NetworkTransient,
            TransportError::SendingData(_) => TransportErrorSeverity::NetworkTransient,
            TransportError::ReadingMail(_) => TransportErrorSeverity::Local,
        }
    }
}

async fn read_for_reply<T>(
    fut: impl Future<Output = io::Result<T>>,
    waiting_for_reply_since: &chrono::DateTime<Utc>,
    timeout: chrono::Duration,
) -> Result<T, TransportError> {
    smol::future::or(
        async { fut.await.map_err(TransportError::ReceivingReplyBytes) },
        async {
            // TODO: this should be smol::Timer::at, but we would need to convert from
            // Chrono::DateTime<Utc> to std::time::Instant and I can't find how right now
            let max_delay: std::time::Duration = (*waiting_for_reply_since + timeout - Utc::now())
                .to_std()
                .unwrap_or(ZERO_DURATION);
            smol::Timer::after(max_delay).await;
            Err(TransportError::TimedOutWaitingForReply)
        },
    )
    .await
}

async fn read_reply<IO>(
    io: &mut IO,
    rdbuf: &mut [u8; RDBUF_SIZE],
    unhandled: &mut Range<usize>,
    timeout: chrono::Duration,
) -> Result<Reply, TransportError>
where
    IO: Unpin + Send + AsyncRead + AsyncWrite,
{
    let start = Utc::now();
    // TODO: try to think of unifying this logic with the one in smtp-server?
    if (*unhandled).is_empty() {
        *unhandled = 0..read_for_reply(io.read(rdbuf), &start, timeout).await?;
        if (*unhandled).is_empty() {
            return Err(TransportError::ConnectionAborted);
        }
    }
    loop {
        trace!(
            buf = String::from_utf8_lossy(&rdbuf[unhandled.clone()]).as_ref(),
            "Trying to parse from buffer"
        );
        match Reply::<&str>::parse(&rdbuf[unhandled.clone()]) {
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
                    // error out that the reply is too big.
                    // TODO: maybe there's something intelligent to be done here, like parsing reply
                    // line per reply line?
                    return Err(TransportError::TooLongReply(
                        String::from_utf8_lossy(&rdbuf[unhandled.clone()]).to_string(),
                    ));
                } else {
                    let read =
                        read_for_reply(io.read(&mut rdbuf[unhandled.end..]), &start, timeout)
                            .await?;
                    if read == 0 {
                        return Err(TransportError::ConnectionAborted);
                    }
                    unhandled.end += read;
                }
            }
            Err(_) => {
                // Syntax error
                // TODO: maybe we can recover better than this?
                return Err(TransportError::SyntaxError(
                    String::from_utf8_lossy(&rdbuf[unhandled.clone()]).to_string(),
                ));
            }
            Ok((rem, reply)) => {
                // Got a reply
                unhandled.start = unhandled.end - rem.len();
                // TODO: when polonius is ready, we can remove this allocation by returning a
                // borrow of the input buffer (with NLL it conflicts with the mutable borrow of
                // rdbuf in the other match arm)
                return Ok(reply.into_owned());
            }
        }
    }
}

fn verify_reply(r: Reply, expected: ReplyCodeKind) -> Result<(), TransportError> {
    use EnhancedReplyCodeSubject::*;
    use ReplyCodeKind::*;
    use TransportError::*;
    match (r.code.kind(), r.ecode.as_ref().map(|e| e.subject())) {
        (k, _) if k == expected => Ok(()),
        (TransientNegative, Some(Mailbox)) => Err(TransientMailbox(r)),
        (PermanentNegative, Some(Mailbox)) => Err(PermanentMailbox(r)),
        (TransientNegative, Some(MailSystem)) => Err(TransientMailSystem(r)),
        (PermanentNegative, Some(MailSystem)) => Err(PermanentMailSystem(r)),
        (TransientNegative, _) => Err(TransientMail(r)),
        (PermanentNegative, _) => Err(PermanentMail(r)),
        (_, _) => Err(UnexpectedReplyCode(r)),
    }
}

async fn send_command<IO>(
    io: &mut IO,
    cmd: Command<&str>,
    timeout: chrono::Duration,
) -> Result<(), TransportError>
where
    IO: Unpin + Send + AsyncRead + AsyncWrite,
{
    trace!(
        cmd = String::from_utf8_lossy(
            // TODO: there _must_ be a better way to do that
            &cmd.as_io_slices()
                .flat_map(|s| s.to_vec().into_iter())
                .collect::<Vec<_>>()
        )
        .as_ref(),
        "Sending command"
    );
    smol::future::or(
        async {
            io.write_all_vectored(&mut cmd.as_io_slices().collect::<Vec<_>>())
                .await
                .map_err(TransportError::SendingCommand)?;
            Ok(())
        },
        async {
            smol::Timer::after(timeout.to_std().unwrap_or(ZERO_DURATION)).await;
            Err(TransportError::TimedOutSendingCommand)
        },
    )
    .await
}

pub struct Client<C, P, Cfg>
where
    C: trust_dns_resolver::proto::DnsHandle<Error = trust_dns_resolver::error::ResolveError>,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
    Cfg: Config,
{
    resolver: AsyncResolver<C, P>,
    cfg: Arc<Cfg>,
}

impl<C, P, Cfg> Client<C, P, Cfg>
where
    C: trust_dns_resolver::proto::DnsHandle<Error = trust_dns_resolver::error::ResolveError>,
    P: trust_dns_resolver::ConnectionProvider<Conn = C>,
    Cfg: Config,
{
    /// Note: Passing as `resolver` something that is configured with
    /// `Ipv6andIpv4` may lead to unexpected behavior, as the client will
    /// attempt to connect to both the Ipv6 and the Ipv4 address if whichever
    /// comes first doesn't successfully connect. In particular, it means that
    /// performance could be degraded.
    pub fn new(resolver: AsyncResolver<C, P>, cfg: Arc<Cfg>) -> Client<C, P, Cfg> {
        Client { resolver, cfg }
    }

    pub async fn get_destination(&self, host: &Hostname) -> Result<Destination, TransportError> {
        // TODO: already resolve here, but that means having to handle DNS expiration
        // down the road
        Ok(Destination { host: host.clone() })
    }

    pub async fn connect(&self, dest: &Destination) -> Result<Sender<Cfg>, TransportError> {
        match dest.host {
            Hostname::Ipv4 { ip, .. } => self.connect_to_ip(IpAddr::V4(ip), SMTP_PORT).await,
            Hostname::Ipv6 { ip, .. } => self.connect_to_ip(IpAddr::V6(ip), SMTP_PORT).await,
            Hostname::AsciiDomain { ref raw } => self.connect_to_mx(&raw).await,
            Hostname::Utf8Domain { ref punycode, .. } => self.connect_to_mx(&punycode).await,
        }
    }

    pub async fn connect_to_mx(&self, host: &str) -> Result<Sender<Cfg>, TransportError> {
        // TODO: consider adding a `.` at the end of `host`... but is it
        // actually allowed?
        // Run MX lookup
        let lookup = self.resolver.mx_lookup(host).await;
        let lookup = match lookup {
            Ok(l) => l,
            Err(e) => {
                if let ResolveErrorKind::NoRecordsFound { .. } = e.kind() {
                    // If there are no MX records, try A/AAAA records
                    return self
                        .connect_to_host(
                            host.into_name()
                                .map_err(|e| TransportError::HostToTrustDns(host.to_owned(), e))?,
                            SMTP_PORT,
                        )
                        .await;
                } else {
                    return Err(TransportError::DnsMx(host.to_owned(), e));
                }
            }
        };

        // Retrieve the actual records
        let mut mx_records = BTreeMap::new();
        for record in lookup.iter() {
            mx_records
                .entry(record.preference())
                .or_insert_with(|| Vec::with_capacity(1))
                .push(record.exchange());
        }

        // If there are no MX records, try A/AAAA records
        if mx_records.is_empty() {
            // TODO: is this actually required? trust_dns_resolver should return
            // NoRecordsFound anyway
            return self
                .connect_to_host(
                    host.into_name()
                        .map_err(|e| TransportError::HostToTrustDns(host.to_owned(), e))?,
                    SMTP_PORT,
                )
                .await;
        }

        // By increasing order of priority, try each MX
        // TODO: definitely should not return the first error but the first least severe
        // error
        let mut first_error = None;
        for (_, mut mxes) in mx_records {
            // Among a single priority level, randomize the order
            // TODO: consider giving a way to seed for reproducibility?
            mxes.shuffle(&mut rand::thread_rng());

            // Then try to connect to each address
            // TODO: sometimes the DNS server already returns the IP alongside the MX record
            // in the answer to the MX request, in which case we could directly
            // connect_to_ip
            for mx in mxes {
                match self.connect_to_host(mx.clone(), SMTP_PORT).await {
                    Ok(sender) => return Ok(sender),
                    Err(e) => first_error = first_error.or(Some(e)),
                }
            }
        }

        // The below unwrap is safe because, to reach it:
        // - there must be some MX records or we'd have returned in the if above
        // - there have been no error as otherwise first_error wouldn't be None
        // - there must have only be errors as otherwise we'd have returned in the match
        //   above
        // Hence, if it triggers it means that \exists N, N > 1 \wedge N = 0, where N is
        // the number of errors.
        //   QED.
        Err(first_error.unwrap())
    }

    async fn connect_to_host(
        &self,
        name: trust_dns_resolver::Name,
        port: u16,
    ) -> Result<Sender<Cfg>, TransportError> {
        // Lookup the IP addresses associated with this name
        let lookup = self
            .resolver
            .lookup_ip(name.clone())
            .await
            .map_err(|e| TransportError::DnsIp(name, e))?;

        // Following the order given by the DNS server, attempt connecting
        // TODO: definitely should not return the first error but the first least severe
        // error
        let mut first_error = None;
        for ip in lookup.iter() {
            match self.connect_to_ip(ip, port).await {
                Ok(sender) => return Ok(sender),
                Err(e) => first_error = first_error.or(Some(e)),
            }
        }

        // See comment on connect_to_mx above for why this unwrap is correct
        Err(first_error.unwrap())
    }

    pub async fn connect_to_ip(
        &self,
        ip: IpAddr,
        port: u16,
    ) -> Result<Sender<Cfg>, TransportError> {
        // TODO: introduce a connection uuid to associate log messages together
        trace!("Connecting to ip {}:{}", ip, port);
        // TODO: bind to specified outgoing IP address with net2 (first bind the builder
        // to the outgoing IP, then connect)
        let io = TcpStream::connect((ip, port))
            .await
            .map_err(|e| TransportError::Connecting(ip, port, e))?;
        let (reader, writer) = io.split();
        self.connect_to_stream(duplexify::Duplex::new(Box::pin(reader), Box::pin(writer)))
            .await
    }

    // TODO: add a connect_to_{host,ip}_smtps

    pub async fn connect_to_stream(
        &self,
        io: DynAsyncReadWrite,
    ) -> Result<Sender<Cfg>, TransportError> {
        let mut sender = Sender {
            io,
            rdbuf: [0; RDBUF_SIZE],
            unhandled: 0..0,
            extensions: Extensions::empty(),
            cfg: self.cfg.clone(),
        };
        // TODO: Are there interesting things to do with replies apart from checking
        // they're successful? Maybe logging them or something like that?

        // Read the banner
        let reply = read_reply(
            &mut sender.io,
            &mut sender.rdbuf,
            &mut sender.unhandled,
            self.cfg.banner_read_timeout(),
        )
        .await?;
        verify_reply(reply, ReplyCodeKind::PositiveCompletion)?;

        // Send EHLO
        // TODO: fallback to HELO if EHLO fails (also record somewhere that this
        // destination doesn't support HELO)
        self.send_ehlo(&mut sender).await?;

        // Send STARTTLS if possible
        let mut did_tls = false;
        if sender.extensions.contains(Extensions::STARTTLS) && self.cfg.can_do_tls() {
            // Send STARTTLS and check the reply
            send_command(
                &mut sender.io,
                Command::Starttls,
                self.cfg.command_write_timeout(),
            )
            .await?;
            let reply = read_reply(
                &mut sender.io,
                &mut sender.rdbuf,
                &mut sender.unhandled,
                self.cfg.starttls_reply_timeout(),
            )
            .await?;
            if let Ok(()) = verify_reply(reply, ReplyCodeKind::PositiveCompletion) {
                // TODO: pipelining is forbidden across starttls, check unhandled.empty()
                // Negotiate STARTTLS
                sender.io = self
                    .cfg
                    .tls_connect(sender.io)
                    .await
                    .map_err(TransportError::NegotiatingTls)?;
                // TODO: in case this call fails, maybe log? also, if
                // we have must_do_tls, this server should probably be
                // removed from the retry list as no matching ciphers
                // is probably a permanent error.
                //
                // TODO: Retry without TLS enabled! Currently servers that support starttls but
                // only with ancient ciphers are unreachable
                //
                // TODO: Split out the error condition “network error” from “negotiation failed”
                // so as to know whether we should try STARTTLS again next time

                // Send EHLO again
                self.send_ehlo(&mut sender).await?;
                did_tls = true;
            } else {
                // Server failed to accept STARTTLS. Let's fall through and
                // continue without it (unless must_do_tls is enabled)
                // TODO: maybe log? also, if we have must_do_tls and this
                // returns a permanent error we definitely should bounce
            }
        }
        if !did_tls && self.cfg.must_do_tls() {
            return Err(TransportError::CannotDoTls);
        }

        // TODO: AUTH

        Ok(sender)
    }

    async fn send_ehlo(&self, sender: &mut Sender<Cfg>) -> Result<(), TransportError> {
        send_command(
            &mut sender.io,
            Command::Ehlo {
                hostname: self.cfg.ehlo_hostname().to_ref(),
            },
            self.cfg.command_write_timeout(),
        )
        .await?;

        // Parse the reply and verify it
        let reply = read_reply(
            &mut sender.io,
            &mut sender.rdbuf,
            &mut sender.unhandled,
            self.cfg.ehlo_reply_timeout(),
        )
        .await?;
        sender.extensions = Extensions::empty();
        for line in reply.text.iter() {
            // TODO: parse other extensions that may be of interest (eg. pipelining)
            if line.as_str().eq_ignore_ascii_case("STARTTLS") {
                sender.extensions.insert(Extensions::STARTTLS);
            }
        }
        verify_reply(reply, ReplyCodeKind::PositiveCompletion)?;

        Ok(())
    }
}

bitflags! {
    struct Extensions: u8 {
        const STARTTLS = 0b1;
    }
}

pub struct Sender<Cfg> {
    io: DynAsyncReadWrite,
    rdbuf: [u8; RDBUF_SIZE],
    unhandled: Range<usize>,
    extensions: Extensions,
    cfg: Arc<Cfg>,
}

impl<Cfg> Sender<Cfg>
where
    Cfg: Config,
{
    // TODO: Figure out a way to batch a single mail (with the same metadata) going
    // out to multiple recipients, so as to just use multiple RCPT TO
    /// Note: `mail` must be a reader of the *already escaped and
    /// CRLF-dot-CRLF-terminated* message! If this is not the format
    /// you have, please looking into the `smtp-message` crate's
    /// utilities.
    pub async fn send<Reader>(
        &mut self,
        from: Option<&Email>,
        to: &Email,
        mail: Reader,
    ) -> Result<(), TransportError>
    where
        Reader: AsyncRead,
    {
        macro_rules! send_command {
            ($cmd:expr) => {
                send_command(&mut self.io, $cmd, self.cfg.command_write_timeout())
            };
        }
        macro_rules! read_reply {
            ($expected:expr, $timeout:expr) => {
                async {
                    let reply =
                        read_reply(&mut self.io, &mut self.rdbuf, &mut self.unhandled, $timeout)
                            .await?;
                    verify_reply(reply, $expected)
                }
            };
        }

        // MAIL FROM
        send_command!(Command::Mail {
            path: None,
            email: from.map(|f| f.to_ref()),
            params: Parameters(Vec::new()),
        })
        .await?;
        read_reply!(
            ReplyCodeKind::PositiveCompletion,
            self.cfg.mail_reply_timeout()
        )
        .await?;

        // RCPT TO
        send_command!(Command::Rcpt {
            path: None,
            email: to.to_ref(),
            params: Parameters(Vec::new()),
        })
        .await?;
        read_reply!(
            ReplyCodeKind::PositiveCompletion,
            self.cfg.rcpt_reply_timeout()
        )
        .await?;

        // DATA
        send_command!(Command::Data).await?;
        read_reply!(
            ReplyCodeKind::PositiveIntermediate,
            self.cfg.data_init_reply_timeout()
        )
        .await?;

        // Send the contents of the email
        {
            pin_mut!(mail);
            let cfg = self.cfg.clone();
            let mut databuf = [0; DATABUF_SIZE];
            loop {
                match mail.read(&mut databuf).await {
                    Ok(0) => {
                        // End of stream
                        break;
                    }
                    Ok(n) => {
                        // Got n bytes, try sending with a timeout
                        smol::future::or(
                            async {
                                self.io
                                    .write_all(&databuf[..n])
                                    .await
                                    .map_err(TransportError::SendingData)
                            },
                            async {
                                smol::Timer::after(
                                    cfg.data_block_write_timeout()
                                        .to_std()
                                        .unwrap_or(ZERO_DURATION),
                                )
                                .await;
                                Err(TransportError::TimedOutSendingData)
                            },
                        )
                        .await?;
                    }
                    Err(e) => return Err(TransportError::ReadingMail(e)),
                }
            }
        }

        // Wait for a reply
        read_reply!(
            ReplyCodeKind::PositiveCompletion,
            self.cfg.data_end_reply_timeout()
        )
        .await?;

        Ok(())
    }
}

// TODO: is it important to call QUIT before closing the TCP stream?

// TODO: add tests
