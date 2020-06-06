#![no_main]
#![type_length_limit = "109238057"]

use libfuzzer_sys::fuzz_target;

use smtp_message::Command;

fuzz_target!(|data: &[u8]| {
    let _ = Command::<&str>::parse(data);
});
