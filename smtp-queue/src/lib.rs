use std::{
    hash::Hash,
    io::IoSlice,
    marker::PhantomData,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{io, join, pin_mut, AsyncRead, AsyncWrite, Stream, StreamExt, TryFutureExt};
use smtp_message::Email;

// TODO:
//  - Record SendFailLevel (Server/Mailbox/Email)
//  - Have one queue per server instead of per email, trying to eliminate all
//    emails at once by using SendFailLevel to identify whether it makes sense
//    to continue sending email to this mailbox/server
//  - Decouple the concept of sub-queue (which is the thing by which emails get
//    rescheduled and batched to the transport) from the destination server
//    (because eg. it happens that we want all email delivered to the local
//    antivirus regardless of the destination hostname)

// Use cases to take into account:
//  * By mistake, multiple instances have been started with the same queue
//    directory
//  * The user wants to modify by hand data in the queue for some reason, it's
//    better not to have to shut down the server in order to do that (esp. as
//    they may forget to do it). But it's OK to require them to notify the
//    server after having done that
// Idea:
//  * Before sending mails, move them to an in-progress directory so that
//    multiple simultaneously-running instances don't send the same mail at the
//    same time.
//  * If there is a crash, a mail may be stuck in this in-progress directory.
//    So, at startup:
//     * Also scan the in-progress directory.
//     * If there is a mail there, it *could* be in the process of being sent,
//       so wait long enough (1 hour?) to be sure all timeouts are passed, and
//       check if it is still there.
//     * If it is still there, then it means that it was left here after a crash
//       while sending it, as the name in the in-progress directory is randomly
//       picked (so even if it was actually in-progress and had been
//       re-scheduled and put back in the in-progress directory, it would have a
//       new name).

#[derive(serde::Deserialize, serde::Serialize)]
pub struct MailMetadata<U> {
    pub from: Option<Email>,
    pub to: Email,
    pub metadata: U,
}

#[derive(Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct ScheduleInfo {
    pub at: DateTime<Utc>,
    pub last_attempt: Option<DateTime<Utc>>,
}

impl ScheduleInfo {
    pub fn last_interval(&self) -> Result<Option<Duration>, time::OutOfRangeError> {
        self.last_attempt
            .map(|last| (last - self.at).to_std())
            .transpose()
    }
}

#[derive(Clone, Debug)]
pub struct QueueId(pub Arc<String>);

impl QueueId {
    pub fn new<S: ToString>(s: S) -> QueueId {
        QueueId(Arc::new(s.to_string()))
    }
}

#[async_trait]
pub trait Config<U, StorageError>: 'static + Send + Sync {
    // Returning None means dropping the email from the queue. If it does so, this
    // function probably should bounce!
    async fn next_interval(&self, s: ScheduleInfo) -> Option<Duration>;

    async fn log_storage_error(&self, err: StorageError, id: Option<QueueId>);
    async fn log_found_inflight(&self, inflight: QueueId);
    async fn log_found_pending_cleanup(&self, pcm: QueueId);
    async fn log_queued_mail_vanished(&self, id: QueueId);
    async fn log_inflight_mail_vanished(&self, id: QueueId);
    async fn log_pending_cleanup_mail_vanished(&self, id: QueueId);
    async fn log_too_big_duration(&self, id: QueueId, too_big: Duration, new: Duration);

    // The important thing is it must be longer than the time between
    // switching a mail to inflight and either completing it or
    // returning it to the queue
    fn found_inflight_check_delay(&self) -> Duration {
        Duration::from_secs(3600)
    }

    fn io_error_next_retry_delay(&self, d: Duration) -> Duration {
        if d < Duration::from_secs(30) {
            Duration::from_secs(60)
        } else {
            d.mul_f64(2.0)
        }
    }
}

