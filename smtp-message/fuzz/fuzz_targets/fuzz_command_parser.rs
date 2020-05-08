#![no_main]

use bytes::Bytes;
use libfuzzer_sys::fuzz_target;

use smtp_message::Command;

fuzz_target!(|data: &[u8]| {
    let _ = Command::parse(Bytes::copy_from_slice(data));
});
