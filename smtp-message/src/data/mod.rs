use std::io;

use nom::crlf;

use helpers::*;
use parse_helpers::*;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct DataCommand {
    _useless: (),
}

impl DataCommand {
    // SMTP-escapes (ie. doubles leading ‘.’) messages first
    pub fn new() -> DataCommand {
        DataCommand { _useless: () }
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        w.write_all(b"DATA\r\n")
    }
}

named!(pub command_data_args(&[u8]) -> DataCommand, do_parse!(
    eat_spaces >> crlf >>
    (DataCommand { _useless: () })
));

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub enum DataLine<'a> {
    Line(&'a [u8]),
    Eof,
}

impl<'a> DataLine<'a> {
    pub fn new(line: &[u8]) -> Result<DataLine, BuildError> {
        if let Some(pos) = line.windows(2).position(|x| x == b"\r\n") {
            Err(BuildError::ContainsNewLine { pos })
        } else {
            Ok(DataLine::Line(line))
        }
    }

    pub fn parse(raw: &[u8]) -> Result<DataLine, ParseError> {
        nom_to_result(parse_data_line(raw))
    }

    pub fn send_to(&self, w: &mut io::Write) -> io::Result<()> {
        match self {
            &DataLine::Line(l) => {
                if l.len() > 0 && l[0] == b'.' {
                    w.write(b".")?;
                }
                w.write_all(l)?;
                w.write_all(b"\r\n")
            }
            &DataLine::Eof => w.write_all(b".\r\n"),
        }
    }
}

named!(parse_data_line(&[u8]) -> DataLine, alt!(
    value!(DataLine::Eof, tag!(".\r\n")) |
    map!(
        preceded!(opt!(tag!(".")), take_until_and_consume!("\r\n")),
        DataLine::Line
    )
));

#[cfg(test)]
mod tests {
    use super::*;
    use nom::*;

    #[test]
    fn valid_command_data_args() {
        let tests = vec![&b" \t  \t \r\n"[..], &b"\r\n"[..]];
        for test in tests.into_iter() {
            assert_eq!(
                command_data_args(test),
                IResult::Done(&b""[..], DataCommand { _useless: () })
            );
        }
    }

    #[test]
    fn valid_command_data_build() {
        let mut v = Vec::new();
        DataCommand::new().send_to(&mut v).unwrap();
        assert_eq!(v, b"DATA\r\n");
    }

    #[test]
    fn valid_data_line_build() {
        assert_eq!(DataLine::new(b"").unwrap(), DataLine::Line(b""));
        assert_eq!(DataLine::new(b"foo bar").unwrap(), DataLine::Line(b"foo bar"));
        assert!(DataLine::new(b"foo\r\nbar").is_err());
    }

    #[test]
    fn valid_data_line_parse() {
        assert_eq!(DataLine::parse(b"foo bar\r\n").unwrap(), DataLine::Line(b"foo bar"));
        assert_eq!(DataLine::parse(b"\r\n").unwrap(), DataLine::Line(b""));
        assert_eq!(DataLine::parse(b".baz\r\n").unwrap(), DataLine::Line(b"baz"));
        assert_eq!(DataLine::parse(b".\r\n").unwrap(), DataLine::Eof);
        assert_eq!(DataLine::parse(b" .baz\r\n").unwrap(), DataLine::Line(b" .baz"));
    }

    #[test]
    fn valid_data_line_send() {
        let tests: &[(&[u8], &[u8])] = &[
            (b"foo bar", b"foo bar\r\n"),
            (b"", b"\r\n"),
            (b".", b"..\r\n"),
        ];
        let mut v = Vec::new();
        DataLine::Eof.send_to(&mut v).unwrap();
        assert_eq!(v, b".\r\n");
        for &(l, r) in tests {
            v.clear();
            DataLine::Line(l).send_to(&mut v).unwrap();
            assert_eq!(v, r);
        }
    }
}
