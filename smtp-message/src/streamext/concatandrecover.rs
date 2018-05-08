use tokio::prelude::*;

pub struct ConcatAndRecover<S: Stream>
where
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>,
{
    stream: Option<S>,
    extend: Option<S::Item>,
}

impl<S: Stream> ConcatAndRecover<S>
where
    S::Item: Default + IntoIterator + Extend<<S::Item as IntoIterator>::Item>,
{
    pub fn new(s: S) -> ConcatAndRecover<S> {
        ConcatAndRecover {
            stream: Some(s),
            extend: None,
        }
    }
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
