use bytes::Bytes;
use std::{cmp::min, io, ops::Add, slice};

use crate::sendable::Sendable;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SmtpString(Bytes);

impl From<Bytes> for SmtpString {
    fn from(b: Bytes) -> SmtpString {
        SmtpString(b)
    }
}

impl From<Vec<u8>> for SmtpString {
    fn from(b: Vec<u8>) -> SmtpString {
        SmtpString(Bytes::from(b))
    }
}

// TODO: (C) specialize for 'static or remove?
impl<'a> From<&'a [u8]> for SmtpString {
    fn from(b: &'a [u8]) -> SmtpString {
        SmtpString(Bytes::from(b))
    }
}

// TODO: (C) specialize for 'static or remove?
impl<'a> From<&'a str> for SmtpString {
    fn from(s: &'a str) -> SmtpString {
        SmtpString(Bytes::from(s.as_bytes()))
    }
}

impl SmtpString {
    pub fn from_static(b: &'static [u8]) -> SmtpString {
        SmtpString(Bytes::from_static(b))
    }

    // TODO: (C) add hint for length in Sendable
    pub fn from_sendable<T: Sendable>(t: &T) -> io::Result<SmtpString> {
        let mut v = Vec::new();
        t.send_to(&mut v)?;
        Ok(SmtpString::from(v))
    }

    pub fn iter_bytes(&self) -> slice::Iter<u8> {
        self.0.iter()
    }

    pub fn byte_len(&self) -> usize {
        self.0.len()
    }

    pub fn byte(&self, pos: usize) -> u8 {
        self.0[pos]
    }

    pub fn bytes(&self) -> &Bytes {
        &self.0
    }

    pub fn byte_chunks(&self, bytes: usize) -> impl Iterator<Item = SmtpString> {
        let copy = self.0.clone();
        (0..(self.byte_len() + bytes - 1) / bytes)
            .map(move |i| SmtpString(copy.slice(i * bytes, min(copy.len(), (i + 1) * bytes))))
    }
}

impl Sendable for SmtpString {
    fn send_to(&self, w: &mut dyn io::Write) -> io::Result<()> {
        w.write_all(&self.0[..])
    }
}

// TODO: (C) either remove or optimize
impl Add<SmtpString> for SmtpString {
    type Output = SmtpString;

    fn add(mut self, rhs: SmtpString) -> SmtpString {
        self.0.extend_from_slice(&rhs.0);
        self
    }
}
