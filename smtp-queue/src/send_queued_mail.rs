use std::sync::Arc;
use tokio::prelude::*;

use mail::{QueuedMail, InflightMail, FoundInflightMail};
use storage::Storage;
use transport::Transport;

pub fn send_queued_mail<M, QM, IM, FIM, Stor, Transp>(
    storage: Arc<Stor>,
    transport: Arc<Transp>,
    mail: QM,
) -> impl Future<Item = (), Error = ()>
where
    QM: QueuedMail<M>,
    IM: InflightMail<M>,
    FIM: FoundInflightMail<M>,
    Stor: Storage<M, QM, IM, FIM>,
    Transp: Transport<M>,
{
    future::ok(()) // TODO: (A) implement
}
