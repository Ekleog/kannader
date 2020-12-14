use std::{
    io::{self, Write},
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

pub const DATA_DIR: &'static str = "data";
pub const QUEUE_DIR: &'static str = "queue";
pub const INFLIGHT_DIR: &'static str = "inflight";
pub const CLEANUP_DIR: &'static str = "cleanup";

pub const DATA_DIR_FROM_OTHER_QUEUE: &'static str = "../data";

pub const CONTENTS_FILE: &'static str = "contents";
pub const METADATA_FILE: &'static str = "metadata";
pub const SCHEDULE_FILE: &'static str = "schedule";
pub const TMP_METADATA_FILE_PREFIX: &'static str = "metadata.";
pub const TMP_SCHEDULE_FILE_PREFIX: &'static str = "schedule.";

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
            Arc::new(unblock!(Dir::open(&*path))?)
        };
        let data = {
            let main_dir = main_dir.clone();
            Arc::new(unblock!(main_dir.sub_dir(DATA_DIR))?)
        };
        let queue = {
            let main_dir = main_dir.clone();
            Arc::new(unblock!(main_dir.sub_dir(QUEUE_DIR))?)
        };
        let inflight = {
            let main_dir = main_dir.clone();
            Arc::new(unblock!(main_dir.sub_dir(INFLIGHT_DIR))?)
        };
        let cleanup = {
            let main_dir = main_dir.clone();
            Arc::new(unblock!(main_dir.sub_dir(CLEANUP_DIR))?)
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

#[async_trait]
impl<U> smtp_queue::Storage<U> for FsStorage<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    type Enqueuer = FsEnqueuer<U>;
    type InflightLister =
        Pin<Box<dyn Send + Stream<Item = Result<FsInflightMail, (io::Error, Option<QueueId>)>>>>;
    type InflightMail = FsInflightMail;
    type PendingCleanupLister = Pin<
        Box<dyn Send + Stream<Item = Result<FsPendingCleanupMail, (io::Error, Option<QueueId>)>>>,
    >;
    type PendingCleanupMail = FsPendingCleanupMail;
    type QueueLister =
        Pin<Box<dyn Send + Stream<Item = Result<FsQueuedMail, (io::Error, Option<QueueId>)>>>>;
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
        let mail_dir = {
            let inflight = self.inflight.clone();
            let mail = mail.id.0.clone();
            Arc::new(unblock!(inflight.sub_dir(&*mail))?)
        };
        let metadata = {
            let mail_dir = mail_dir.clone();
            unblock!(
                mail_dir
                    .open_file(METADATA_FILE)
                    .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
            )?
        };
        let reader = {
            let mail_dir = mail_dir.clone();
            let contents = unblock!(mail_dir.open_file(CONTENTS_FILE))?;
            Box::pin(smol::Async::new(contents)?)
        };
        Ok((metadata, reader))
    }

    async fn enqueue(&self) -> io::Result<FsEnqueuer<U>> {
        let data = self.data.clone();
        let queue = self.queue.clone();

        unblock!({
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);

            data.create_dir(&*uuid, 0600)?;
            let mail_dir = data.sub_dir(&*uuid)?;

            let contents_file = mail_dir.new_file(CONTENTS_FILE, 0600)?;
            Ok(FsEnqueuer {
                queue,
                uuid: uuid.to_owned(),
                mail_dir,
                writer: Box::pin(smol::Async::new(contents_file)?),
                phantom: PhantomData,
            })
        })
    }

    async fn reschedule(&self, mail: &mut FsQueuedMail, schedule: ScheduleInfo) -> io::Result<()> {
        mail.schedule = schedule;

        let mail_dir = {
            let queue = self.queue.clone();
            let id = mail.id.0.clone();
            unblock!(queue.sub_dir(&*id))?
        };

        unblock!({
            let mut tmp_sched_file = String::from(TMP_SCHEDULE_FILE_PREFIX);
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            tmp_sched_file.push_str(uuid);

            let tmp_file = mail_dir.new_file(&tmp_sched_file, 0600)?;
            serde_json::to_writer(tmp_file, &schedule)?;

            mail_dir.local_rename(&tmp_sched_file, SCHEDULE_FILE)?;

            Ok::<_, io::Error>(())
        })?;
        Ok(())
    }

    async fn remeta(&self, mail: &mut FsInflightMail, meta: &MailMetadata<U>) -> io::Result<()> {
        let mail_dir = {
            let inflight = self.inflight.clone();
            let id = mail.id.0.clone();
            unblock!(inflight.sub_dir(&*id))?
        };

        // TODO: figure out a way to scope tasks so that we don't have to create this
        // all in memory ahead of time
        let new_meta_bytes = serde_json::to_vec(meta)?;

        unblock!({
            let mut tmp_meta_file = String::from(TMP_METADATA_FILE_PREFIX);
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            tmp_meta_file.push_str(uuid);

            let mut tmp_file = mail_dir.new_file(&tmp_meta_file, 0600)?;
            tmp_file.write_all(&new_meta_bytes)?;

            mail_dir.local_rename(&tmp_meta_file, METADATA_FILE)?;

            Ok::<(), io::Error>(())
        })
    }

    async fn send_start(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsInflightMail>, (FsQueuedMail, io::Error)> {
        let queue = self.queue.clone();
        let inflight = self.inflight.clone();
        unblock!({
            match openat::rename(&*queue, &*mail.id.0, &*inflight, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_inflight())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            }
        })
    }

    async fn send_done(
        &self,
        mail: FsInflightMail,
    ) -> Result<Option<FsPendingCleanupMail>, (FsInflightMail, io::Error)> {
        let inflight = self.inflight.clone();
        let cleanup = self.cleanup.clone();
        unblock!({
            match openat::rename(&*inflight, &*mail.id.0, &*cleanup, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_pending_cleanup())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            }
        })
    }

    async fn send_cancel(
        &self,
        mail: FsInflightMail,
    ) -> Result<Option<FsQueuedMail>, (FsInflightMail, io::Error)> {
        let inflight = self.inflight.clone();
        let queue = self.queue.clone();
        unblock!({
            match openat::rename(&*inflight, &*mail.id.0, &*queue, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_queued())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            }
        })
    }

    async fn drop(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsPendingCleanupMail>, (FsQueuedMail, io::Error)> {
        let queue = self.queue.clone();
        let cleanup = self.cleanup.clone();
        unblock!({
            match openat::rename(&*queue, &*mail.id.0, &*cleanup, &*mail.id.0) {
                Ok(()) => Ok(Some(mail.into_pending_cleanup())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err((mail, e)),
            }
        })
    }

    async fn cleanup(
        &self,
        mail: FsPendingCleanupMail,
    ) -> Result<bool, (FsPendingCleanupMail, io::Error)> {
        let cleanup = self.cleanup.clone();
        let data = self.data.clone();
        unblock!({
            match cleanup.sub_dir(&*mail.id.0) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (), // already removed
                Err(e) => return Err((mail, e)),
                Ok(mail_dir) => {
                    match mail_dir.remove_file(CONTENTS_FILE) {
                        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                        _ => (),
                    }

                    match mail_dir.remove_file(METADATA_FILE) {
                        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                        _ => (),
                    }

                    match mail_dir.remove_file(SCHEDULE_FILE) {
                        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                        _ => (),
                    }
                }
            };

            let symlink_target = match cleanup.read_link(&*mail.id.0) {
                Ok(t) => t,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err((mail, e)),
            };

            let path_in_data_dir = match symlink_target.strip_prefix(DATA_DIR_FROM_OTHER_QUEUE) {
                Ok(p) => p,
                Err(_) => {
                    return Err((
                        mail,
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "message symlink is outside the queue",
                        ),
                    ));
                }
            };

            match data.remove_dir(path_in_data_dir) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((mail, e)),
                _ => (),
            }

            match cleanup.remove_file(&*mail.id.0) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err((mail, e)),
            }
        })
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
    let it = unblock!(WalkDir::new(path).into_iter());
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
            let schedule = unblock!(
                dir.open_file(&schedule_path)
                    .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
            )
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
    queue: Arc<Dir>,
    uuid: String,
    mail_dir: Dir,
    writer: Pin<Box<dyn 'static + Send + AsyncWrite>>,
    // FsEnqueuer needs the U type parameter just so as to be able to take it as a parameter later
    // on
    phantom: PhantomData<fn(U)>,
}

#[async_trait]
impl<U> smtp_queue::StorageEnqueuer<U, FsQueuedMail> for FsEnqueuer<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    async fn commit(
        mut self,
        metadata: MailMetadata<U>,
        schedule: ScheduleInfo,
    ) -> io::Result<FsQueuedMail> {
        self.flush().await?;
        unblock!({
            let schedule_file = self.mail_dir.new_file(SCHEDULE_FILE, 0600)?;
            serde_json::to_writer(schedule_file, &schedule)?;

            let metadata_file = self.mail_dir.new_file(METADATA_FILE, 0600)?;
            serde_json::to_writer(metadata_file, &metadata)?;

            let mut symlink_value = String::from(DATA_DIR_FROM_OTHER_QUEUE);
            symlink_value.push_str(&self.uuid);
            self.queue.symlink(&self.uuid, symlink_value)?;

            Ok(FsQueuedMail::found(FoundMail {
                id: QueueId(Arc::new(self.uuid)),
                schedule,
            }))
        })
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
