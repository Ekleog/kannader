#![no_main]

#[macro_use]
extern crate libfuzzer_sys;

extern crate bytes;
extern crate smtp_message;

use bytes::Bytes;
use smtp_message::ReplyLine;

fuzz_target!(|data: &[u8]| {
    let _ = ReplyLine::parse(Bytes::from(data));
});