#[async_trait]
pub trait Storage<U>: 'static + Send + Sync {
    type Error: Send + std::error::Error;

    type QueuedMail: QueuedMail;
    type InflightMail: InflightMail;
    type PendingCleanupMail: PendingCleanupMail;

    type QueueLister: Send + Stream<Item = Result<Self::QueuedMail, (Self::Error, Option<QueueId>)>>;
    type InflightLister: Send
        + Stream<Item = Result<Self::InflightMail, (Self::Error, Option<QueueId>)>>;
    type PendingCleanupLister: Send
        + Stream<Item = Result<Self::PendingCleanupMail, (Self::Error, Option<QueueId>)>>;

    type Enqueuer: StorageEnqueuer<U, Self, Self::QueuedMail>;
    type Reader: Send + AsyncRead;

    async fn list_queue(&self) -> Self::QueueLister;
    async fn find_inflight(&self) -> Self::InflightLister;
    async fn find_pending_cleanup(&self) -> Self::PendingCleanupLister;

    async fn read_inflight(
        &self,
        mail: &Self::InflightMail,
    ) -> Result<(MailMetadata<U>, Self::Reader), Self::Error>;

    async fn enqueue(&self) -> Result<Self::Enqueuer, Self::Error>;

    async fn reschedule(
        &self,
        mail: &mut Self::QueuedMail,
        schedule: ScheduleInfo,
    ) -> Result<(), Self::Error>;

    async fn send_start(
        &self,
        mail: Self::QueuedMail,
    ) -> Result<Option<Self::InflightMail>, (Self::QueuedMail, Self::Error)>;

    async fn send_done(
        &self,
        mail: Self::InflightMail,
    ) -> Result<Option<Self::PendingCleanupMail>, (Self::InflightMail, Self::Error)>;

    async fn send_cancel(
        &self,
        mail: Self::InflightMail,
    ) -> Result<Option<Self::QueuedMail>, (Self::InflightMail, Self::Error)>;

    async fn drop(
        &self,
        mail: Self::QueuedMail,
    ) -> Result<Option<Self::PendingCleanupMail>, (Self::QueuedMail, Self::Error)>;

    // Returns true if the mail was successfully cleaned up, and false
    // if the mail somehow vanished during the cleanup operation or
    // was already cleaned up. Returns an error if the mail could not
    // be cleaned up.
    async fn cleanup(
        &self,
        mail: Self::PendingCleanupMail,
    ) -> Result<bool, (Self::PendingCleanupMail, Self::Error)>;
}

pub trait QueuedMail: Send + Sync {
    fn id(&self) -> QueueId;
    fn schedule(&self) -> ScheduleInfo;
}

pub trait InflightMail: Send + Sync {
    fn id(&self) -> QueueId;
}

pub trait PendingCleanupMail: Send + Sync {
    fn id(&self) -> QueueId;
}

#[async_trait]
pub trait StorageEnqueuer<U, S, QueuedMail>: Send + AsyncWrite
where
    S: ?Sized + Storage<U>,
{
    async fn commit(
        self,
        destinations: Vec<(MailMetadata<U>, ScheduleInfo)>,
    ) -> Result<Vec<QueuedMail>, S::Error>;
}

pub enum TransportFailure {
    Local,
    NetworkTransient,
    MailTransient,
    MailboxTransient,
    MailSystemTransient,
    MailPermanent,
    MailboxPermanent,
    MailSystemPermanent,
}

#[async_trait]
pub trait Transport<U>: 'static + Send + Sync {
    type Destination: Send + Sync + Eq + Hash;
    type Sender: TransportSender<U>;

    async fn destination(
        &self,
        meta: &MailMetadata<U>,
    ) -> Result<Self::Destination, TransportFailure>;

    async fn connect(&self, dest: &Self::Destination) -> Result<Self::Sender, TransportFailure>;
}

