use std::{
    cmp,
    io::{self, IoSlice, IoSliceMut},
    ops::Range,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{pin_mut, AsyncRead, AsyncWrite, AsyncWriteExt};
use pin_project::pin_project;

// use crate::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum EscapedDataReaderState {
    Start,
    Cr,
    CrLf,
    CrLfDot,
    CrLfDotCr,
    End,
    Completed,
}

/// `AsyncRead` instance that returns an unescaped `DATA` stream.
///
/// Note that:
///  - If a line (as defined by b"\r\n" endings) starts with a b'.', it is an
///    "escaping" dot that is not part of the actual contents of the line.
///  - If a line is exactly b".\r\n", it is the last line of the stream this
///    stream will give. It is not part of the actual contents of the message.
#[pin_project]
pub struct EscapedDataReader<'a, R> {
    buf: &'a mut [u8],

    // This should be another &'a mut [u8], but the issue described in [1] makes it not work
    // [1] https://github.com/rust-lang/rust/issues/72477
    unhandled: Range<usize>,

    state: EscapedDataReaderState,

    #[pin]
    read: R,
}

impl<'a, R> EscapedDataReader<'a, R>
where
    R: AsyncRead,
{
    #[inline]
    pub fn new(buf: &'a mut [u8], unhandled: Range<usize>, read: R) -> Self {
        EscapedDataReader {
            buf,
            unhandled,
            state: EscapedDataReaderState::CrLf,
            read,
        }
    }

    /// Returns `true` iff the message has been successfully streamed
    /// to completion
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.state == EscapedDataReaderState::End || self.state == EscapedDataReaderState::Completed
    }

    /// Asserts that the full message has been read, then marks this
    /// reader as complete. Note that this should be called only once
    /// the stream has been successfully saved, as subsequent users
    /// will assume that a completed stream means that the email has
    /// entered the queue.
    #[inline]
    pub fn complete(&mut self) {
        assert!(self.is_finished());
        self.state = EscapedDataReaderState::Completed;
    }

    /// Returns the range of data in the `buf` passed to `new` that
    /// contains data that hasn't been handled yet (ie. what followed
    /// the end-of-data marker) if `complete()` has been called, and
    /// `None` otherwise.
    #[inline]
    pub fn get_unhandled(&self) -> Option<Range<usize>> {
        if self.state == EscapedDataReaderState::Completed {
            Some(self.unhandled.clone())
        } else {
            None
        }
    }
}

impl<'a, R> AsyncRead for EscapedDataReader<'a, R>
where
    R: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_vectored(cx, &mut [IoSliceMut::new(buf)])
    }

    fn poll_read_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &mut [IoSliceMut],
    ) -> Poll<io::Result<usize>> {
        // If we have already finished, return early
        if self.is_finished() {
            return Poll::Ready(Ok(0));
        }

        let this = self.project();

        // First, fill the bufs with incoming data
        let raw_size = {
            let unhandled_len_start = this.unhandled.end - this.unhandled.start;
            if unhandled_len_start > 0 {
                for buf in bufs.iter_mut() {
                    let copy_len = cmp::min(buf.len(), this.unhandled.end - this.unhandled.start);
                    let next_start = this.unhandled.start + copy_len;
                    buf[..copy_len].copy_from_slice(&this.buf[this.unhandled.start..next_start]);
                    this.unhandled.start = next_start;
                }
                unhandled_len_start - (this.unhandled.end - this.unhandled.start)
            } else {
                match this.read.poll_read_vectored(cx, bufs) {
                    Poll::Ready(Ok(s)) => s,
                    other => return other,
                }
            }
        };

        // If there was nothing to read, return early
        if raw_size == 0 {
            if bufs.iter().map(|b| b.len()).sum::<usize>() == 0 {
                return Poll::Ready(Ok(0));
            } else {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "connection aborted without finishing the data stream",
                )));
            }
        }

        // Then, look for the end in the bufs
        let mut size = 0;
        for b in 0..bufs.len() {
            for i in 0..cmp::min(bufs[b].len(), raw_size - size) {
                use EscapedDataReaderState::*;
                match (*this.state, bufs[b][i]) {
                    (Cr, b'\n') => *this.state = CrLf,
                    (CrLf, b'.') => *this.state = CrLfDot,
                    (CrLfDot, b'\r') => *this.state = CrLfDotCr,
                    (CrLfDotCr, b'\n') => {
                        *this.state = End;
                        size += i + 1;

                        if this.unhandled.start == this.unhandled.end {
                            // The data (most likely) comes from `this.read` -- or, at least, we
                            // know that there can be nothing left in `this.unhandled`.
                            let remaining = cmp::min(bufs[b].len() - (i + 1), raw_size - size);
                            this.buf[..remaining]
                                .copy_from_slice(&bufs[b][i + 1..i + 1 + remaining]);
                            let mut copied = remaining;
                            for bb in b + 1..bufs.len() {
                                let remaining = cmp::min(bufs[bb].len(), raw_size - size - copied);
                                this.buf[copied..copied + remaining]
                                    .copy_from_slice(&bufs[bb][..remaining]);
                                copied += remaining;
                            }
                            *this.unhandled = 0..copied;
                        } else {
                            // The data comes straight out of `this.unhandled`,
                            // so let's just reuse it
                            this.unhandled.start -= raw_size - size;
                        }

                        return Poll::Ready(Ok(size));
                    }
                    (_, b'\r') => *this.state = Cr,
                    _ => *this.state = Start,
                }
            }
            size += cmp::min(bufs[b].len(), raw_size - size);
        }

        // Didn't reach the end, let's return everything found
        Poll::Ready(Ok(size))
    }
}

