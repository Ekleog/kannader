mod sink;
mod stream;

use nom::crlf;
use std::io;

use crate::{byteslice::ByteSlice, stupidparsers::eat_spaces};

pub use self::{sink::DataSink, stream::DataStream};

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::{Bytes, BytesMut};
    use futures::{executor::block_on, stream, StreamExt};
    use nom::IResult;

    use crate::streamext::StreamExt as SmtpStreamExt;

    quickcheck! {
        fn data_stream_and_sink_are_compatible(end_with_crlf: bool, v: Vec<Vec<u8>>) -> bool {
            let mut input = v.into_iter().map(|x| Bytes::from(x)).collect::<Vec<_>>();
            eprintln!("Got: ({:?}, {:?})", end_with_crlf, input);
            let mut raw_input = input.iter().flat_map(|x| x.iter().cloned()).collect::<Vec<_>>();
            if end_with_crlf && (raw_input.len() < 2 || &raw_input[(raw_input.len() - 2)..] != b"\r\n") {
                raw_input.extend_from_slice(b"\r\n");
                input.push(Bytes::from(&b"\r\n"[..]));
            }
            if !end_with_crlf && raw_input.len() >= 2 && &raw_input[(raw_input.len() - 2)..] == b"\r\n" {
                raw_input.pop();
                let l = input.len();
                let ll = input[l - 1].len();
                input[l - 1].truncate(ll - 1);
            }
            let mut on_the_wire = Vec::new();
            {
                let mut sink = DataSink::new(&mut on_the_wire);
                block_on(async {
                    for i in input.iter().cloned() {
                        sink.send(i).await.unwrap();
                    }
                    sink.end().await.unwrap();
                });
            }
            eprintln!("Moving on the wire: {:?}", on_the_wire);
            let received = block_on(async {
                let mut stream = stream::iter(on_the_wire.into_iter().map(|b| BytesMut::from(&b[..]))).prependable();
                let mut stream = DataStream::new(&mut stream);
                let mut res = BytesMut::new();
                while let Some(i) = stream.next().await {
                    res.unsplit(i);
                }
                res
            });
            eprintln!("Recovered: {:?}", received);
            if !end_with_crlf && !raw_input.is_empty() {
                raw_input.extend_from_slice(b"\r\n");
            }
            eprintln!("Expected: {:?}", BytesMut::from(&raw_input[..]));
            received == raw_input
        }
    }
}
