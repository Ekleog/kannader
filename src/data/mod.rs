use std::{fmt, io};

use nom::crlf;

use helpers::*;
use parse_helpers::*;

// Still SMTP-escaped (ie. leading ‘.’ doubled) message
// Must end with `\r\n`
#[cfg_attr(test, derive(PartialEq))]
enum ActualData<'a> {
    Owned(Vec<u8>),
    Borrowing(&'a [u8]),
}

impl<'a> ActualData<'a> {
    pub fn get(&self) -> &[u8] {
        match self {
            &ActualData::Owned(ref v) => &v,
            &ActualData::Borrowing(v) => v,
        }
    }
}

impl<'a> fmt::Debug for ActualData<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            &ActualData::Owned(ref v) => write!(f, "ActualData::Owned({})", bytes_to_dbg(v)),
            &ActualData::Borrowing(v) => write!(f, "ActualData::Borrowing({})", bytes_to_dbg(v)),
        }
    }
}

#[derive(Copy, Clone)]
enum EscapeState { Start, CrPassed, CrlfPassed }

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct DataCommand<'a> {
    data: ActualData<'a>,
}

impl<'a> DataCommand<'a> {
    // SMTP-escapes (ie. doubles leading ‘.’) messages first
    pub fn new(data: &[u8], likely_starting_dots: usize) -> DataCommand {
        let mut res = Vec::with_capacity(data.len() + likely_starting_dots);
        let mut state = EscapeState::Start;
        for &x in data {
            match (state, x) {
                (_, b'\r')                      => { state = EscapeState::CrPassed; },
                (EscapeState::CrPassed, b'\n')  => { state = EscapeState::CrlfPassed; },
                (EscapeState::CrlfPassed, b'.') => { state = EscapeState::Start; res.push(b'.'); },
                _                               => { state = EscapeState::Start; }
            }
            res.push(x);
        }
        match state {
            EscapeState::Start      => { res.extend_from_slice(b"\r\n"); },
            EscapeState::CrPassed   => { res.push(b'\n'); },
            EscapeState::CrlfPassed => { },
        }
        DataCommand { data: ActualData::Owned(res) }
    }

    pub unsafe fn new_raw(data: &[u8]) -> DataCommand {
        DataCommand { data: ActualData::Borrowing(data) }
    }

    pub fn raw_data(&self) -> &[u8] {
        self.data.get()
    }

    pub fn data(&self) -> Vec<u8> {
        self.data.get().iter().scan(EscapeState::Start, |state, &x| {
            match (*state, x) {
                (_, b'\r')                      => { *state = EscapeState::CrPassed;   Some(Some(x)) },
                (EscapeState::CrPassed, b'\n')  => { *state = EscapeState::CrlfPassed; Some(Some(x)) },
                (EscapeState::CrlfPassed, b'.') => { *state = EscapeState::Start;      Some(None   ) },
                _                               => { *state = EscapeState::Start;      Some(Some(x)) },
            }
        }).filter_map(|x| x).collect()
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"DATA\r\n")?;
        w.write_all(self.data.get())?;
        w.write_all(b".\r\n")
    }
}

named!(pub command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    data: alt!(
        map!(peek!(tag!(".\r\n")), |_| &b""[..]) |
        recognize!(do_parse!(
            take_until!("\r\n.\r\n") >>
            tag!("\r\n") >>
            ()
        ))
    ) >>
    tag!(".\r\n") >>
    (DataCommand {
        data: ActualData::Borrowing(data),
    })
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn data_looks_good() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"hello\r\nworld\r\n..\r\n", b"hello\r\nworld\r\n.\r\n"),
            (b"hello\r\nworld\r\n.. see ya\r\n", b"hello\r\nworld\r\n. see ya\r\n"),
            (b"hello\r\nworld\r\n .. see ya\r\n", b"hello\r\nworld\r\n .. see ya\r\n"),
            (b"hello\r\nworld\r\n ..\r\n", b"hello\r\nworld\r\n ..\r\n"),
        ];
        for &(s, r) in tests.into_iter() {
            let d = DataCommand { data: ActualData::Borrowing(s) };
            assert_eq!(d.data(), r);
        }
    }

    #[test]
    fn valid_command_data_args() {
        let tests = vec![
            (&b"  \r\nhello\r\nworld\r\n..\r\n.\r\n"[..], DataCommand {
                data: ActualData::Borrowing(b"hello\r\nworld\r\n..\r\n"),
            }),
            (&b" \t \r\n.\r\n"[..], DataCommand {
                data: ActualData::Borrowing(b""),
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_data_args(s), IResult::Done(&b""[..], r));
        }
    }

    #[test]
    fn valid_sending() {
        let mut v = Vec::new();
        unsafe { DataCommand::new_raw(b"hello\r\nworld\r\n") }.send_to(&mut v).unwrap();
        assert_eq!(v, b"DATA\r\nhello\r\nworld\r\n.\r\n");
    }

    #[test]
    fn valid_escaping() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"foo\r\n.\r\nbar\r\n", b"foo\r\n..\r\nbar\r\n"),
            (b"foo\r\nbar\r\n", b"foo\r\nbar\r\n"),
            (b"foo\r\nbar\r", b"foo\r\nbar\r\n"),
            (b"foo\r\nbar", b"foo\r\nbar\r\n"),
        ];
        for &(a, b) in tests {
            assert_eq!(DataCommand::new(a, 16).data, ActualData::Owned(b.to_owned()));
        }
    }
}
