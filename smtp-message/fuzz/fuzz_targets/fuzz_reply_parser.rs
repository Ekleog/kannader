#![no_main]

use libfuzzer_sys::fuzz_target;

use smtp_message::Reply;

fuzz_target!(|data: &[u8]| {
    let _ = Reply::<&str>::parse(data);
});
