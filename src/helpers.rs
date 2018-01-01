use std::str;

pub fn bytes_to_dbg(b: &[u8]) -> String {
    if let Ok(s) = str::from_utf8(b) {
        format!("b\"{}\"", s.chars().flat_map(|x| x.escape_default()).collect::<String>())
    } else {
        format!("{:?}", b)
    }
}