pub struct DataUnescapeRes {
    pub written: usize,
    pub unhandled_idx: usize,
}

/// Helper struct to unescape a data stream.
///
/// Note that one unescaper should be used for a single data stream. Creating a
/// `DataUnescaper` is basically free, and not creating a new one would probably
/// lead to initial `\r\n` being handled incorrectly.
pub struct DataUnescaper {
    is_preceded_by_crlf: bool,
}

impl DataUnescaper {
    /// Creates a `DataUnescaper`.
    ///
    /// The `is_preceded_by_crlf` argument is used to indicate whether, before
    /// the first buffer that is fed into `unescape`, the unescaper should
    /// assume that a `\r\n` was present.
    ///
    /// Usually, one will want to set `true` as an argument, as starting a
    /// `DataUnescaper` mid-line is a rare use case.
    pub fn new(is_preceded_by_crlf: bool) -> DataUnescaper {
        DataUnescaper {
            is_preceded_by_crlf,
        }
    }

    /// Unescapes data coming from an [`EscapedDataReader`](EscapedDataReader).
    ///
    /// This takes a `data` argument. It will modify the `data` argument,
    /// removing the escaping that could happen with it, and then returns a
    /// [`DataUnescapeRes`](DataUnescapeRes).
    ///
    /// It is possible that the end of `data` does not land on a boundary that
    /// allows yet to know whether data should be output or not. This is the
    /// reason why this returns a [`DataUnescapeRes`](DataUnescapeRes). The
    /// returned value will contain:
    ///  - `.written`, which is the number of unescaped bytes that have been
    ///    written in `data` — that is, `data[..res.written]` is the unescaped
    ///    data, and
    ///  - `.unhandled_idx`, which is the number of bytes at the end of `data`
    ///    that could not be handled yet for lack of more information — that is,
    ///    `data[res.unhandled_idx..]` is data that should be at the beginning
    ///    of the next call to `data_unescape`.
    ///
    /// Note that the unhandled data's length is never going to be longer than 4
    /// bytes long ("\r\n.\r", the longest sequence that can't be interpreted
    /// yet), so it should not be an issue to just copy it to the next
    /// buffer's start.
    pub fn unescape(&mut self, data: &mut [u8]) -> DataUnescapeRes {
        // TODO: this could be optimized by having a state machine we handle ourselves.
        // Unfortunately, neither regex nor regex_automata provide tooling for
        // noalloc replacements when the replacement is guaranteed to be shorter than
        // the match

        let mut written = 0;
        let mut unhandled_idx = 0;

        if self.is_preceded_by_crlf {
            if data.len() <= 3 {
                // Don't have enough information to know whether it's the end or just an escape.
                // Maybe it's nothing special, but let's not make an effort to check it, as
                // asking for 4-byte buffers should hopefully not be too much.
                return DataUnescapeRes {
                    written: 0,
                    unhandled_idx: 0,
                };
            } else if data.starts_with(b".\r\n") {
                // It is the end already
                return DataUnescapeRes {
                    written: 0,
                    unhandled_idx: 3,
                };
            } else if data[0] == b'.' {
                // It is just an escape, skip the dot
                unhandled_idx += 1;
            } else {
                // It is nothing special, just go the regular path
            }

            self.is_preceded_by_crlf = false;
        }

        // First, look for "\r\n."
        while let Some(i) = data[unhandled_idx..].windows(3).position(|s| s == b"\r\n.") {
            if data.len() <= unhandled_idx + i + 4 {
                // Don't have enough information to know whether it's the end or just an escape
                if unhandled_idx != written {
                    data.copy_within(unhandled_idx..unhandled_idx + i, written);
                }
                return DataUnescapeRes {
                    written: written + i,
                    unhandled_idx: unhandled_idx + i,
                };
            } else if &data[unhandled_idx + i + 3..unhandled_idx + i + 5] != b"\r\n" {
                // It is just an escape
                if unhandled_idx != written {
                    data.copy_within(unhandled_idx..unhandled_idx + i + 2, written);
                }
                written += i + 2;
                unhandled_idx += i + 3;
            } else {
                // It is the end
                if unhandled_idx != written {
                    data.copy_within(unhandled_idx..unhandled_idx + i + 2, written);
                }
                return DataUnescapeRes {
                    written: written + i + 2,
                    unhandled_idx: unhandled_idx + i + 5,
                };
            }
        }

        // There is no "\r\n." any longer, let's handle the remaining bytes by simply
        // checking whether they end with something that needs handling.
        if data.ends_with(b"\r\n") {
            if unhandled_idx != written {
                data.copy_within(unhandled_idx..data.len() - 2, written);
            }
            DataUnescapeRes {
                written: written + data.len() - 2 - unhandled_idx,
                unhandled_idx: data.len() - 2,
            }
        } else if data.ends_with(b"\r") {
            if unhandled_idx != written {
                data.copy_within(unhandled_idx..data.len() - 1, written);
            }
            DataUnescapeRes {
                written: written + data.len() - 1 - unhandled_idx,
                unhandled_idx: data.len() - 1,
            }
        } else {
            if unhandled_idx != written {
                data.copy_within(unhandled_idx..data.len(), written);
            }
            DataUnescapeRes {
                written: written + data.len() - unhandled_idx,
                unhandled_idx: data.len(),
            }
        }
    }
}

