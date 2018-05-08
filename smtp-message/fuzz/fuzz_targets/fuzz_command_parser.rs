#![no_main]

extern crate bytes;
#[macro_use]
extern crate libfuzzer_sys;
extern crate smtp_message;

use bytes::Bytes;

use smtp_message::Command;

fuzz_target!(|data: &[u8]| {
    let _ = Command::parse(Bytes::from(data));
});
