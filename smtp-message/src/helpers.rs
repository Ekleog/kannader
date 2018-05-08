use bytes::Bytes;
use nom::{self, IResult, Needed};
use std::{cmp::min, collections::HashMap, fmt, mem, ops::Deref, slice};
use tokio::prelude::*;

// TODO: grep for '::*' and try to rationalize imports
use byteslice::ByteSlice;
use parse_helpers::*;

// TODO: This file should not exist. Every function should find a better home.

#[derive(Fail, Debug, Clone)]
pub enum ParseError {
    DidNotConsumeEverything(usize),
    ParseError(#[cause] nom::Err),
    IncompleteString(Needed),
}

pub fn nom_to_result<T>(d: nom::IResult<ByteSlice, T>) -> Result<T, ParseError> {
    match d {
        IResult::Done(rem, res) => if rem.len() == 0 {
            Ok(res)
        } else {
            Err(ParseError::DidNotConsumeEverything(rem.len()))
        },
        IResult::Error(e) => Err(ParseError::ParseError(e)),
        IResult::Incomplete(n) => Err(ParseError::IncompleteString(n)),
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use ParseError::*;
        // TODO: make error display nicer with nom
        match self {
            &DidNotConsumeEverything(rem) => {
                write!(f, "Input contains {} trailing characters", rem)
            }
            &ParseError(ref err) => write!(f, "Parse error: {}", err),
            &IncompleteString(Needed::Unknown) => write!(f, "Input appears to be incomplete"),
            &IncompleteString(Needed::Size(sz)) => {
                write!(f, "Input appears to be missing {} characters", sz)
            }
        }
    }
}

#[derive(Fail, Debug, Clone)]
pub enum BuildError {
    LineTooLong { length: usize, limit:  usize },
    DisallowedByte { b:   u8, pos: usize },
    ContainsNewLine { pos: usize },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &BuildError::LineTooLong { length, limit } => {
                write!(f, "Line is too long: {} (max: {})", length, limit)
            }
            &BuildError::DisallowedByte { b, pos } => {
                write!(f, "Disallowed byte found at position {}: {}", pos, b)
            }
            &BuildError::ContainsNewLine { pos } => write!(f, "New line found at position {}", pos),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SmtpString(Bytes);

impl From<Bytes> for SmtpString {
    fn from(b: Bytes) -> SmtpString {
        SmtpString(b)
    }
}

// TODO: specialize for 'static or remove?
impl<'a> From<&'a [u8]> for SmtpString {
    fn from(b: &'a [u8]) -> SmtpString {
        SmtpString(Bytes::from(b))
    }
}

// TODO: specialize for 'static or remove?
impl<'a> From<&'a str> for SmtpString {
    fn from(s: &'a str) -> SmtpString {
        SmtpString(Bytes::from(s.as_bytes()))
    }
}

impl SmtpString {
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

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SpParameters(pub HashMap<SmtpString, Option<SmtpString>>);

#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct Domain(SmtpString); // TODO: split between IP and DNS

impl Domain {
    pub fn new(domain: ByteSlice) -> Result<Domain, ParseError> {
        nom_to_result(hostname(domain))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Domain, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(hostname(ByteSlice::from(&b)))
    }

    pub fn as_string(&self) -> &SmtpString {
        &self.0
    }
}

impl Deref for Domain {
    type Target = SmtpString;

