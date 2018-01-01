use std::fmt;

use nom::crlf;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct DataCommand<'a> {
    // Still SMTP-escaped (ie. leading ‘.’ doubled) message
    data: &'a [u8],
}

impl<'a> DataCommand<'a> {
    pub fn raw_data(&self) -> &'a [u8] {
        self.data
    }

    pub fn data(&self) -> Vec<u8> {
        #[derive(Copy, Clone)]
        enum State { Start, CrPassed, CrlfPassed };

        self.data.iter().scan(State::Start, |state, &x| {
            match (*state, x) {
                (_, b'\r')                => { *state = State::CrPassed;   Some(Some(x)) },
                (State::CrPassed, b'\n')  => { *state = State::CrlfPassed; Some(Some(x)) },
                (State::CrlfPassed, b'.') => { *state = State::Start;      Some(None   ) },
                _                         => { *state = State::Start;      Some(Some(x)) },
            }
        }).filter_map(|x| x).collect()
    }
}

impl<'a> fmt::Debug for DataCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "DataCommand {{ data: {} }}", bytes_to_dbg(self.data))
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
        data: data,
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
            let d = DataCommand { data: s };
            assert_eq!(d.data(), r);
        }
    }

    #[test]
    fn valid_command_data_args() {
        let tests = vec![
            (&b"  \r\nhello\r\nworld\r\n..\r\n.\r\n"[..], DataCommand {
                data: &b"hello\r\nworld\r\n..\r\n"[..],
            }),
            (&b" \t \r\n.\r\n"[..], DataCommand {
                data: &b""[..],
            }),
        ];
        for (s, r) in tests.into_iter() {
            assert_eq!(command_data_args(s), IResult::Done(&b""[..], r));
        }
    }
}