#[async_trait]
pub trait TransportSender<U>: 'static + Send {
    // TODO: Figure out a way to batch a single mail (with the same metadata) going
    // out to multiple recipients, so as to just use multiple RCPT TO
    async fn send<Reader>(
        &mut self,
        meta: &MailMetadata<U>,
        mail: Reader,
    ) -> Result<(), TransportFailure>
    where
        Reader: Send + AsyncRead;
}

// Interval used when the duration doesn't match (ie. only in error conditions)
const INTERVAL_ON_TOO_BIG_DURATION: Duration = Duration::from_secs(4 * 3600);

struct QueueImpl<C, S, T> {
    executor: Arc<smol::Executor<'static>>,
    config: C,
    storage: S,
    transport: T,
}

pub struct Queue<U, C, S, T> {
    q: Arc<QueueImpl<C, S, T>>,
    phantom: PhantomData<U>,
}

macro_rules! io_retry_loop {
    ($this:ident, $init:expr, | $mail:ident | $e:expr) => {{
        let mut delay = Duration::from_secs(0);
        let mut $mail = $init;
        loop {
            match $e {
                Ok(v) => {
                    break v;
                }
                Err((mail, e)) => {
                    $this.q.config.log_storage_error(e, Some(mail.id())).await;
                    $mail = mail;
                }
            }
            smol::Timer::after(delay).await;
            delay = $this.q.config.io_error_next_retry_delay(delay);
        }
    }};
}

macro_rules! io_retry_loop_raw {
    ($this:ident, $id:expr, $e:expr) => {{
        let mut delay = Duration::from_secs(0);
        loop {
            match $e {
                Ok(v) => {
                    break v;
                }
                Err(e) => {
                    $this.q.config.log_storage_error(e, Some($id)).await;
                }
            }
            smol::Timer::after(delay).await;
            delay = $this.q.config.io_error_next_retry_delay(delay);
        }
    }};
}

impl<U, C, S, T> Queue<U, C, S, T>
where
    U: 'static + Send + Sync,
    C: Config<U, S::Error>,
    S: Storage<U>,
    T: Transport<U>,
{
    async fn cleanup(&self, pcm: S::PendingCleanupMail) {
        let id = pcm.id();
        let cleanup_successful = io_retry_loop!(self, pcm, |p| self.q.storage.cleanup(p).await);
        if !cleanup_successful {
            self.q.config.log_pending_cleanup_mail_vanished(id).await;
        }
    }
}

