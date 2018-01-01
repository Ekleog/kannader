use std::fmt;

use helpers::*;

#[cfg_attr(test, derive(PartialEq))]
pub struct Reply<'a> {
    num: u16,
    lines: &'a[&'a [u8]]
}

impl<'a> Reply<'a> {
    // Panics if “num” is an invalid code for SMTP
    pub fn freeform<'b>(num: u16, lines: &'b [&'b [u8]]) -> Reply<'b> {
        assert!(2 <= (num / 100) && (num / 100) <= 5,
                "Invalid reply code: {}", num);
        Reply { num, lines }
    }
}

impl<'a> fmt::Debug for Reply<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let mut res = "[".to_owned();
        for i in 0..self.lines.len() {
            res += &bytes_to_dbg(self.lines[i]);
            if i != self.lines.len() - 1 { res += ", " }
        }
        res += "]";
        write!(f, "Reply {{ num: {}, lines: {} }}", self.num, res)
    }
}

pub fn build(r: &Reply) -> Vec<u8> {
    let mut res = Vec::new();
    let code = &[((r.num % 1000) / 100) as u8 + b'0',
                 ((r.num % 100 ) / 10 ) as u8 + b'0',
                 ((r.num % 10  )      ) as u8 + b'0'];
    for i in 0..(r.lines.len() - 1) {
        res.extend_from_slice(code);
        res.push(b'-');
        res.extend_from_slice(r.lines[i]);
        res.extend_from_slice(b"\r\n");
    }
    res.extend_from_slice(code);
    res.push(b' ');
    if let Some(last) = r.lines.last() {
        res.extend_from_slice(last);
    }
    res.extend_from_slice(b"\r\n");
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeform_multiline() {
        let text: &[&[u8]] = &[b"hello", b"world", b"!"];
        let r = Reply::freeform(254, text);
        assert_eq!(r, Reply { num: 254, lines: text });
        assert_eq!(build(&r), b"254-hello\r\n254-world\r\n254 !\r\n");
    }

    #[test]
    fn freeform_oneline() {
        let text: &[&[u8]] = &[b"test"];
        let r = Reply::freeform(521, text);
        assert_eq!(r, Reply { num: 521, lines: text });
        assert_eq!(build(&r), b"521 test\r\n");
    }

    #[test] #[should_panic(expected = "Invalid reply code: 123")]
    fn freeform_invalid_too_low() {
        Reply::freeform(123, &[]);
    }

    #[test] #[should_panic(expected = "Invalid reply code: 678")]
    fn freeform_invalid_too_high() {
        Reply::freeform(678, &[]);
    }
}
