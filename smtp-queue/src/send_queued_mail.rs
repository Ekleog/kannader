use std::sync::Arc;
use tokio::{self, prelude::*};

use mail::{FoundInflightMail, InflightMail, QueuedMail};
use storage::Storage;
use transport::Transport;

// TODO: (B) Async/await should remove this Box h:async-await
pub fn send_queued_mail<M, QM, IM, FIM, Stor, Transp>(
    storage: Arc<Stor>,
    transport: Arc<Transp>,
    mail: QM,
) -> Box<dyn Future<Item = (), Error = ()> + Send>
where
    M: 'static,
    QM: QueuedMail<M>,
    IM: InflightMail<M>,
    FIM: FoundInflightMail<M>,
    Stor: Storage<M, QM, IM, FIM>,
    Transp: Transport<M>,
{
    Box::new(
        storage
            .send_start(mail)
            .and_then(|im| im.get_mail().map(|mail| (im, mail)))
            .and_then(|(im, mail)| {
                transport.send(mail).then(|res| {
                    match res {
                        Ok(()) => future::Either::A(storage.send_done(im)),
                        Err((_code, _msg)) => {
                            // TODO: (B) log an error / retry
                            future::Either::B(storage.send_cancel(im).and_then(|qm| {
                                tokio::spawn(send_queued_mail(storage, transport, qm))
                            }))
                        }
                    }
                })
            }),
    )
}
