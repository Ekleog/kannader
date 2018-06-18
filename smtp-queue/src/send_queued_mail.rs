use tokio::prelude::*;

use mail::QueuedMail;

pub fn send_queued_mail<M, QM: QueuedMail<M>>(mail: QM) -> impl Future<Item = (), Error = ()> {
    future::ok(()) // TODO: (A) implement
}
