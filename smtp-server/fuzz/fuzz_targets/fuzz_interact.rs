#![no_main]

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures::{executor, prelude::*};
use libfuzzer_sys::fuzz_target;

use smtp_message::{DataStream, Email, ReplyCode, SmtpString};
use smtp_server::{interact, ConnectionMetadata, Decision, MailMetadata, Refusal};

struct DiscardSink {}

impl Sink<Bytes> for DiscardSink {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), ()>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, _item: Bytes) -> Result<(), ()> {
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), ()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), ()>> {
        Poll::Ready(Ok(()))
    }
}

struct FuzzConfig;

impl smtp_server::Config<()> for FuzzConfig {
    fn hostname(&self) -> SmtpString {
        SmtpString::from_static(b"test.example.org")
    }

    fn filter_from<'a>(
        &'a mut self,
        from: &'a mut Option<Email>,
        _conn_meta: &'a mut ConnectionMetadata<()>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Decision>>> {
        if let Some(from) = from {
            let loc = from.localpart();
            let locb = loc.bytes();
            if locb.len() >= 2 && locb[0] > locb[1] {
                Box::pin(future::ready(Decision::Accept))
            } else {
                Box::pin(future::ready(Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg: (&"forbidden user"[..]).into(),
                })))
            }
        } else {
            Box::pin(future::ready(Decision::Accept))
        }
    }

    fn filter_to<'a>(
        &'a mut self,
        to: &'a mut Email,
        _meta: &'a mut MailMetadata,
        _conn_meta: &'a mut ConnectionMetadata<()>,
    ) -> Pin<Box<dyn 'a + Send + Future<Output = Decision>>> {
        let loc = to.localpart();
        let locb = loc.bytes();
        if locb.len() >= 2 && locb[0] > locb[1] {
            Box::pin(future::ready(Decision::Accept))
        } else {
            Box::pin(future::ready(Decision::Reject(Refusal {
                code: ReplyCode::POLICY_REASON,
                msg: (&"forbidden user"[..]).into(),
            })))
        }
    }

    fn handle_mail<'a, S>(
        &'a mut self,
        stream: &'a mut DataStream<S>,
        mail: MailMetadata,
        _conn_meta: &'a mut ConnectionMetadata<()>,
    ) -> Pin<Box<dyn 'a + Future<Output = Decision>>>
    where
        S: 'a + Unpin + Stream<Item = BytesMut>,
    {
        Box::pin(async move {
            stream.skip_while(|_| future::ready(true)).next().await;
            stream.complete().unwrap();
            if mail.to.len() > 3 {
                // This is stupid, please use filter_to instead if you're not just willing
                // to fuzz
                Decision::Reject(Refusal {
                    code: ReplyCode::POLICY_REASON,
                    msg: (&"Too many recipients!"[..]).into(),
                })
            } else {
                Decision::Accept
            }
        })
    }
}

fuzz_target!(|data: Vec<Vec<u8>>| {
    let chunks = data.into_iter().map(|d| {
        let res = BytesMut::from(&d[..]);
        // println!("Sending chunk {:?}", res);
        res
    });

    // And send stuff in
    let stream = stream::iter(chunks);
    let sink = DiscardSink {};
    futures::pin_mut!(sink);
    let _ignore_errors = executor::block_on(interact(stream, sink, (), &mut FuzzConfig));
});
