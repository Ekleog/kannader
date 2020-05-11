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
use chrono::{DateTime, Utc};
use futures::{io::IoSlice, prelude::*};
use openat::Dir;
use smol::blocking;
use smtp_queue::{MailMetadata, QueueId};
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
//  - <mail>/schedule: the JSON-encoded (scheduled, last_attempt) couple. This
//    one is the only one that could change over time, and it gets written by
//    writing a `schedule.{{random}}` then renaming it in-place

// TODO: make those configurable
const DATA_DIR: &'static str = "data";
const QUEUE_DIR: &'static str = "queue";
const INFLIGHT_DIR: &'static str = "inflight";
const CLEANUP_DIR: &'static str = "cleanup";

const CONTENTS_FILE: &'static str = "contents";
const METADATA_FILE: &'static str = "metadata";
const SCHEDULE_FILE: &'static str = "schedule";
const TMP_SCHEDULE_FILE_PREFIX: &'static str = "schedule.";

struct FsStorageImpl<U> {
    path: PathBuf,
    queue: Dir,
    phantom: PhantomData<U>,
}

pub struct FsStorage<U> {
    s: Arc<FsStorageImpl<U>>,
}

impl<U> FsStorage<U> {
    pub async fn new(path: PathBuf) -> io::Result<FsStorage<U>> {
        let path2 = path.clone();
        let queue = blocking!(Dir::open(&path2))?;
        Ok(FsStorage {
            s: Arc::new(FsStorageImpl {
                path,
                queue,
                phantom: PhantomData,
            }),
        })
    }
}

impl<U> Clone for FsStorage<U> {
    fn clone(&self) -> FsStorage<U> {
        FsStorage { s: self.s.clone() }
    }
}

