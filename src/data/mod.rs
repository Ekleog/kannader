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

#[cfg_attr(test, derive(PartialEq))]
pub struct DataCommand<'a> {
    data: ActualData<'a>,
}

impl<'a> DataCommand<'a> {
    pub unsafe fn new_raw(data: &[u8]) -> DataCommand {
        DataCommand { data: ActualData::Borrowing(data) }
    }

    pub fn raw_data(&self) -> &[u8] {
        self.data.get()
    }

    pub fn data(&self) -> Vec<u8> {
        #[derive(Copy, Clone)]
        enum State { Start, CrPassed, CrlfPassed };

        self.data.get().iter().scan(State::Start, |state, &x| {
            match (*state, x) {
                (_, b'\r')                => { *state = State::CrPassed;   Some(Some(x)) },
                (State::CrPassed, b'\n')  => { *state = State::CrlfPassed; Some(Some(x)) },
                (State::CrlfPassed, b'.') => { *state = State::Start;      Some(None   ) },
                _                         => { *state = State::Start;      Some(Some(x)) },
            }
        }).filter_map(|x| x).collect()
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"DATA\r\n")?;
        w.write_all(self.data.get())?;
        w.write_all(b".\r\n")
    }
}

impl<'a> fmt::Debug for DataCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "DataCommand {{ data: {} }}", bytes_to_dbg(self.data.get()))
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
}
