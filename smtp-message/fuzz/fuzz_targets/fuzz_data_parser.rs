#![no_main]

use bytes::BytesMut;
use futures::{executor, prelude::*};
use libfuzzer_sys::fuzz_target;

use smtp_message::{DataStream, StreamExt};

fuzz_target!(|data: Vec<Vec<u8>>| {
    // Compute what DataStream gives
    // `result` will hold:
    //  * None if the stream was not terminated
    //  * Some((output, remaining)) if the stream was terminated
    let result = {
        let stream = stream::iter(data.iter().map(|d| {
            let res = BytesMut::from(&d[..]);
            // println!("Sending chunk {:?}", res);
            res
        })).prependable();
        let fut = Box::pin(stream);
        let mut stream = DataStream::new(fut.as_mut());
        let output = executor::block(stream.by_ref().concat()).ok();
        output.map(|out| (out, stream.into_inner().concat2().wait().unwrap()))
    };

    // Compute with a naive algorithm
    let raw_data = data
        .into_iter()
        .flat_map(|x| x.into_iter())
        .collect::<Vec<u8>>();
    let eof = (if raw_data.get(..3) == Some(b".\r\n") {
        Some((0, 3))
    } else {
        None
    })
    .or_else(|| {
        raw_data
            .windows(5)
            .position(|x| x == b"\r\n.\r\n")
            .map(|p| (p + 2, p + 5))
    });
    let naive_result = eof.map(|(eof, rem)| {
        if eof < 2 {
            (
                BytesMut::from(&raw_data[..eof]),
                BytesMut::from(&raw_data[rem..]),
            )
        } else {
            let mut out = if raw_data[0] == b'.' {
                raw_data[1..2].to_vec()
            } else {
                raw_data[..2].to_vec()
            };
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