#[async_trait]
impl<U> smtp_queue::Storage<U> for FsStorage<U>
where
    U: 'static + Send + Sync + for<'a> serde::Deserialize<'a> + serde::Serialize,
{
    type Enqueuer = FsEnqueuer;
    type InflightMail = FsInflightMail;
    type QueuedMail = FsQueuedMail;
    type Reader = FsReader;

    async fn list_queue(
        &self,
    ) -> Pin<Box<dyn Send + Stream<Item = Result<FsQueuedMail, (io::Error, Option<QueueId>)>>>>
    {
        Box::pin(
            scan_queue(self.clone(), QUEUE_DIR)
                .await
                .map(|r| r.map(FsQueuedMail::found)),
        )
    }

    async fn find_inflight(
        &self,
    ) -> Pin<Box<dyn Send + Stream<Item = Result<FsInflightMail, (io::Error, Option<QueueId>)>>>>
    {
        Box::pin(
            scan_queue(self.clone(), INFLIGHT_DIR)
                .await
                .map(|r| r.map(FsInflightMail::found)),
        )
    }

    async fn read_inflight(
        &self,
        mail: &FsInflightMail,
    ) -> Result<(MailMetadata<U>, FsReader), io::Error> {
        unimplemented!() // TODO
    }

    fn enqueue<'s, 'a>(
        &'s self,
        meta: MailMetadata<U>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = io::Result<FsEnqueuer>>>>
    where
        's: 'a,
    {
        unimplemented!() // TODO
    }

    async fn reschedule(
        &self,
        mail: &mut FsQueuedMail,
        at: DateTime<Utc>,
        last_attempt: Option<DateTime<Utc>>,
    ) -> io::Result<()> {
        mail.scheduled = at;
        mail.last_attempt = last_attempt;
        let this = self.clone();
        let id = mail.id.clone();
        smol::Task::blocking(async move {
            let mut tmp_sched_file = String::from(TMP_SCHEDULE_FILE_PREFIX);
            let mut uuid_buf: [u8; 45] = Uuid::encode_buffer();
            let uuid = Uuid::new_v4()
                .to_hyphenated_ref()
                .encode_lower(&mut uuid_buf);
            tmp_sched_file.push_str(uuid);
            let tmp_rel_path = Path::new(QUEUE_DIR).join(&*id.0).join(tmp_sched_file);
            let tmp_file = this.s.queue.new_file(&tmp_rel_path, 0600)?;
            serde_json::to_writer(tmp_file, &(at, last_attempt)).map_err(io::Error::from)?;
            let rel_path = Path::new(QUEUE_DIR).join(&*id.0).join(SCHEDULE_FILE);
            this.s.queue.local_rename(&tmp_rel_path, &rel_path)?;
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
}

struct FoundMail {
    id: QueueId,
    scheduled: DateTime<Utc>,
    last_attempt: Option<DateTime<Utc>>,
}

// TODO: handle dangling symlinks
async fn scan_queue<U, P>(
    this: FsStorage<U>,
    dir: P,
) -> impl 'static + Send + Stream<Item = Result<FoundMail, (io::Error, Option<QueueId>)>>
where
    U: 'static + Send + Sync,
    P: 'static + Send + AsRef<Path>,
{
    let dir = Arc::new(dir.as_ref().to_owned());
    // TODO: should use openat, not raw walkdir that'll do non-openat calls
    // (once that's done, `self.path` can probably be removed)
    let it = {
        let this = this.clone();
        let dir = dir.clone();
        blocking!(WalkDir::new(this.s.path.join(&*dir)).into_iter())
    };
    smol::iter(it)
        .then(move |p| {
            let this = this.clone();
            let dir = dir.clone();
            async move {
                let p = p.map_err(|e| (io::Error::from(e), None))?;
                if !p.path_is_symlink() {
                    Ok(None)
                } else {
                    let path = p
                        .path()
                        .to_str()
                        .ok_or((
                            io::Error::new(io::ErrorKind::InvalidData, "file path is not utf-8"),
                            None,
                        ))?
                        .to_owned();
                    let id = QueueId::new(&path);
                    // Note: if rust's type system knew that blocking!() is well-scoped, it'd
                    // probably make it possible to avoid the `to_owned` above
                    let (scheduled, last_attempt) = blocking!(
                        this.s
                            .queue
                            .open_file(&dir.join(path).join(SCHEDULE_FILE))
                            .and_then(|f| serde_json::from_reader(f).map_err(io::Error::from))
                    )
                    .map_err(|e| (e, Some(id.clone())))?;
                    Ok(Some(FoundMail {
                        id,
                        scheduled,
                        last_attempt,
                    }))
                }
            }
        })
        .filter_map(|r| async move { r.transpose() })
}

pub struct FsQueuedMail {
    id: QueueId,
    scheduled: DateTime<Utc>,
    last_attempt: Option<DateTime<Utc>>,
}

impl FsQueuedMail {
    fn found(f: FoundMail) -> FsQueuedMail {
        FsQueuedMail {
            id: f.id,
            scheduled: f.scheduled,
            last_attempt: f.last_attempt,
        }
    }

    // Not public, so that it doesn't encourage cloning -- cloning should work, but
    // will result in unexpected behavior
    fn clone(&self) -> FsQueuedMail {
        FsQueuedMail {
            id: self.id.clone(),
            scheduled: self.scheduled.clone(),
            last_attempt: self.last_attempt.clone(),
        }
    }
}

impl smtp_queue::QueuedMail for FsQueuedMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }

    fn scheduled_at(&self) -> DateTime<Utc> {
        self.scheduled
    }

    fn last_attempt(&self) -> Option<DateTime<Utc>> {
        self.last_attempt
    }
}

pub struct FsInflightMail {
    id: QueueId,
    scheduled: DateTime<Utc>,
    last_attempt: Option<DateTime<Utc>>,
}

impl FsInflightMail {
    fn found(f: FoundMail) -> FsInflightMail {
        FsInflightMail {
            id: f.id,
            scheduled: f.scheduled,
            last_attempt: f.last_attempt,
        }
    }
}

impl smtp_queue::InflightMail for FsInflightMail {
    fn id(&self) -> QueueId {
        self.id.clone()
    }
}

pub struct FsReader {
    // TODO
}

impl AsyncRead for FsReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        unimplemented!() // TODO
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
