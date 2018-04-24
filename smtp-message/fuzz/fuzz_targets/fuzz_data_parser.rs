#![no_main]
#[macro_use] extern crate libfuzzer_sys;

extern crate bytes;
extern crate smtp_message;
extern crate tokio;

use bytes::BytesMut;
use smtp_message::*;
use tokio::prelude::*;

fuzz_target!(|data: &[u8]| {
    // Parse the input
    if data.len() < 1 {
        return;
    }
    let num_blocks = data[0] as usize;
    if data.len() < 1 + num_blocks || num_blocks < 1 {
        return;
    }
    let lengths = data[1..num_blocks].iter().map(|&x| x as usize).collect::<Vec<_>>();
    let total_len = lengths.iter().sum::<usize>();
    if data.len() < 256 + total_len {
        return;
    }
    let raw_data = &data[256..(256 + total_len)];

    // Compute what DataStream gives
    // `result` will hold:
    //  * None if the stream was not terminated
    //  * Some((output, remaining)) if the stream was terminated
    let result = {
        let mut stream = DataStream::new(
            stream::iter_ok(lengths.iter().scan(raw_data, |d, &l| {
                let res = BytesMut::from(&d[..l]);
                *d = &d[l..];
                println!("Sending chunk {:?}", res);
                Some(res)
            })).map_err(|()| ())
                .prependable()
        );
        let output = stream.by_ref().concat2().wait().ok();
        output.map(|out| (out, stream.into_inner().concat2().wait().unwrap()))
    };

    // Compute with a naive algorithm
    let eof = raw_data.windows(5).position(|x| x == b"\r\n.\r\n").map(|p| (p + 2, p + 5));
    let naive_result = eof.map(|(eof, rem)| {
        if eof < 2 {
            (BytesMut::from(&raw_data[..eof]), BytesMut::from(&raw_data[eof..]))
        } else {
            let mut out = raw_data[..2].to_vec();
            for w in raw_data[..eof].windows(3) {
                if w != b"\r\n." {
                    out.push(w[2]);
                }
            }
            (BytesMut::from(out), BytesMut::from(&raw_data[rem..]))
        }
    });

    // And compare
    assert_eq!(result, naive_result);
});
