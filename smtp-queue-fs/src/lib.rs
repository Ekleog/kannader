use std::{
    future::Future,
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
use smol::blocking;
use smtp_queue::{MailMetadata, QueueId, ScheduleInfo};
use uuid::Uuid;
use walkdir::WalkDir;

// Assumptions:
//  - Moving a symlink to another folder is atomic between <queue>/queue,
//    <queue>/inflight and <queue>/cleanup
//  - Moving a file is atomic between files in the same <mail> folder
//  - Once a write is flushed without error, it is guaranteed not to be changed
//    by something other than a yuubind instance (or another system aware of
//    yuubind's protocol and guarantees)
//
// File structure:
//  - <queue>/data: location for the contents and metadata of the emails in the
//    queue
//  - <queue>/queue: folder for holding symlinks to the emails
//  - <queue>/inflight: folder for holding symlinks to the emails that are
//    currently in flight
//  - <queue>/cleanup: folder for holding symlinks to the emails that are
//    currently being deleted after being successfully sent
//
// Each email in <queue>/data is a folder, that is constituted of:
//  - <mail>/contents: the RFC5322 content of the email
//  - <mail>/metadata: the JSON-encoded MailMetadata<U>
//  - <mail>/schedule: the JSON-encoded ScheduleInfo couple. This one is the
//    only one that could change over time, and it gets written by writing a
//    `schedule.{{random}}` then renaming it in-place
//
// When enqueuing, the process is:
//  - Create <queue>/data/<uuid>, thereafter named <mail>
//  - Write <mail>/schedule and <mail>/metadata
//  - Give out the Enqueuer to the user for writing <mail>/contents
//  - Wait for the user to commit the Enqueuer
//  - Create a symlink from <queue>/queue/<uuid> to <mail>
//
// When starting to send / cancelling sends, the process is:
//  - Move <queue>/queue/<id> to <queue>/inflight/<id> (or back)
//
// When done with sending a mail and it thus needs to be removed from disk, the
// process is.
//  - Move <queue>/inflight/<id> to <queue>/cleanup/<id>
//  - Remove <queue>/cleanup/<id>/* (which actually are in <queue>/data/<id>/*)
//  - Remove the target of <queue>/cleanup/<id> (the folder in <queue>/data)
//  - Remove the <queue>/cleanup/<id> symlink

// TODO: make those configurable?
pub const DATA_DIR: &'static str = "data";
pub const QUEUE_DIR: &'static str = "queue";
pub const INFLIGHT_DIR: &'static str = "inflight";
pub const CLEANUP_DIR: &'static str = "cleanup";

pub const CONTENTS_FILE: &'static str = "contents";
pub const METADATA_FILE: &'static str = "metadata";
pub const SCHEDULE_FILE: &'static str = "schedule";
pub const TMP_SCHEDULE_FILE_PREFIX: &'static str = "schedule.";

pub struct FsStorage<U> {
    path: PathBuf,
    queue: Arc<Dir>,
    inflight: Arc<Dir>,
    cleanup: Arc<Dir>,
    phantom: PhantomData<U>,
}

// TODO: remove all these clone() that are required only due to
// https://github.com/stjepang/blocking/issues/1
impl<U> FsStorage<U> {
    pub async fn new(path: PathBuf) -> io::Result<FsStorage<U>> {
        let main_dir = {
            let path = path.clone();
            Arc::new(blocking!(Dir::open(&path))?)
        };
        let queue = {
            let main_dir = main_dir.clone();
            Arc::new(blocking!(main_dir.sub_dir(QUEUE_DIR))?)
        };
        let inflight = {
            let main_dir = main_dir.clone();
            Arc::new(blocking!(main_dir.sub_dir(INFLIGHT_DIR))?)
        };
        let cleanup = {
            let main_dir = main_dir.clone();
            Arc::new(blocking!(main_dir.sub_dir(CLEANUP_DIR))?)
        };
        Ok(FsStorage {
            path,
            queue,
            inflight,
            cleanup,
            phantom: PhantomData,
        })
    }
}

impl<U> Clone for FsStorage<U> {
    fn clone(&self) -> FsStorage<U> {
        FsStorage {
            path: self.path.clone(),
            queue: self.queue.clone(),
            inflight: self.inflight.clone(),
            cleanup: self.cleanup.clone(),
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<U> smtp_queue::Storage<U> for FsStorage<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    type Enqueuer = FsEnqueuer;
    type InDropMail = FsInDropMail;
    type InflightLister =
        Pin<Box<dyn Send + Stream<Item = Result<FsInflightMail, (io::Error, Option<QueueId>)>>>>;
    type InflightMail = FsInflightMail;
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

    // TODO: ideally this'd return a *future* to MailMetadata<U> as well as a
    // FsReader, so that it's possible to select() on the two or even spawn them to
    // run in parallel. That's probably an optimization not worth doing right now,
    // though
    async fn read_inflight(
        &self,
        mail: &FsInflightMail,
    ) -> Result<(MailMetadata<U>, Self::Reader), io::Error> {
        let mail = {
            let this = self.clone();
            let mail = mail.id.0.clone();
            Arc::new(blocking!(this.inflight.sub_dir(&*mail))?)
        };
        let metadata = {
            let mail = mail.clone();
            blocking!(
                mail.open_file(METADATA_FILE)
                    .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
            )?
        };
        let reader = {
            let mail = mail.clone();
            let contents = blocking!(mail.open_file(CONTENTS_FILE))?;
            Box::pin(smol::reader(contents))
        };
        Ok((metadata, reader))
    }

    async fn enqueue(
        &self,
        meta: MailMetadata<U>,
        schedule: ScheduleInfo,
    ) -> io::Result<FsEnqueuer> {
        smol::Task::blocking(async move {
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            unimplemented!() // TODO
        })
        .await;
        unimplemented!() // TODO
    }

    async fn reschedule(&self, mail: &mut FsQueuedMail, schedule: ScheduleInfo) -> io::Result<()> {
        mail.schedule = schedule;

        let mail_dir = {
            let this = self.clone();
            let id = mail.id.clone();
            blocking!(this.queue.sub_dir(&*id.0))?
        };

        smol::Task::blocking(async move {
            let mut tmp_sched_file = String::from(TMP_SCHEDULE_FILE_PREFIX);
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            tmp_sched_file.push_str(uuid);

            let tmp_file = mail_dir.new_file(&tmp_sched_file, 0600)?;
            serde_json::to_writer(tmp_file, &schedule).map_err(io::Error::from)?;

            mail_dir.local_rename(&tmp_sched_file, SCHEDULE_FILE)?;

            Ok::<_, io::Error>(())
        })
        .await?;
        Ok(())
    }

    fn send_start<'s, 'a>(
        &'s self,
        mail: FsQueuedMail,
    ) -> Pin<
        Box<
            dyn 'a
                + Send
                + Future<Output = Result<Option<FsInflightMail>, (FsQueuedMail, io::Error)>>,
        >,
    >
    where
        's: 'a,
    {
        unimplemented!() // TODO
    }

    fn send_done<'s, 'a>(
        &'s self,
        mail: FsInflightMail,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Result<(), (FsInflightMail, io::Error)>>>>
    where
        's: 'a,
    {
        unimplemented!() // TODO
    }

    fn send_cancel<'s, 'a>(
        &'s self,
        mail: FsInflightMail,
    ) -> Pin<
        Box<
            dyn 'a
                + Send
                + Future<Output = Result<Option<FsQueuedMail>, (FsInflightMail, io::Error)>>,
        >,
    >
    where
        's: 'a,
    {
        unimplemented!() // TODO
    }

    async fn drop_start(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsInDropMail>, (FsQueuedMail, io::Error)> {
        unimplemented!() // TODO
    }

    async fn drop_confirm(&self, mail: FsInDropMail) -> Result<bool, (FsInDropMail, io::Error)> {
        unimplemented!() // TODO
    }
}

struct FoundMail {
    id: QueueId,
    schedule: ScheduleInfo,
}

// TODO: handle dangling symlinks
async fn scan_queue<P>(
    path: P,
    dir: Arc<Dir>,
) -> impl 'static + Send + Stream<Item = Result<FoundMail, (io::Error, Option<QueueId>)>>
where
    P: 'static + Send + AsRef<Path>,
{
    // TODO: should use openat, not raw walkdir that'll do non-openat calls
    // (once that's done, `self.path` can probably be removed)
    let it = blocking!(WalkDir::new(path).into_iter());
    smol::iter(it)
        .then(move |p| {
            let dir = dir.clone();
            async move {
                let p = p.map_err(|e| (io::Error::from(e), None))?;
                if !p.path_is_symlink() {
                    Ok(None)
                } else {
                    let path = p.path().to_str().ok_or((
                        io::Error::new(io::ErrorKind::InvalidData, "file path is not utf-8"),
                        None,
                    ))?;
                    let id = QueueId::new(path);
                    let schedule_path = Path::new(path).join(SCHEDULE_FILE);
                    let schedule = blocking!(
                        dir.open_file(&schedule_path)
                            .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
                    )
                    .map_err(|e| (e, Some(id.clone())))?;
                    Ok(Some(FoundMail { id, schedule }))
                }
            }
        })
        .filter_map(|r| async move { r.transpose() })
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

    // Not public, so that it doesn't encourage cloning -- cloning should work, but
    // will result in unexpected behavior
    fn clone(&self) -> FsQueuedMail {
        FsQueuedMail {
            id: self.id.clone(),
            schedule: self.schedule.clone(),
        }
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
}

impl smtp_queue::InflightMail for FsInflightMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }
}

pub struct FsInDropMail {
    id: QueueId,
}

impl smtp_queue::InDropMail for FsInDropMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }
}

pub struct FsEnqueuer {
    // TODO
}

#[async_trait]
impl smtp_queue::StorageEnqueuer<FsQueuedMail> for FsEnqueuer {
    async fn commit(self) -> io::Result<FsQueuedMail> {
        unimplemented!() // TODO
    }
}

impl AsyncWrite for FsEnqueuer {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        unimplemented!() // TODO
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unimplemented!() // TODO
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        unimplemented!() // TODO
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[IoSlice],
    ) -> Poll<io::Result<usize>> {
        unimplemented!() // TODO
    }
}
