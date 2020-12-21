use std::{
    io,
    marker::PhantomData,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures::{io::IoSlice, prelude::*};
use openat::Dir;
use smol::unblock;
use smtp_queue::{MailMetadata, QueueId, ScheduleInfo};
use uuid::Uuid;
use walkdir::WalkDir;

pub const DATA_DIR: &str = "data";
pub const QUEUE_DIR: &str = "queue";
pub const INFLIGHT_DIR: &str = "inflight";
pub const CLEANUP_DIR: &str = "cleanup";

pub const DATA_DIR_FROM_OTHER_QUEUE: &str = "../data";

pub const CONTENTS_FILE: &str = "contents";
pub const METADATA_FILE: &str = "metadata";
pub const SCHEDULE_FILE: &str = "schedule";
pub const TMP_METADATA_FILE_PREFIX: &str = "metadata.";
pub const TMP_SCHEDULE_FILE_PREFIX: &str = "schedule.";

const ONLY_USER_RW: u32 = 0o600;

// TODO: auto-detect orphan files (pointed to by nowhere in the queue)

pub struct FsStorage<U> {
    path: Arc<PathBuf>,
    data: Arc<Dir>,
    queue: Arc<Dir>,
    inflight: Arc<Dir>,
    cleanup: Arc<Dir>,
    phantom: PhantomData<U>,
}

impl<U> FsStorage<U> {
    pub async fn new(path: Arc<PathBuf>) -> io::Result<FsStorage<U>> {
        let main_dir = {
            let path = path.clone();
            Arc::new(unblock(move || Dir::open(&*path)).await?)
        };
        let data = {
            let main_dir = main_dir.clone();
            Arc::new(unblock(move || main_dir.sub_dir(DATA_DIR)).await?)
        };
        let queue = {
            let main_dir = main_dir.clone();
            Arc::new(unblock(move || main_dir.sub_dir(QUEUE_DIR)).await?)
        };
        let inflight = {
            let main_dir = main_dir.clone();
            Arc::new(unblock(move || main_dir.sub_dir(INFLIGHT_DIR)).await?)
        };
        let cleanup = {
            let main_dir = main_dir.clone();
            Arc::new(unblock(move || main_dir.sub_dir(CLEANUP_DIR)).await?)
        };
        Ok(FsStorage {
            path,
            data,
            queue,
            inflight,
            cleanup,
            phantom: PhantomData,
        })
    }
}

type DynStreamOf<T> = Pin<Box<dyn Send + Stream<Item = T>>>;

#[async_trait]
impl<U> smtp_queue::Storage<U> for FsStorage<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    type Enqueuer = FsEnqueuer<U>;
    type InflightLister = DynStreamOf<Result<FsInflightMail, (io::Error, Option<QueueId>)>>;
    type InflightMail = FsInflightMail;
    type PendingCleanupLister =
        DynStreamOf<Result<FsPendingCleanupMail, (io::Error, Option<QueueId>)>>;
    type PendingCleanupMail = FsPendingCleanupMail;
    type QueueLister = DynStreamOf<Result<FsQueuedMail, (io::Error, Option<QueueId>)>>;
    type QueuedMail = FsQueuedMail;
    type Reader = Pin<Box<dyn Send + AsyncRead>>;

    async fn list_queue(
        &self,
    ) -> Pin<Box<dyn Send + Stream<Item = Result<FsQueuedMail, (io::Error, Option<QueueId>)>>>>
    {
        Box::pin(
            scan_queue(self.path.join(QUEUE_DIR), self.queue.clone())
                .await
                .map(|r| r.map(FsQueuedMail::found)),
        )
    }

    async fn find_inflight(
        &self,
    ) -> Pin<Box<dyn Send + Stream<Item = Result<FsInflightMail, (io::Error, Option<QueueId>)>>>>
    {
        Box::pin(
            scan_queue(self.path.join(INFLIGHT_DIR), self.inflight.clone())
                .await
                .map(|r| r.map(FsInflightMail::found)),
        )
    }

    async fn find_pending_cleanup(
        &self,
    ) -> Pin<
        Box<dyn Send + Stream<Item = Result<FsPendingCleanupMail, (io::Error, Option<QueueId>)>>>,
    > {
        Box::pin(
            scan_folder(self.path.join(CLEANUP_DIR))
                .await
                .map(|r| r.map(FsPendingCleanupMail::found)),
        )
    }

    async fn read_inflight(
        &self,
        mail: &FsInflightMail,
    ) -> Result<(MailMetadata<U>, Self::Reader), io::Error> {
        let inflight = self.inflight.clone();
        let mail = mail.id.0.clone();

        unblock(move || {
            let dest_dir = inflight.sub_dir(&*mail)?;
            let metadata = dest_dir
                .open_file(METADATA_FILE)
                .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))?;
            let contents_file = dest_dir.sub_dir("..")?.open_file(CONTENTS_FILE)?;
            let reader = Box::pin(smol::Async::new(contents_file)?) as _;
            Ok((metadata, reader))
        })
        .await
    }

    async fn enqueue(&self) -> io::Result<FsEnqueuer<U>> {
        let data = self.data.clone();
        let queue = self.queue.clone();

        unblock(move || {
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let mail_uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);

            data.create_dir(&*mail_uuid, ONLY_USER_RW)?;
            let mail_dir = data.sub_dir(&*mail_uuid)?;
            let contents_file = mail_dir.new_file(CONTENTS_FILE, ONLY_USER_RW)?;

            Ok(FsEnqueuer {
                mail_uuid: mail_uuid.to_string(),
                mail_dir,
                queue,
                writer: Box::pin(smol::Async::new(contents_file)?),
                phantom: PhantomData,
            })
        })
        .await
    }

    // TODO: make reschedule only ever happen on the inflight mails, like remeta
    async fn reschedule(&self, mail: &mut FsQueuedMail, schedule: ScheduleInfo) -> io::Result<()> {
        mail.schedule = schedule;

        let queue = self.queue.clone();
        let id = mail.id.0.clone();

        unblock(move || {
            let dest_dir = queue.sub_dir(&*id)?;

            let mut tmp_sched_file = String::from(TMP_SCHEDULE_FILE_PREFIX);
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            tmp_sched_file.push_str(uuid);

            let tmp_file = dest_dir.new_file(&tmp_sched_file, ONLY_USER_RW)?;
            serde_json::to_writer(tmp_file, &schedule)?;

            dest_dir.local_rename(&tmp_sched_file, SCHEDULE_FILE)?;

            Ok::<_, io::Error>(())
        })
        .await?;
        Ok(())
    }

    async fn send_start(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsInflightMail>, (FsQueuedMail, io::Error)> {
        let queue = self.queue.clone();
        let inflight = self.inflight.clone();
        unblock(
            move || match openat::rename(&*queue, &*mail.id.0, &*inflight, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_inflight())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            },
        )
        .await
    }

    async fn send_done(
        &self,
        mail: FsInflightMail,
    ) -> Result<Option<FsPendingCleanupMail>, (FsInflightMail, io::Error)> {
        let inflight = self.inflight.clone();
        let cleanup = self.cleanup.clone();
        unblock(
            move || match openat::rename(&*inflight, &*mail.id.0, &*cleanup, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_pending_cleanup())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            },
        )
        .await
    }

    async fn send_cancel(
        &self,
        mail: FsInflightMail,
    ) -> Result<Option<FsQueuedMail>, (FsInflightMail, io::Error)> {
        let inflight = self.inflight.clone();
        let queue = self.queue.clone();
        unblock(
            move || match openat::rename(&*inflight, &*mail.id.0, &*queue, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_queued())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            },
        )
        .await
    }

    async fn drop(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsPendingCleanupMail>, (FsQueuedMail, io::Error)> {
        let queue = self.queue.clone();
        let cleanup = self.cleanup.clone();
        unblock(
            move || match openat::rename(&*queue, &*mail.id.0, &*cleanup, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_pending_cleanup())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            },
        )
        .await
    }

    async fn cleanup(
        &self,
        mail: FsPendingCleanupMail,
    ) -> Result<bool, (FsPendingCleanupMail, io::Error)> {
        let cleanup = self.cleanup.clone();
        let data = self.data.clone();
        unblock(move || {
            let dest = match cleanup.read_link(&*mail.id.0) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err((mail, e)),
                Ok(d) => d,
            };

            let dest_dir = match cleanup.sub_dir(&*mail.id.0) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err((mail, e)),
                Ok(d) => d,
            };

            match dest_dir.remove_file(METADATA_FILE) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                _ => (),
            }

            match dest_dir.remove_file(SCHEDULE_FILE) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                _ => (),
            }

            let mail_dir = match dest_dir.sub_dir("..") {
                Ok(d) => d,
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                Err(_) => return Ok(false),
            };

            let dest_name = match dest.file_name() {
                Some(d) => d,
                None => {
                    return Err((
                        mail,
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "could not extract destination uuid from message symlink",
                        ),
                    ));
                }
            };

            match mail_dir.remove_dir(dest_name) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                _ => (),
            }

            // rm mail_dir iff the only remaining file is CONTENTS_FILE
            // `mut` is required here because list_self() returns an Iterator
            let mut mail_dir_list = match mail_dir.list_self() {
                Ok(l) => l,
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                Err(_) => return Ok(false),
            };
            let should_rm_mail_dir =
                mail_dir_list.all(|e| matches!(e, Ok(e) if e.file_name() == CONTENTS_FILE));

            if should_rm_mail_dir {
                match mail_dir.remove_file(CONTENTS_FILE) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                    _ => (),
                }

                let mail_name = match dest.parent().and_then(|p| p.file_name()) {
                    Some(m) => m,
                    None => {
                        return Err((
                            mail,
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "could not extract mail uuid from message symlink",
                            ),
                        ));
                    }
                };

                match data.remove_dir(mail_name) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                    _ => (),
                }
            }

            match cleanup.remove_file(&*mail.id.0) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err((mail, e)),
            }
        })
        .await
    }
}

