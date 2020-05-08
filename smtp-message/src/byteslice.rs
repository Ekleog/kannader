use bytes::Bytes;
use nom::{Compare, CompareResult, FindSubstring, InputIter, InputLength, Slice};
use std::{
    cmp::PartialEq,
    iter::{Enumerate, Iterator, Map},
    ops::{Deref, Range, RangeFrom, RangeFull, RangeTo},
    slice, str,
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ByteSlice<'a> {
    buf: &'a Bytes,
    start: usize,
    end: usize,
}

impl<'a> From<&'a Bytes> for ByteSlice<'a> {
    fn from(b: &'a Bytes) -> ByteSlice<'a> {
        ByteSlice {
            buf: b,
            start: 0,
            end: b.len(),
        }
    }
}

impl<'a> ByteSlice<'a> {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn promote(&self) -> Bytes {
        self.buf.slice(self.start..self.end)
    }

    pub fn demote(self) -> &'a [u8] {
        &self.buf[self.start..self.end]
    }

    pub fn into_utf8(self) -> Result<&'a str, str::Utf8Error> {
        str::from_utf8(self.demote())
    }
}

impl<'a> Deref for ByteSlice<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }
}

impl<'a> PartialEq for ByteSlice<'a> {
    fn eq(&self, other: &ByteSlice<'a>) -> bool {
        let bufs = (self.buf as *const Bytes) == (other.buf as *const Bytes);
        self.start == other.start && self.end == other.end && bufs
    }
}

impl<'a> Slice<Range<usize>> for ByteSlice<'a> {
    fn slice(&self, range: Range<usize>) -> Self {
        assert!(range.start <= range.end && range.end <= self.end);
        ByteSlice {
            buf: self.buf,
            start: self.start + range.start,
            end: self.start + range.end,
        }
    }
}

impl<'a> Slice<RangeTo<usize>> for ByteSlice<'a> {
    fn slice(&self, range: RangeTo<usize>) -> Self {
        self.slice(0..range.end)
    }
}

impl<'a> Slice<RangeFrom<usize>> for ByteSlice<'a> {
    fn slice(&self, range: RangeFrom<usize>) -> Self {
        self.slice(range.start..self.end - self.start)
    }
}

impl<'a> Slice<RangeFull> for ByteSlice<'a> {
    fn slice(&self, _: RangeFull) -> Self {
        self.clone()
    }
}

impl<'a> InputIter for ByteSlice<'a> {
    type Item = u8;
    type Iter = Enumerate<Self::IterElem>;
    type IterElem = Map<slice::Iter<'a, Self::Item>, fn(&u8) -> u8>;
    type RawItem = u8;

    fn iter_indices(&self) -> Self::Iter {
        self.iter_elements().enumerate()
    }

    fn iter_elements(&self) -> Self::IterElem {
        self.buf[self.start..self.end]
            .iter()
            .map((|x| *x) as fn(&u8) -> u8)
    }

    fn position<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::RawItem) -> bool,
    {
        self.buf[self.start..self.end]
            .iter()
            .position(|b| predicate(*b))
    }

    fn slice_index(&self, count: usize) -> Option<usize> {
        if self.end - self.start >= count {
            Some(count)
        } else {
            None
        }
    }
}

impl<'a> InputLength for ByteSlice<'a> {
    fn input_len(&self) -> usize {
        self.end - self.start
    }
}

impl<'a, T> Compare<T> for ByteSlice<'a>
where
    &'a [u8]: Compare<T>,
{
    fn compare(&self, t: T) -> CompareResult {
        (&self.buf[self.start..self.end]).compare(t)
    }

    fn compare_no_case(&self, t: T) -> CompareResult {
        (&self.buf[self.start..self.end]).compare_no_case(t)
    }
}

impl<'a, T> FindSubstring<T> for ByteSlice<'a>
where
    &'a [u8]: FindSubstring<T>,
{
    fn find_substring(&self, t: T) -> Option<usize> {
        (&self.buf[self.start..self.end]).find_substring(t)
    }
}
