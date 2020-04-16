#![feature(test)]

extern crate test;

use bytes::{Bytes, BytesMut};
use futures::{executor, future, stream};
use smtp_message::{Command, DataSink, DataStream, StreamExt};
use test::Bencher;

#[bench]
fn parse_command(b: &mut Bencher) {
    let commands = &[
        Bytes::from_static(b"DATA\r\n"),
        Bytes::from_static(b"EHLO example.org\r\n"),
        Bytes::from_static(b"EXPN test\r\n"),
        Bytes::from_static(b"HELO example.org\r\n"),
        Bytes::from_static(b"HELP stuff\r\n"),
        Bytes::from_static(
            b"MAIL FROM:<@example.com,@example.org:test@example.net> FOO=BAR BAZ\r\n",
        ),
        Bytes::from_static(b"NOOP things\r\n"),
        Bytes::from_static(b"QUIT\r\n"),
        Bytes::from_static(
            b"RCPT TO:<@example.org,@example.com:foo@example.net> THINGS=DONE MAYBE\r\n",
        ),
        Bytes::from_static(b"RSET\r\n"),
        Bytes::from_static(b"VRFY root\r\n"),
    ];
    b.iter(|| {
        for c in commands {
            test::black_box(Command::parse(c.clone()).unwrap());
        }
    });
    b.bytes = commands.iter().map(|b| b.len() as u64).sum();
}

#[bench]
fn parse_data_stream(b: &mut Bencher) {
    let mut data = BytesMut::new();
    data.extend(&b"Blah blah blah...\r\n...etc. etc. etc.\r\n.\r\n"[..]);
    b.iter(|| {
        let mut stream = stream::once(future::ready(data.clone())).prependable();
        test::black_box(executor::block_on_stream(&mut DataStream::new(&mut stream)).count());
    });
    b.bytes = data.len() as u64;
}

#[bench]
fn output_data_sink(b: &mut Bencher) {
    let data = Bytes::from_static(b"Blah blah blah...\r\n..etc. etc. etc.\r\n");
    let mut v = Vec::with_capacity(1024);
    b.iter(|| {
        let mut sink = DataSink::new(&mut v);
        test::black_box(executor::block_on(sink.send(data.clone())).unwrap());
        test::black_box(executor::block_on(sink.end()).unwrap());
    });
    b.bytes = data.len() as u64;
}