impl<U, C, S, T> Queue<U, C, S, T>
where
    U: 'static + Send + Sync,
    C: Config<U, S::Error>,
    S: Storage<U>,
    T: Transport<U>,
{
    pub async fn new(
        executor: Arc<smol::Executor<'static>>,
        config: C,
        storage: S,
        transport: T,
    ) -> Queue<U, C, S, T> {
        let this = Queue {
            q: Arc::new(QueueImpl {
                executor,
                config,
                storage,
                transport,
            }),
            phantom: PhantomData,
        };

        join!(this.scan_inflight(), this.scan_pending_cleanup());

        let this2 = this.clone();
        this.q
            .executor
            .spawn(async move { this2.scan_queue().await })
            .detach();

        this
    }

    pub async fn enqueue(&self) -> Result<Enqueuer<U, C, S, T>, S::Error> {
        Ok(Enqueuer {
            queue: self.clone(),
            enqueuer: Some(self.q.storage.enqueue().await?),
        })
    }

    async fn scan_inflight(&self) {
        let found_inflight_stream = self.q.storage.find_inflight().await;
        pin_mut!(found_inflight_stream);
        while let Some(inflight) = found_inflight_stream.next().await {
            match inflight {
                Err((e, id)) => self.q.config.log_storage_error(e, id).await,
                Ok(inflight) => {
                    self.q.config.log_found_inflight(inflight.id()).await;
                    let this = self.clone();
                    self.q
                        .executor
                        .spawn(async move {
                            smol::Timer::after(this.q.config.found_inflight_check_delay()).await;
                            let queued = io_retry_loop!(this, inflight, |i| this
                                .q
                                .storage
                                .send_cancel(i)
                                .await);
                            if let Some(queued) = queued {
                                // Mail is still waiting, probably was
                                // inflight during a crash
                                this.send(queued).await
                            } else {
                                // Mail is no longer waiting, probably
                                // was inflight because another
                                // process was currently sending it
                            }
                        })
                        .detach();
                }
            }
        }
    }

    async fn scan_queue(&self) {
        let queued_stream = self.q.storage.list_queue().await;
        pin_mut!(queued_stream);
        while let Some(queued) = queued_stream.next().await {
            match queued {
                Err((e, id)) => self.q.config.log_storage_error(e, id).await,
                Ok(queued) => {
                    let this = self.clone();
                    self.q
                        .executor
                        .spawn(async move {
                            this.send(queued).await;
                        })
                        .detach();
                }
            }
        }
    }

    async fn scan_pending_cleanup(&self) {
        let pcm_stream = self.q.storage.find_pending_cleanup().await;
        pin_mut!(pcm_stream);
        while let Some(pcm) = pcm_stream.next().await {
            match pcm {
                Err((e, id)) => self.q.config.log_storage_error(e, id).await,
                Ok(pcm) => {
                    self.q.config.log_found_pending_cleanup(pcm.id()).await;
                    let this = self.clone();
                    self.q
                        .executor
                        .spawn(async move {
                            this.cleanup(pcm).await;
                        })
                        .detach();
                }
            }
        }
    }

    async fn send(&self, mail: S::QueuedMail) {
        let mut mail = mail;
        loop {
            // TODO: this should be smol::Timer::at, but I can't find how to convert from
            // chrono::DateTime<Utc> to std::time::Instant right now
            const ZERO_DURATION: Duration = Duration::from_secs(0);
            let wait_time = (mail.schedule().at - Utc::now())
                .to_std()
                .unwrap_or(ZERO_DURATION);
            smol::Timer::after(wait_time).await;
            match self.try_send(mail).await {
                Ok(()) => return,
                Err(m) => mail = m,
            }
            let this_attempt = Utc::now();
            match self.q.config.next_interval(mail.schedule()).await {
                Some(next_interval) => {
                    let next_interval = match chrono::Duration::from_std(next_interval) {
                        Ok(i) => i,
                        Err(_) => {
                            let new_next_interval = INTERVAL_ON_TOO_BIG_DURATION;
                            self.q
                                .config
                                .log_too_big_duration(mail.id(), next_interval, new_next_interval)
                                .await;
                            chrono::Duration::from_std(new_next_interval).unwrap()
                        }
                    };
                    let next_attempt = this_attempt + next_interval;
                    let schedule = ScheduleInfo {
                        at: next_attempt,
                        last_attempt: Some(this_attempt),
                    };
                    io_retry_loop_raw!(
                        self,
                        mail.id(),
                        self.q.storage.reschedule(&mut mail, schedule).await
                    );
                }
                None => {
                    let id = mail.id();
                    let pcm = io_retry_loop!(self, mail, |m| self.q.storage.drop(m).await);
                    let pcm = match pcm {
                        Some(pcm) => pcm,
                        None => {
                            self.q.config.log_queued_mail_vanished(id).await;
                            return;
                        }
                    };

                    self.cleanup(pcm).await;
                    return;
                }
            }
        }
    }

    async fn try_send(&self, mail: S::QueuedMail) -> Result<(), S::QueuedMail> {
        let id = mail.id();
        let inflight = io_retry_loop!(self, mail, |m| self.q.storage.send_start(m).await);
        let inflight = match inflight {
            Some(inflight) => inflight,
            None => {
                self.q.config.log_queued_mail_vanished(id).await;
                return Ok(());
            }
        };

        let (inflight, meta, reader) = io_retry_loop!(self, inflight, |i| match self
            .q
            .storage
            .read_inflight(&i)
            .await
        {
            Ok((m, r)) => Ok((i, m, r)),
            Err(e) => Err((i, e)),
        });

        // TODO: connect only once for all mails towards a single destination
        // Note that this will probably mean having to refactor smtp-client, as
        // Destination currently does not remember for how long the DNS reply was valid
        // Also, we will have to consider how to properly handle the case here multiple
        // hostnames have the same top-prio MX IP but not the same lower-prio MX IPs
        let meta_ref = &meta;
        let send_attempt = self
            .q
            .transport
            .destination(&meta)
            .and_then(|dest| async move { self.q.transport.connect(&dest).await })
            .and_then(|mut sender| async move { sender.send(meta_ref, reader).await })
            .await;

        match send_attempt {
            Ok(()) => {
                let pcm = io_retry_loop!(self, inflight, |i| self.q.storage.send_done(i).await);
                match pcm {
                    Some(pcm) => {
                        self.cleanup(pcm).await;
                    }
                    None => {
                        self.q.config.log_queued_mail_vanished(id).await;
                    }
                };
                return Ok(());
            }
            Err(_e) => {
                // TODO: actually make a distinction between all the cases, and
                // retry iff required and not even if getting a permanent error
            }
        }
        // The above match falls through only in cases where we ought to retry
        let id = inflight.id();
        let queued = io_retry_loop!(self, inflight, |i| self.q.storage.send_cancel(i).await);
        match queued {
            Some(queued) => Err(queued),
            None => {
                self.q.config.log_inflight_mail_vanished(id).await;
                Ok(())
            }
        }
    }
}