struct FoundMail {
    id: QueueId,
    schedule: ScheduleInfo,
}

async fn scan_folder<P>(
    path: P,
) -> impl 'static + Send + Stream<Item = Result<QueueId, (io::Error, Option<QueueId>)>>
where
    P: 'static + Send + AsRef<Path>,
{
    let it = unblock(move || WalkDir::new(path).into_iter()).await;
    smol::stream::iter(it)
        .then(move |p| async move {
            let p = p.map_err(|e| (io::Error::from(e), None))?;
            if !p.path_is_symlink() {
                Ok(None)
            } else {
                let path = p.path().to_str().ok_or((
                    io::Error::new(io::ErrorKind::InvalidData, "file path is not utf-8"),
                    None,
                ))?;
                Ok(Some(QueueId::new(path)))
            }
        })
        .filter_map(|r| async move { r.transpose() })
}

async fn scan_queue<P>(
    path: P,
    dir: Arc<Dir>,
) -> impl 'static + Send + Stream<Item = Result<FoundMail, (io::Error, Option<QueueId>)>>
where
    P: 'static + Send + AsRef<Path>,
{
    scan_folder(path).await.then(move |id| {
        let dir = dir.clone();
        async move {
            let id = id?;
            let schedule_path = Path::new(&*id.0).join(SCHEDULE_FILE);
            let schedule = unblock(move || {
                dir.open_file(&schedule_path)
                    .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
            })
            .await
            .map_err(|e| (e, Some(id.clone())))?;
            Ok(FoundMail { id, schedule })
        }
    })
}

