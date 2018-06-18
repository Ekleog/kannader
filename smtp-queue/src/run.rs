use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{self, prelude::*, timer::Delay};

use mail::{FoundInflightMail, InflightMail, QueuedMail};
use send_queued_mail::send_queued_mail;
use storage::Storage;
use transport::Transport;

pub fn run<M, QM, IM, FIM, Stor, Transp>(
    storage: Arc<Stor>,
    transport: Arc<Transp>,
) -> impl Future<Item = (), Error = ()>
where
    M: 'static,
    QM: QueuedMail<M>,
    IM: InflightMail<M>,
    FIM: FoundInflightMail<M>,
    Stor: Storage<M, QM, IM, FIM>,
    Transp: Transport<M>,
{
    let startup = Instant::now();
    let storage_copy = storage.clone();
    let transport_copy = transport.clone();
    storage
        .list_queue()
        .for_each(move |qm| {
            tokio::spawn(send_queued_mail(
                storage_copy.clone(),
                transport_copy.clone(),
                qm,
            ))
        })
        .and_then(move |()| {
            // TODO: (B) Make this delay configurable
            // The important thing is it must be longer than the time between switching a
            // mail to inflight and either completing it or returning it to the queue
            storage.find_inflight().for_each(move |fim| {
                let storage_copy = storage.clone();
                let storage_copy2 = storage.clone();
                let transport_copy = transport.clone();
                tokio::spawn(
                    Delay::new(startup.clone() + Duration::from_secs(3600))
                        .map_err(|_| ())
                        .and_then(move |()| storage_copy.cancel_found_inflight(fim))
                        .and_then(move |qm| send_queued_mail(storage_copy2, transport_copy, qm)),
                )
            })
        })
}
