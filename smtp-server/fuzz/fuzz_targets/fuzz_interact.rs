#![no_main]
#[macro_use] extern crate libfuzzer_sys;
extern crate smtp_message;
extern crate smtp_server;
extern crate tokio;

use tokio::prelude::*;

use smtp_message::*;
use smtp_server::*;

struct DiscardSink {}

impl Sink for DiscardSink {
    type SinkItem = u8;
    type SinkError = ();

    fn start_send(&mut self, _: u8) -> Result<AsyncSink<u8>, ()> {
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, ()> {
        Ok(Async::Ready(()))
    }
}

fn filter_from(addr: &Option<Email>, _: &ConnectionMetadata<()>) -> Decision<()> {
    if let Some(ref addr) = addr {
        let loc = addr.localpart();
        let locb = loc.as_bytes();
        if locb.len() >= 2 && locb[0] > locb[1] {
            Decision::Accept(())
        } else {
            Decision::Reject(Refusal {
                code: ReplyCode::POLICY_REASON,
                msg: (&"forbidden user"[..]).into(),
            })
        }
    } else {
        Decision::Accept(())
    }
}

fn filter_to(email: &Email, _: &mut (), _: &ConnectionMetadata<()>, _: &MailMetadata) -> Decision<()> {
    let loc = email.localpart();
    let locb = loc.as_bytes();
    if locb.len() >= 2 && locb[0] > locb[1] {
        Decision::Accept(())
    } else {
        Decision::Reject(Refusal {
            code: ReplyCode::POLICY_REASON,
            msg: (&"forbidden user"[..]).into(),
        })
    }
}

fn handler<R: Stream<Item = u8>>(mail: MailMetadata, (): (), _: &ConnectionMetadata<()>, mut reader: DataStream<R>) -> (R, Decision<()>) {
    if mail.to.len() > 3 {
        (reader.into_inner(), Decision::Reject(Refusal {
            code: ReplyCode::POLICY_REASON,
            msg: (&"Too many recipients!"[..]).into(),
        }))
    } else {
        (reader.into_inner(), Decision::Accept(()))
    }
}

fuzz_target!(|data: &[u8]| {
    let stream = stream::iter_ok(data.iter().cloned());
    let mut sink = DiscardSink {};
    interact(
        stream,
        &mut sink,
        (),
        |()| panic!(),
        |()| panic!(),
        &filter_from,
        &filter_to,
        &handler,
    ).wait().unwrap();
});
