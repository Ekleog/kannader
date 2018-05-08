#![no_main]

#[macro_use]
extern crate libfuzzer_sys;

extern crate bytes;
extern crate smtp_message;
extern crate smtp_server;
extern crate tokio;

use bytes::{Bytes, BytesMut};
use tokio::prelude::*;

use smtp_message::{DataStream, Email, Prependable, ReplyCode, StreamExt};
use smtp_server::{interact, ConnectionMetadata, Decision, MailMetadata, Refusal};

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

struct FuzzConfig {}

impl smtp_server::Config<()> for FuzzConfig {
    fn filter_from(&mut self, addr: &Option<Email>, _: &ConnectionMetadata<()>) -> Decision {
        if let Some(ref addr) = addr {
            let loc = addr.localpart();
            let locb = loc.bytes();
            if locb.len() >= 2 && locb[0] > locb[1] {
                Decision::Accept
            } else {
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg:  (&"forbidden user"[..]).into(),
                })
            }
        } else {
            Decision::Accept
        }
    }

    fn filter_to(
        &mut self,
        email: &Email,
        _: &MailMetadata,
        _: &ConnectionMetadata<()>,
    ) -> Decision {
        let loc = email.localpart();
        let locb = loc.bytes();
        if locb.len() >= 2 && locb[0] > locb[1] {
            Decision::Accept
        } else {
            Decision::Reject(Refusal {
                code: ReplyCode::POLICY_REASON,
                msg:  (&"forbidden user"[..]).into(),
            })
        }
    }

    fn handle_mail<'a, S: 'a + Stream<Item = BytesMut, Error = ()>>(
        &'a mut self,
        reader: DataStream<S>,
        mail: MailMetadata,
        _: &ConnectionMetadata<()>,
    ) -> Box<'a + Future<Item = (&'a mut Self, Option<Prependable<S>>, Decision), Error = ()>> {
        Box::new(
            reader
                .concat_and_recover()
                .map_err(|_| ())
                .and_then(move |(_, reader)| {
                    if mail.to.len() > 3 {
                        // This is stupid, please use filter_to instead if you're not just willing
                        // to fuzz
                        future::ok((
                            self,
                            Some(reader.into_inner()),
                            Decision::Reject(Refusal {
                                code: ReplyCode::POLICY_REASON,
                                msg:  (&"Too many recipients!"[..]).into(),
                            }),
                        ))
                    } else {
                        future::ok((self, Some(reader.into_inner()), Decision::Accept))
                    }
                }),
        )
    }
}

fuzz_target!(|data: Vec<Vec<u8>>| {
    let chunks = data.into_iter().map(|d| {
        let res = BytesMut::from(d);
        // println!("Sending chunk {:?}", res);
        res
    });

    // And send stuff in
    let stream = stream::iter_ok(chunks);
    let mut sink = DiscardSink {};
    let mut cfg = FuzzConfig {};
    let _ignore_errors = interact(
        stream,
        &mut sink,
        (),
        |()| panic!(),
        |()| panic!(),
        &mut cfg,
    ).wait();
});
