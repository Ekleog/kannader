#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|inp: (Vec<Vec<Vec<u8>>>, usize, usize, Vec<usize>)| {
    smtp_message::fuzz::escaping_then_unescaping(inp.0, inp.1, inp.2, inp.3);
});
