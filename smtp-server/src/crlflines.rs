use std::pin::Pin;

use bytes::BytesMut;
use futures::{Stream, StreamExt};

use smtp_message::Prependable;

pub async fn next_crlf_line<S: Stream<Item = BytesMut>>(
    mut s: Pin<&mut Prependable<S>>,
) -> Option<BytesMut> {
    let mut buf = BytesMut::new();
    while let Some(pkt) = s.next().await {
        buf.unsplit(pkt);

        // TODO: (B) implement line length limits id:line-length-limit

        // TODO: (C) optimize searching for crlf p:line-length-limit
        // This can be done with much fewer allocations and searches through the buffer.
        // Technique : do not extending the buffers straightaway but store them in a vec
        // until the CRLF is found, and then extending with the right size).
        // Other idea: just extend at line length limit.

        if let Some(pos) = buf.windows(2).position(|x| x == b"\r\n") {
            // This unwrap is free of risk, as `s.next()` has just been called above
            s.prepend(buf.split_off(pos + 2)).unwrap();
            return Some(buf);
        }
    }

    // Failed to find a crlf before end-of-stream
    // This unwrap is free of risk, as `s.next()` has just been called above
    s.prepend(buf).unwrap();
    // TODO: (A) here s will already have returned none, need to make sure we're not
    // polling it again for the next none!!
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::{executor, stream};

    use smtp_message::StreamExt;

    #[test]
    fn crlflines_looks_good() {
        let stream = stream::iter(
            vec![
                &b"MAIL FROM:<foo@bar.example.org>\r\n"[..],
                b"RCPT TO:<baz@quux.example.org>\r\n",
                b"RCPT TO:<foo2@bar.example.org>\r\n",
                b"DATA\r\n",
                b"Hello World\r\n",
                b".\r\n",
                b"QUIT\r\n",
            ]
            .into_iter()
            .map(BytesMut::from),
        )
        .prependable();

        assert_eq!(executor::block_on_stream(stream).collect::<Vec<_>>(), vec![
            b"MAIL FROM:<foo@bar.example.org>\r\n".to_vec(),
            b"RCPT TO:<baz@quux.example.org>\r\n".to_vec(),
            b"RCPT TO:<foo2@bar.example.org>\r\n".to_vec(),
            b"DATA\r\n".to_vec(),
            b"Hello World\r\n".to_vec(),
            b".\r\n".to_vec(),
            b"QUIT\r\n".to_vec(),
        ]);
    }
}
