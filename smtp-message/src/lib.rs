#![type_length_limit = "109238057"]

pub use nom;

mod command;
mod data;
mod misc;
mod reply;

// use command::*;
// use data::*;
use misc::*;
// use reply::*;

pub use command::{Command, ParameterName, Parameters};
pub use data::{DataUnescapeRes, DataUnescaper, EscapedDataReader, EscapingDataWriter};
pub use misc::{next_crlf, Email, Hostname, Localpart, MaybeUtf8, NextCrLfState, Path};
pub use reply::{
    EnhancedReplyCode, EnhancedReplyCodeClass, EnhancedReplyCodeSubject, Reply, ReplyCode,
    ReplyCodeCategory, ReplyCodeKind, ReplyLine,
};

#[cfg(test)]
use std::str;

/// Used as `println!("{:?}", show_bytes(b))`
#[cfg(test)]
pub(crate) fn show_bytes(b: &[u8]) -> String {
    if b.len() > 128 {
        "{too long}".into()
    } else if let Ok(s) = str::from_utf8(b) {
        s.into()
    } else {
        format!("{:?}", b)
    }
}

#[cfg(any(test, feature = "fuzz-targets"))]
pub mod fuzz {
    use super::*;

    use std::{cmp, io::IoSlice};

    use futures::{
        executor,
        io::{AsyncReadExt, AsyncWriteExt, Cursor},
    };
    use regex_automata::RegexBuilder;

    pub fn escaping_then_unescaping(
        data: Vec<Vec<Vec<u8>>>,
        maxread: usize,
        initread: usize,
        mut readlen: Vec<usize>,
    ) {
        if readlen.is_empty() {
            readlen.push(1);
        }
        // println!("==> NEW TEST");
        // println!("  maxread = {}, initread = {}", maxread, initread);
        // if readlen.len() < 128 {
        // println!("  readlen = {:?}", readlen);
        // } else {
        // println!("  readlen is too long to be displayed");
        // }
        // if data
        // .iter()
        // .flat_map(|v| v.iter().map(|w| w.len()))
        // .sum::<usize>()
        // < 128
        // {
        // println!("  data = {:?}", data);
        // } else {
        // println!("  data is too long to be displayed");
        // }

        let mut wire = Vec::new();

        // println!("Writing to the wire");
        {
            let mut writer = EscapingDataWriter::new(Cursor::new(&mut wire));
            for write in data.iter() {
                let mut written = 0;
                let total_to_write = write.iter().map(|b| b.len()).sum::<usize>();
                while written != total_to_write {
                    let mut i = Vec::new();
                    let mut skipped = 0;
                    for s in write {
                        if skipped + s.len() <= written {
                            skipped += s.len();
                            continue;
                        }
                        if written - skipped != 0 {
                            i.push(IoSlice::new(&s[(written - skipped)..]));
                            skipped = written;
                        } else {
                            i.push(IoSlice::new(s));
                        }
                    }
                    written += executor::block_on(writer.write_vectored(&i)).unwrap();
                }
            }
            executor::block_on(writer.finish()).unwrap();
        }

        // println!("Checking that the wire looks good");
        {
            // println!("  Wire is: {:?}", show_bytes(&wire));

            assert!(wire == b".\r\n" || wire.ends_with(b"\r\n.\r\n"));

            // Either there's no such sequence, or it's at the end
            let reg = RegexBuilder::new()
                .allow_invalid_utf8(true)
                .build(r#"\r\n\.[^.]"#)
                .unwrap();
            assert!(
                reg.find(&wire)
                    .map(|(start, _)| start == wire.len() - 5)
                    .unwrap_or(true)
            );
        }

        // println!("Reading from the wire");
        let mut read = Vec::new();
        {
            // Let's cap at 16MiB of buffer, or it's going to be too much. And minimum at 5,
            // as documented in unescape, we need 4 bytes for unhandled data plus 1 byte for
            // the newly read data.
            let maxread = cmp::max(cmp::min(maxread, 16 * 1024 * 1024), 5);
            let mut initbuf = vec![0; maxread];
            let mut buf = vec![0; maxread];
            let initread = cmp::min(cmp::min(initread, maxread), wire.len());
            initbuf[..initread].copy_from_slice(&wire[..initread]);
            wire = wire[initread..].to_owned();
            let mut reader = EscapedDataReader::new(&mut initbuf, 0..initread, &wire[..]);
            let mut unescaper = DataUnescaper::new(true);
            let mut i = 0;
            let mut start = 0;
            loop {
                // println!("  Entering the loop with i={}", i);
                let read_size = cmp::min(cmp::max(1, readlen[i % readlen.len()]), maxread - start);
                assert!(read_size > 0, "read_size = 0, bug in the test harness");
                let bytes_read =
                    executor::block_on(reader.read(&mut buf[start..start + read_size])).unwrap();
                // println!(
                // "    Raw read: {:?} (read_size {})",
                // show_bytes(&buf[start..start + bytes_read]),
                // read_size,
                // );
                if bytes_read == 0 {
                    break;
                }
                let unesc = unescaper.unescape(&mut buf[..start + bytes_read]);
                read.extend_from_slice(&buf[..unesc.written]);
                // println!(
                // "    Unescaped read: {:?}",
                // show_bytes(&buf[..unesc.written])
                // );
                buf.copy_within(unesc.unhandled_idx..start + bytes_read, 0);
                start = start + bytes_read - unesc.unhandled_idx;
                i += 1;
            }
            // println!("  Exiting the loop");
            reader.complete();
            assert!(reader.get_unhandled().unwrap().is_empty());
        }

        // println!("Checking that the output matches");
        {
            let mut expected = data
                .iter()
                .flat_map(|v| v.iter().flat_map(|w| w.iter().cloned()))
                .collect::<Vec<u8>>();
            if !expected.is_empty() && !expected.ends_with(b"\r\n") {
                expected.extend_from_slice(b"\r\n");
            }
            // println!("Read    : {:?}", show_bytes(&read));
            // println!("Expected: {:?}", show_bytes(&expected));
            assert_eq!(read, expected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use quickcheck_macros::quickcheck;

    #[quickcheck]
    pub fn escaping_then_unescaping(
        data: Vec<Vec<Vec<u8>>>,
        maxread: usize,
        initread: usize,
        readlen: Vec<usize>,
    ) {
        fuzz::escaping_then_unescaping(data, maxread, initread, readlen)
    }
}
