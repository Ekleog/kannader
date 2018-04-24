#![no_main]
#[macro_use] extern crate libfuzzer_sys;

extern crate bytes;
extern crate smtp_message;
extern crate smtp_server;
extern crate tokio;

use bytes::*;
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

fn handler<R: Stream<Item = BytesMut, Error = ()>>(mail: MailMetadata, (): (), _: &ConnectionMetadata<()>, mut reader: DataStream<R>) -> (Option<Prependable<R>>, Decision<()>) {
    // TODO: should be async
    if let Err(_) = reader.by_ref().fold((), |_, _| future::ok(())).wait() {
        return (None, Decision::Reject(Refusal {
            code: ReplyCode::SYNTAX_ERROR,
            msg: (&"plz no syntax error"[..]).into(),
        }))
    }
    if mail.to.len() > 3 {
        (Some(reader.into_inner()), Decision::Reject(Refusal {
            code: ReplyCode::POLICY_REASON,
            msg: (&"Too many recipients!"[..]).into(),
        }))
    } else {
        (Some(reader.into_inner()), Decision::Accept(()))
    }
}

fuzz_target!(|data: &[u8]| {
    // Parse the input
    if data.len() < 1 {
        return;
    }
    let num_blocks = data[0] as usize;
    if data.len() < 1 + num_blocks || num_blocks < 1 {
        return;
    }
    let lengths = data[1..num_blocks].iter().map(|&x| x as usize).collect::<Vec<_>>();
    let total_len = lengths.iter().sum::<usize>();
    if data.len() < 256 + total_len {
        return;
    }
    let raw_data = &data[256..(256 + total_len)];

    let stream = stream::iter_ok(lengths.iter().scan(raw_data, |d, &l| {
        let res = BytesMut::from(&d[..l]);
        *d = &d[l..];
        //println!("Sending chunk {:?}", res);
        Some(res)
    }));
    let mut sink = DiscardSink {};
    let _ignore_errors = interact(
        stream,
        &mut sink,
        (),
        |()| panic!(),
        |()| panic!(),
        &filter_from,
        &filter_to,
        &handler,
    ).wait();
});