// This cannot be a #[derive] due to the absence of bounds on U,C,S,T
impl<U, C, S, T> Clone for Queue<U, C, S, T> {
    fn clone(&self) -> Self {
        Self {
            q: self.q.clone(),
            phantom: self.phantom,
        }
    }
}

pub struct Enqueuer<U, C, S, T>
where
    S: Storage<U>,
{
    queue: Queue<U, C, S, T>,
    enqueuer: Option<S::Enqueuer>,
}

impl<U, C, S, T> Enqueuer<U, C, S, T>
where
    U: 'static + Send + Sync,
    C: Config<U, S::Error>,
    S: Storage<U>,
    T: Transport<U>,
{
    pub async fn commit(
        self,
        destinations: Vec<(MailMetadata<U>, ScheduleInfo)>,
    ) -> Result<(), S::Error> {
        let mut this = self;
        let mails = this.enqueuer.take().unwrap().commit(destinations).await?;
        for mail in mails {
            let q = this.queue.clone();
            this.queue
                .q
                .executor
                .spawn(async move { q.send(mail).await })
                .detach();
        }
        Ok(())
    }
}

impl<U, C, S, T> AsyncWrite for Enqueuer<U, C, S, T>
where
    U: 'static + Send + Sync,
    C: Config<U, S::Error>,
    S: Storage<U>,
    T: Transport<U>,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        unsafe {
            self.map_unchecked_mut(|s| {
                s.enqueuer
                    .as_mut()
                    .expect("Tried writing to enqueuer after having committed it")
            })
        }
        .poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unsafe {
            self.map_unchecked_mut(|s| {
                s.enqueuer
                    .as_mut()
                    .expect("Tried writing to enqueuer after having committed it")
            })
        }
        .poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unsafe {
            self.map_unchecked_mut(|s| {
                s.enqueuer
                    .as_mut()
                    .expect("Tried writing to enqueuer after having committed it")
            })
        }
        .poll_close(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[IoSlice],
    ) -> Poll<io::Result<usize>> {
        unsafe {
            self.map_unchecked_mut(|s| {
                s.enqueuer
                    .as_mut()
                    .expect("Tried writing to enqueuer after having committed it")
            })
        }
        .poll_write_vectored(cx, bufs)
    }
}