#[derive(Clone, Copy)]
enum EscapingDataWriterState {
    Start,
    Cr,
    CrLf,
}

/// `AsyncWrite` instance that takes an unescaped `DATA` stream and
/// escapes it.
#[pin_project]
pub struct EscapingDataWriter<W> {
    state: EscapingDataWriterState,

    #[pin]
    write: W,
}

impl<W> EscapingDataWriter<W>
where
    W: AsyncWrite,
{
    #[inline]
    pub fn new(write: W) -> Self {
        EscapingDataWriter {
            state: EscapingDataWriterState::CrLf,
            write,
        }
    }

    #[inline]
    pub async fn finish(self) -> io::Result<()> {
        let write = self.write;
        pin_mut!(write);
        match self.state {
            EscapingDataWriterState::CrLf => write.write_all(b".\r\n").await,
            _ => write.write_all(b"\r\n.\r\n").await,
        }
    }
}

impl<W> AsyncWrite for EscapingDataWriter<W>
where
    W: AsyncWrite,
{
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.poll_write_vectored(cx, &[IoSlice::new(buf)])
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.project().write.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Other,
            "tried closing a stream during a message",
        )))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[IoSlice],
    ) -> Poll<io::Result<usize>> {
        fn set_state_until(state: &mut EscapingDataWriterState, bufs: &[IoSlice], n: usize) {
            use EscapingDataWriterState::*;
            let mut n = n;
            for buf in bufs {
                if n.saturating_sub(2) > buf.len() {
                    n -= buf.len();
                    *state = Start;
                    continue;
                }
                for i in n.saturating_sub(2)..cmp::min(buf.len(), n) {
                    n -= 1;
                    match (*state, buf[i]) {
                        (_, b'\r') => *state = Cr,
                        (Cr, b'\n') => *state = CrLf,
                        // We know that this function can't be called with an escape happening
                        _ => *state = Start,
                    }
                }
                if n == 0 {
                    return;
                }
            }
        }

        let mut this = self.project();

        let initial_state = *this.state;
        for b in 0..bufs.len() {
            for i in 0..bufs[b].len() {
                use EscapingDataWriterState::*;
                match (*this.state, bufs[b][i]) {
                    (_, b'\r') => *this.state = Cr,
                    (Cr, b'\n') => *this.state = CrLf,
                    (CrLf, b'.') => {
                        let mut v = Vec::with_capacity(b + 1);
                        let mut writing = 0;
                        for bb in 0..b {
                            v.push(IoSlice::new(&bufs[bb]));
                            writing += bufs[bb].len();
                        }
                        v.push(IoSlice::new(&bufs[b][..=i]));
                        writing += i + 1;
                        return match this.write.poll_write_vectored(cx, &v) {
                            Poll::Ready(Ok(s)) => {
                                if s == writing {
                                    *this.state = Start;
                                    Poll::Ready(Ok(s - 1))
                                } else {
                                    *this.state = initial_state;
                                    set_state_until(&mut this.state, bufs, s);
                                    Poll::Ready(Ok(s))
                                }
                            }
                            o => o,
                        };
                    }
                    _ => *this.state = Start,
                }
            }
        }

        match this.write.poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(s)) => {
                if s == bufs.iter().map(|b| b.len()).sum::<usize>() {
                    Poll::Ready(Ok(s))
                } else {
                    *this.state = initial_state;
                    set_state_until(&mut this.state, bufs, s);
                    Poll::Ready(Ok(s))
                }
            }
            o => o,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    use futures::{
        executor,
        io::{AsyncReadExt, Cursor},
    };

    // TODO: actually test the vectored version of the function
    #[test]
    fn escaped_data_reader() {
        let tests: &[(&[&[u8]], &[u8], &[u8])] = &[
            (
                &[b"foo", b" bar", b"\r\n", b".\r", b"\n"],
                b"foo bar\r\n.\r\n",
                b"",
            ),
            (&[b"\r\n.\r\n", b"\r\n"], b"\r\n.\r\n", b"\r\n"),
            (&[b".\r\n"], b".\r\n", b""),
            (&[b".baz\r\n", b".\r\n", b"foo"], b".baz\r\n.\r\n", b"foo"),
            (&[b" .baz", b"\r\n.", b"\r\nfoo"], b" .baz\r\n.\r\n", b"foo"),
            (&[b".\r\n", b"MAIL FROM"], b".\r\n", b"MAIL FROM"),
            (&[b"..\r\n.\r\n"], b"..\r\n.\r\n", b""),
            (
                &[b"foo\r\n. ", b"bar\r\n.\r\n"],
                b"foo\r\n. bar\r\n.\r\n",
                b"",
            ),
            (&[b".\r\nMAIL FROM"], b".\r\n", b"MAIL FROM"),
            (&[b"..\r\n.\r\nMAIL FROM"], b"..\r\n.\r\n", b"MAIL FROM"),
        ];
        let mut surrounding_buf: [u8; 16] = [0; 16];
        let mut enclosed_buf: [u8; 8] = [0; 8];
        for (i, &(inp, out, rem)) in tests.iter().enumerate() {
            println!(
                "Trying to parse test {} into {:?} with {:?} remaining\n",
                i,
                show_bytes(out),
                show_bytes(rem)
            );

            let mut reader = inp[1..].iter().map(Cursor::new).fold(
                Box::pin(futures::io::empty()) as Pin<Box<dyn 'static + AsyncRead>>,
                |a, b| Box::pin(AsyncReadExt::chain(a, b)),
            );

            surrounding_buf[..inp[0].len()].copy_from_slice(inp[0]);
            let mut data_reader =
                EscapedDataReader::new(&mut surrounding_buf, 0..inp[0].len(), reader.as_mut());

            let mut res_out = Vec::<u8>::new();
            while let Ok(r) = executor::block_on(data_reader.read(&mut enclosed_buf)) {
                if r == 0 {
                    break;
                }
                println!(
                    "got out buf (size {}): {:?}",
                    r,
                    show_bytes(&enclosed_buf[..r])
                );
                res_out.extend_from_slice(&enclosed_buf[..r]);
            }
            data_reader.complete();
            println!(
                "total out is: {:?}, hoping for: {:?}",
                show_bytes(&res_out),
                show_bytes(out)
            );
            assert_eq!(&res_out[..], out);

            let unhandled = data_reader.get_unhandled().unwrap();
            let mut res_rem = Vec::<u8>::new();
            res_rem.extend_from_slice(&surrounding_buf[unhandled]);

            while let Ok(r) = executor::block_on(reader.read(&mut surrounding_buf)) {
                if r == 0 {
                    break;
                }
                println!("got rem buf: {:?}", show_bytes(&surrounding_buf[..r]));
                res_rem.extend_from_slice(&surrounding_buf[0..r]);
            }
            println!(
                "total rem is: {:?}, hoping for: {:?}",
                show_bytes(&res_rem),
                show_bytes(rem)
            );
            assert_eq!(&res_rem[..], rem);
        }
    }

    #[test]
    fn data_unescaper() {
        let tests: &[(&[&[u8]], &[u8])] = &[
            (&[b"foo", b" bar", b"\r\n", b".\r", b"\n"], b"foo bar\r\n"),
            (&[b"\r\n.\r\n"], b"\r\n"),
            (&[b".baz\r\n", b".\r\n"], b"baz\r\n"),
            (&[b" .baz", b"\r\n.", b"\r\n"], b" .baz\r\n"),
            (&[b".\r\n"], b""),
            (&[b"..\r\n.\r\n"], b".\r\n"),
            (&[b"foo\r\n. ", b"bar\r\n.\r\n"], b"foo\r\n bar\r\n"),
            (&[b"\r\r\n.\r\n"], b"\r\r\n"),
        ];
        let mut buf: [u8; 1024] = [0; 1024];
        for &(inp, out) in tests {
            println!(
                "Test: {:?}",
                itertools::concat(
                    inp.iter()
                        .map(|i| show_bytes(i).chars().collect::<Vec<char>>())
                )
                .iter()
                .collect::<String>()
            );
            let mut res = Vec::<u8>::new();
            let mut end = 0;
            let mut unescaper = DataUnescaper::new(true);
            for i in inp {
                buf[end..end + i.len()].copy_from_slice(i);
                let r = unescaper.unescape(&mut buf[..end + i.len()]);
                res.extend_from_slice(&buf[..r.written]);
                buf.copy_within(r.unhandled_idx..end + i.len(), 0);
                end = end + i.len() - r.unhandled_idx;
            }
            println!("Result: {:?}", show_bytes(&res));
            assert_eq!(&res[..], out);
        }
    }

    #[test]
    fn escaping_data_writer() {
        let tests: &[(&[&[&[u8]]], &[u8])] = &[
            (&[&[b"foo", b" bar"], &[b" baz"]], b"foo bar baz\r\n.\r\n"),
            (&[&[b"foo\r\n. bar\r\n"]], b"foo\r\n.. bar\r\n.\r\n"),
            (&[&[b""]], b".\r\n"),
            (&[&[b"."]], b"..\r\n.\r\n"),
            (&[&[b"\r"]], b"\r\r\n.\r\n"),
            (&[&[b"foo\r"]], b"foo\r\r\n.\r\n"),
            (&[&[b"foo bar\r", b"\n"]], b"foo bar\r\n.\r\n"),
            (
                &[&[b"foo bar\r\n"], &[b". baz\n"]],
                b"foo bar\r\n.. baz\n\r\n.\r\n",
            ),
        ];
        for &(inp, out) in tests {
            println!("Expected result: {:?}", show_bytes(out));
            let mut v = Vec::new();
            let c = Cursor::new(&mut v);
            let mut w = EscapingDataWriter::new(c);
            for write in inp {
                let mut written = 0;
                let total_to_write = write.iter().map(|b| b.len()).sum::<usize>();
                while written != total_to_write {
                    let mut i = Vec::new();
                    let mut skipped = 0;
                    for s in *write {
                        if skipped + s.len() <= written {
                            skipped += s.len();
                            println!("(skipping, skipped = {})", skipped);
                            continue;
                        }
                        if written - skipped != 0 {
                            println!("(skipping first {} chars)", written - skipped);
                            i.push(IoSlice::new(&s[(written - skipped)..]));
                            skipped = written;
                        } else {
                            println!("(skipping nothing)");
                            i.push(IoSlice::new(s));
                        }
                    }
                    println!("Writing: {:?}", i);
                    written += executor::block_on(w.write_vectored(&i)).unwrap();
                    println!("Written: {:?} (out of {:?})", written, total_to_write);
                }
            }
            executor::block_on(w.finish()).unwrap();
            assert_eq!(&v, &out);
        }
    }
}
