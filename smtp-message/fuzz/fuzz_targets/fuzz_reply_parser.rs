#![no_main]

#[macro_use]
extern crate libfuzzer_sys;
extern crate smtp_message;

use smtp_message::Reply;

fuzz_target!(|data: &[u8]| {
    let _ = Reply::parse(data);
});