pub struct FsQueuedMail {
    id: QueueId,
    schedule: ScheduleInfo,
}

impl FsQueuedMail {
    fn found(f: FoundMail) -> FsQueuedMail {
        FsQueuedMail {
            id: f.id,
            schedule: f.schedule,
        }
    }

    fn into_inflight(self) -> FsInflightMail {
        FsInflightMail {
            id: self.id,
            schedule: self.schedule,
        }
    }

    fn into_pending_cleanup(self) -> FsPendingCleanupMail {
        FsPendingCleanupMail { id: self.id }
    }
}

impl smtp_queue::QueuedMail for FsQueuedMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }

    fn schedule(&self) -> ScheduleInfo {
        self.schedule
    }
}

pub struct FsInflightMail {
    id: QueueId,
    schedule: ScheduleInfo,
}

impl FsInflightMail {
    fn found(f: FoundMail) -> FsInflightMail {
        FsInflightMail {
            id: f.id,
            schedule: f.schedule,
        }
    }

    fn into_queued(self) -> FsQueuedMail {
        FsQueuedMail {
            id: self.id,
            schedule: self.schedule,
        }
    }

    fn into_pending_cleanup(self) -> FsPendingCleanupMail {
        FsPendingCleanupMail { id: self.id }
    }
}

impl smtp_queue::InflightMail for FsInflightMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }
}

pub struct FsPendingCleanupMail {
    id: QueueId,
}

impl FsPendingCleanupMail {
    fn found(id: QueueId) -> FsPendingCleanupMail {
        FsPendingCleanupMail { id }
    }
}

impl smtp_queue::PendingCleanupMail for FsPendingCleanupMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }
}

