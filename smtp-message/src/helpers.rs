use nom::{self, IResult, Needed};
use std::{borrow::Cow, fmt, ops::Deref, slice};
use tokio::prelude::*;

use parse_helpers::*;

#[derive(Fail, Debug, Clone)]
pub enum ParseError {
    DidNotConsumeEverything(usize),
    ParseError(#[cause] nom::Err),
    IncompleteString(Needed),
}

pub fn nom_to_result<'a, T>(d: nom::IResult<&'a [u8], T>) -> Result<T, ParseError> {
    match d {
        IResult::Done(b"", res) => Ok(res),
        IResult::Done(rem, _) => Err(ParseError::DidNotConsumeEverything(rem.len())),
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

#[derive(Eq, Hash, PartialEq)]
pub struct SmtpString<'a>(Cow<'a, [u8]>);

impl<'a> From<&'a [u8]> for SmtpString<'a> {
    fn from(t: &'a [u8]) -> SmtpString<'a> {
        SmtpString(Cow::from(t))
    }
}

impl<'a> From<Vec<u8>> for SmtpString<'a> {
    fn from(t: Vec<u8>) -> SmtpString<'a> {
        SmtpString(Cow::from(t))
    }
}

impl<'a> From<&'a str> for SmtpString<'a> {
    fn from(s: &'a str) -> SmtpString<'a> {
        SmtpString(Cow::from(s.as_bytes()))
    }
}

impl<'a> From<String> for SmtpString<'a> {
    fn from(s: String) -> SmtpString<'a> {
        SmtpString(Cow::from(s.into_bytes()))
    }
}

impl<'a> SmtpString<'a> {
    pub fn take_ownership<'b>(self) -> SmtpString<'b> {
        SmtpString(Cow::from(self.0.into_owned()))
    }

    pub fn borrow<'b>(&'b self) -> SmtpString<'b>
    where
        'a: 'b,
    {
        SmtpString(Cow::from(&*self.0))
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

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_owned()
    }

    pub fn copy_chunks<'b>(&self, bytes: usize) -> Vec<SmtpString<'b>> {
        let mut res = Vec::with_capacity((self.byte_len() + bytes - 1) / bytes);
        let mut it = self.0.iter().cloned();
        for _ in 0..((self.byte_len() + bytes - 1) / bytes) {
            res.push(it.by_ref().take(bytes).collect::<Vec<_>>().into());
        }
        res
    }
}

impl<'a> fmt::Debug for SmtpString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "b\"{}\"",
            self.0
                .iter()
                .flat_map(|x| char::from(*x).escape_default())
                .collect::<String>()
        )
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct Domain<'a>(SmtpString<'a>); // TODO: split between IP and DNS

impl<'a> Domain<'a> {
    pub fn new(domain: SmtpString) -> Result<Domain, ParseError> {
        nom_to_result(hostname(domain.as_bytes()))?;
        Ok(Domain(domain))
    }

    pub fn take_ownership<'b>(self) -> Domain<'b> {
        Domain(self.0.take_ownership())
    }

    pub fn borrow<'b>(&'b self) -> Domain<'b>
    where
        'a: 'b,
    {
        Domain(self.0.borrow())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }
}

pub fn new_domain_exclusively_for_parse_helpers_do_not_use(domain: SmtpString) -> Domain {
    Domain(domain)
}

impl<'a> Deref for Domain<'a> {
    type Target = SmtpString<'a>;

    fn deref(&self) -> &SmtpString<'a> {
        &self.0
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct Email<'a> {
    localpart: SmtpString<'a>,
    hostname:  Option<Domain<'a>>, // TODO: use Domain here
}

impl<'a> Email<'a> {
    pub fn new<'b>(localpart: SmtpString<'b>, hostname: Option<Domain<'b>>) -> Email<'b> {
        Email {
            localpart,
            hostname,
        }
    }

    pub fn parse<'b>(s: &'b SmtpString<'b>) -> Result<Email<'b>, ParseError> {
        nom_to_result(email(s.as_bytes()))
    }

    pub fn take_ownership<'b>(self) -> Email<'b> {
        Email {
            localpart: self.localpart.take_ownership(),
            hostname:  self.hostname.map(|x| x.take_ownership()),
        }
    }

    pub fn borrow<'b>(&'b self) -> Email<'b>
    where
        'a: 'b,
    {
        Email {
            localpart: self.localpart.borrow(),
            hostname:  self.hostname.as_ref().map(|x| x.borrow()),
        }
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
            self.localpart.borrow()
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
            res.into()
        }
    }

    pub fn hostname(&self) -> &Option<Domain> {
        &self.hostname
    }

    pub fn into_smtp_string<'b>(self) -> SmtpString<'b> {
        let mut res = self.localpart.into_bytes();
        if let Some(host) = self.hostname {
            res.push(b'@');
            res.extend_from_slice(host.as_bytes());
        }
        res.into()
    }

    pub fn as_smtp_string<'b>(&self) -> SmtpString<'b> {
        let mut res = self.localpart.borrow().into_bytes();
        if let Some(host) = self.hostname.as_ref().map(|x| x.borrow()) {
            res.push(b'@');
            res.extend_from_slice(host.as_bytes());
        }
        res.into()
    }
}

pub fn opt_email_repr<'a>(e: &Option<Email>) -> SmtpString<'a> {
    if let &Some(ref e) = e {
        e.as_smtp_string()
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
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>
{
    stream: Option<S>,
    extend: Option<S::Item>,
}

impl<S: Stream> Future for ConcatAndRecover<S>
where
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>
{
    type Item = (S::Item, S);
    type Error = (S::Error, S);

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        let mut s = self.stream.take().expect("attempted to poll ConcatAndRecover after result");
        loop {
            match s.poll() {
                Ok(Async::Ready(Some(i))) => {
                    match self.extend {
                        None => self.extend = Some(i),
                        Some(ref mut e) => e.extend(i),
                    }
                }
                Ok(Async::Ready(None)) => {
                    match self.extend.take() {
                        None => return Ok(Async::Ready((Default::default(), s))),
                        Some(i) => return Ok(Async::Ready((i, s))),
                    }
                }
                Ok(Async::NotReady) => {
                    self.stream = Some(s);
                    return Ok(Async::NotReady);
                }
                Err(e) => return Err((e, s)),
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
}

impl<T: ?Sized> StreamExt for T
where
    T: Stream,
{
}
