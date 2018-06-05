use bytes::BytesMut;
use tokio::prelude::*;

use mail::{InflightMail, Mail, QueuedMail};

// TODO: (B) replace all these Box by impl Trait syntax hide:impl-trait-in-trait
// TODO: (B) for a clean api, the futures should not take ownership and return
// but rather take a reference (when async/await will be done)
pub trait Storage<M, QM: QueuedMail, IM: InflightMail>: Sized {
    fn list_queue(&self) -> Box<Stream<Item = QM, Error = ()>>;
    fn list_inflight(&self) -> Box<Stream<Item = IM, Error = ()>>;

    fn enqueue<S>(self, mail: Mail<S, M>) -> Box<Future<Item = (Self, QM), Error = ()>>
    where
        S: Stream<Item = BytesMut, Error = ()>;

    fn send_start(self, mail: QM) -> Box<Future<Item = (Self, IM), Error = ()>>;
    fn send_done(self, mail: IM) -> Box<Future<Item = Self, Error = ()>>;
    fn send_cancelled(self, mail: IM) -> Box<Future<Item = (Self, QM), Error = ()>>;
}
