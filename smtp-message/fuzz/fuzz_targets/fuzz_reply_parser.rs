#![no_main]

use bytes::Bytes;
use libfuzzer_sys::fuzz_target;

use smtp_message::ReplyLine;

fuzz_target!(|data: &[u8]| {
    let _ = ReplyLine::parse(Bytes::copy_from_slice(data));
});