pub struct FsEnqueuer<U> {
    mail_uuid: String,
    mail_dir: Dir,
    queue: Arc<Dir>,
    writer: Pin<Box<dyn 'static + Send + AsyncWrite>>,
    // FsEnqueuer needs the U type parameter just so as to be able to take it as a parameter later
    // on
    phantom: PhantomData<fn(U)>,
}

/// Blocking function!
fn make_dest_dir<U>(
    queue: &Dir,
    mail_uuid: &str,
    mail_dir: &Dir,
    dest_id: &str,
    metadata: &MailMetadata<U>,
    schedule: &ScheduleInfo,
) -> io::Result<FsQueuedMail>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    // TODO: clean up self dest dir when having an io error
    mail_dir.create_dir(dest_id, ONLY_USER_RW)?;
    let dest_dir = mail_dir.sub_dir(dest_id)?;

    let schedule_file = dest_dir.new_file(SCHEDULE_FILE, ONLY_USER_RW)?;
    serde_json::to_writer(schedule_file, &schedule)?;

    let metadata_file = dest_dir.new_file(METADATA_FILE, ONLY_USER_RW)?;
    serde_json::to_writer(metadata_file, &metadata)?;

    let mut dest_uuid_buf: [u8; 45] = Uuid::encode_buffer();
    let dest_uuid = Uuid::new_v4()
        .to_hyphenated_ref()
        .encode_lower(&mut dest_uuid_buf);

    let mut symlink_value = PathBuf::from(DATA_DIR_FROM_OTHER_QUEUE);
    symlink_value.push(&mail_uuid);
    symlink_value.push(dest_id);
    queue.symlink(&*dest_uuid, &symlink_value)?;

    Ok(FsQueuedMail::found(FoundMail {
        id: QueueId(Arc::new(dest_uuid.to_string())),
        schedule: *schedule,
    }))
}

/// Blocking function!
fn cleanup_dest_dir(mail_dir: &Dir, dest_id: &str) {
    // TODO: consider logging IO errors on cleanups that follow an IO error
    let dest_dir = match mail_dir.sub_dir(dest_id) {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = dest_dir.remove_file(METADATA_FILE);
    let _ = dest_dir.remove_file(SCHEDULE_FILE);

    let _ = mail_dir.remove_dir(dest_id);
}

/// Blocking function!
fn cleanup_contents_dir(queue: &Dir, mail_uuid: String, mail_dir: &Dir) {
    // TODO: consider logging IO errors on cleanups that follow an IO error
    let _ = mail_dir.remove_file(CONTENTS_FILE);
    let _ = queue.remove_dir(mail_uuid);
}

#[async_trait]
impl<U> smtp_queue::StorageEnqueuer<U, FsQueuedMail> for FsEnqueuer<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    async fn commit(
        mut self,
        destinations: Vec<(MailMetadata<U>, ScheduleInfo)>,
    ) -> io::Result<Vec<FsQueuedMail>> {
        match self.flush().await {
            Ok(()) => (),
            Err(e) => {
                unblock(move || cleanup_contents_dir(&self.queue, self.mail_uuid, &self.mail_dir))
                    .await;
                return Err(e);
            }
        }
        let destinations = destinations
            .into_iter()
            .map(|(meta, sched)| {
                let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
                let dest_uuid = Uuid::new_v4()
                    .to_hyphenated_ref()
                    .encode_lower(&mut uuid_buf);
                (dest_uuid.to_string(), meta, sched)
            })
            .collect::<Vec<_>>();
        unblock(move || {
            let mut queued_mails = Vec::with_capacity(destinations.len());

            for d in 0..destinations.len() {
                match make_dest_dir(
                    &self.queue,
                    &self.mail_uuid,
                    &self.mail_dir,
                    &destinations[d].0,
                    &destinations[d].1,
                    &destinations[d].2,
                ) {
                    Ok(queued_mail) => queued_mails.push(queued_mail),
                    Err(e) => {
                        for dest in &destinations[0..d] {
                            cleanup_dest_dir(&self.mail_dir, &dest.0);
                        }
                        cleanup_contents_dir(&self.queue, self.mail_uuid, &self.mail_dir);
                        return Err(e);
                    }
                }
            }

            Ok(queued_mails)
        })
        .await
    }
}

impl<U> AsyncWrite for FsEnqueuer<U> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.writer) }.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.writer) }.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.writer) }.poll_close(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[IoSlice],
    ) -> Poll<io::Result<usize>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.writer) }.poll_write_vectored(cx, bufs)
    }
}
