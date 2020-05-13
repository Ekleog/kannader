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

// TODO: turn this into a section of the book
// TODO: the data folder should probably be split in like 256 sub-folders, to
// allow the sysadmin to share it across many partitions
//
// Assumptions:
//  - Moving a symlink to another folder is atomic between <queue>/queue,
//    <queue>/inflight and <queue>/cleanup
//  - Moving a file is atomic between files in the same <mail> folder
//  - Creating a symlink in the <queue> folder is atomic
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
    data: Arc<Dir>,
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
        let data = {
            let main_dir = main_dir.clone();
            Arc::new(blocking!(main_dir.sub_dir(DATA_DIR))?)
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
        let mail_dir = {
            let inflight = self.inflight.clone();
            let mail = mail.id.0.clone();
            Arc::new(blocking!(inflight.sub_dir(&*mail))?)
        };
        let metadata = {
            let mail_dir = mail_dir.clone();
            blocking!(
                mail_dir
                    .open_file(METADATA_FILE)
                    .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
            )?
        };
        let reader = {
            let mail_dir = mail_dir.clone();
            let contents = blocking!(mail_dir.open_file(CONTENTS_FILE))?;
            Box::pin(smol::reader(contents))
        };
        Ok((metadata, reader))
    }

    async fn enqueue(
        &self,
        metadata: MailMetadata<U>,
        schedule: ScheduleInfo,
    ) -> io::Result<FsEnqueuer> {
        let data = self.data.clone();
        let queue = self.queue.clone();

        // TODO: the two files could be written concurrently (being concurrent with the
        // FsEnqueuer is going to lose the early-failure property it currently has)
        blocking!({
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);

            data.create_dir(&*uuid, 0600)?;
            let mail_dir = data.sub_dir(&*uuid)?;

            let schedule_file = mail_dir.new_file(SCHEDULE_FILE, 0600)?;
            serde_json::to_writer(schedule_file, &schedule)?;

            let metadata_file = mail_dir.new_file(METADATA_FILE, 0600)?;
            serde_json::to_writer(metadata_file, &metadata)?;

            let contents_file = mail_dir.new_file(CONTENTS_FILE, 0600)?;
            Ok(FsEnqueuer {
                queue,
                uuid: uuid.to_owned(),
                writer: Box::pin(smol::writer(contents_file)),
                schedule,
            })
        })
    }

    async fn reschedule(&self, mail: &mut FsQueuedMail, schedule: ScheduleInfo) -> io::Result<()> {
        mail.schedule = schedule;

        let mail_dir = {
            let queue = self.queue.clone();
            let id = mail.id.0.clone();
            blocking!(queue.sub_dir(&*id))?
        };

        blocking!({
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

    async fn send_start(
        &self,
        mail: FsQueuedMail,
    ) -> Result<Option<FsInflightMail>, (FsQueuedMail, io::Error)> {
        blocking!({ unimplemented!() })
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
    queue: Arc<Dir>,
    uuid: String,
    writer: Pin<Box<dyn 'static + Send + AsyncWrite>>,
    schedule: ScheduleInfo,
}

#[async_trait]
impl smtp_queue::StorageEnqueuer<FsQueuedMail> for FsEnqueuer {
    async fn commit(mut self) -> io::Result<FsQueuedMail> {
        self.flush().await?;
        blocking!({
            let mut symlink_value = String::from("../");
            symlink_value.push_str(DATA_DIR);
            symlink_value.push_str(&self.uuid);
            self.queue.symlink(&self.uuid, symlink_value)?;

            Ok(FsQueuedMail::found(FoundMail {
                id: QueueId(Arc::new(self.uuid)),
                schedule: self.schedule,
            }))
        })
    }
}

impl AsyncWrite for FsEnqueuer {
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
