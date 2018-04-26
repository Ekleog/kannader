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
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, _: Bytes) -> Result<AsyncSink<Bytes>, ()> {
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

fn handler<'a, R: 'a + Stream<Item = BytesMut, Error = ()>>(
    mail: MailMetadata<'static>,
    (): (),
    _: &ConnectionMetadata<()>,
    reader: DataStream<R>
) -> impl Future<Item = (Option<Prependable<R>>, Decision<()>), Error = ()> + 'a {
    reader.concat_and_recover().map_err(|_| ()).and_then(move |(_, reader)| {
        if mail.to.len() > 3 {
            // This is stupid, please use filter_to instead if you're not just willing to fuzz
            future::ok((Some(reader.into_inner()), Decision::Reject(Refusal {
                code: ReplyCode::POLICY_REASON,
                msg: (&"Too many recipients!"[..]).into(),
            })))
        } else {
            future::ok((Some(reader.into_inner()), Decision::Accept(())))
        }
    })
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
    let chunks = lengths.iter().scan(raw_data, |d, &l| {
        let res = BytesMut::from(&d[..l]);
        *d = &d[l..];
        //println!("Sending chunk {:?}", res);
        Some(res)
    });

    // And send stuff in
    let stream = stream::iter_ok(chunks);
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