    fn deref(&self) -> &SmtpString {
        &self.0
    }
}

pub fn new_domain_unchecked(s: SmtpString) -> Domain {
    Domain(s)
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct Email {
    localpart: SmtpString,
    hostname:  Option<Domain>,
}

impl Email {
    pub fn new(localpart: SmtpString, hostname: Option<Domain>) -> Email {
        Email {
            localpart,
            hostname,
        }
    }

    pub fn parse(b: ByteSlice) -> Result<Email, ParseError> {
        nom_to_result(email(b))
    }

    pub fn parse_slice(b: &[u8]) -> Result<Email, ParseError> {
        let b = Bytes::from(b);
        nom_to_result(email(ByteSlice::from(&b)))
    }

    pub fn raw_localpart(&self) -> &SmtpString {
        &self.localpart
    }

    // Note: this may contain unexpected characters, check RFC5321 / RFC5322 for
    // details.
    // This is a canonicalized version of the potentially quoted localpart, not
    // designed to be sent over the wire as it is no longer correctly quoted
    pub fn localpart(&self) -> SmtpString {
        if self.localpart.byte(0) != b'"' {
            self.localpart.clone()
        } else {
            #[derive(Copy, Clone)]
            enum State {
                Start,
                Backslash,
            }

            let mut res = self.localpart
                .iter_bytes()
                .skip(1)
                .scan(State::Start, |state, &x| match (*state, x) {
                    (State::Backslash, _) => {
                        *state = State::Start;
                        Some(Some(x))
                    }
                    (_, b'\\') => {
                        *state = State::Backslash;
                        Some(None)
                    }
                    (_, _) => {
                        *state = State::Start;
                        Some(Some(x))
                    }
                })
                .filter_map(|x| x)
                .collect::<Vec<u8>>();
            assert_eq!(res.pop().unwrap(), b'"');
            SmtpString::from(Bytes::from(res))
        }
    }

    pub fn hostname(&self) -> &Option<Domain> {
        &self.hostname
    }

    // TODO: actually store just the overall string and a pointer to the @, not two
    // separate fields
    pub fn as_string(&self) -> SmtpString {
        let mut res = self.localpart.bytes().clone();
        if let Some(ref host) = self.hostname {
            res.extend_from_slice(b"@");
            res.extend_from_slice(&host.as_string().bytes()[..]);
        }
        res.into()
    }
}

pub fn opt_email_repr(e: &Option<Email>) -> SmtpString {
    if let &Some(ref e) = e {
        e.as_string()
    } else {
        (&b""[..]).into()
    }
}

pub struct Prependable<S: Stream> {
    stream:    S,
    prepended: Option<S::Item>,
}

impl<S: Stream> Prependable<S> {
    pub fn prepend(&mut self, item: S::Item) -> Result<(), ()> {
        if self.prepended.is_some() {
            Err(())
        } else {
            self.prepended = Some(item);
            Ok(())
        }
    }
}

impl<S: Stream> Stream for Prependable<S> {
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        if let Some(item) = self.prepended.take() {
            Ok(Async::Ready(Some(item)))
        } else {
            self.stream.poll()
        }
    }
}

pub struct ConcatAndRecover<S: Stream>
where
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>,
{
    stream: Option<S>,
    extend: Option<S::Item>,
}

impl<S: Stream> Future for ConcatAndRecover<S>
where
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>,
{
    type Item = (S::Item, S);
    type Error = (S::Error, S);

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        let mut s = self.stream
            .take()
            .expect("attempted to poll ConcatAndRecover after completion");
        loop {
            match s.poll() {
                Ok(Async::Ready(Some(i))) => match self.extend {
                    None => self.extend = Some(i),
                    Some(ref mut e) => e.extend(i),
                },
                Ok(Async::Ready(None)) => match self.extend.take() {
                    None => return Ok(Async::Ready((Default::default(), s))),
                    Some(i) => return Ok(Async::Ready((i, s))),
                },
                Ok(Async::NotReady) => {
                    self.stream = Some(s);
                    return Ok(Async::NotReady);
                }
                Err(e) => return Err((e, s)),
            }
        }
    }
}

enum NextStep<S: Stream, F: Future, Acc> {
    Stream(S, Acc),
    Future(F),
    Completed,
}

pub struct FoldWithStream<S, Acc, Fun, Ret>
where
    S: Stream,
    Fun: FnMut(Acc, S::Item, S) -> Ret,
    Ret: Future<Item = (S, Acc), Error = S::Error>,
{
    next: NextStep<S, Ret, Acc>,
    f:    Fun,
}

impl<S, Acc, Fun, Ret> Future for FoldWithStream<S, Acc, Fun, Ret>
where
    S: Stream,
    Fun: FnMut(Acc, S::Item, S) -> Ret,
    Ret: Future<Item = (S, Acc), Error = S::Error>,
{
    type Item = Acc;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        loop {
            match mem::replace(&mut self.next, NextStep::Completed) {
                NextStep::Stream(mut s, acc) => match s.poll() {
                    Ok(Async::Ready(Some(i))) => {
                        self.next = NextStep::Future((self.f)(acc, i, s));
                    }
                    Ok(Async::Ready(None)) => return Ok(Async::Ready(acc)),
                    Ok(Async::NotReady) => {
                        self.next = NextStep::Stream(s, acc);
                        return Ok(Async::NotReady);
                    }
                    Err(e) => return Err(e),
                },
                NextStep::Future(mut f) => match f.poll() {
                    Ok(Async::Ready((s, acc))) => {
                        self.next = NextStep::Stream(s, acc);
                    }
                    Ok(Async::NotReady) => {
                        self.next = NextStep::Future(f);
                        return Ok(Async::NotReady);
                    }
                    Err(e) => return Err(e),
                },
                NextStep::Completed => panic!("attempted to poll FoldWithStream after completion"),
            }
        }
    }
}

pub trait StreamExt: Stream {
    fn prependable(self) -> Prependable<Self>
    where
        Self: Sized,
    {
        Prependable {
            stream:    self,
            prepended: None,
        }
    }

    fn concat_and_recover(self) -> ConcatAndRecover<Self>
    where
        Self: Sized,
        Self::Item: Default + IntoIterator + Extend<<Self::Item as IntoIterator>::Item>,
    {
        ConcatAndRecover {
            stream: Some(self),
            extend: None,
        }
    }

    fn fold_with_stream<Fun, Acc, Ret>(
        self,
        init: Acc,
        f: Fun,
    ) -> FoldWithStream<Self, Acc, Fun, Ret>
    where
        Self: Sized,
        Fun: FnMut(Acc, Self::Item, Self) -> Ret,
        Ret: Future<Item = (Self, Acc), Error = Self::Error>,
    {
        FoldWithStream {
            next: NextStep::Stream(self, init),
            f,
        }
    }
}

impl<T: ?Sized> StreamExt for T
where
    T: Stream,
{
}
