#![feature(test)]

extern crate test;

use bytes::Bytes;
use smtp_message::Command;
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
