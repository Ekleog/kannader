use tokio::prelude::*;

pub struct Prependable<S: Stream> {
    stream:    S,
    prepended: Option<S::Item>,
}

impl<S: Stream> Prependable<S> {
    pub fn new(s: S) -> Prependable<S> {
        Prependable {
            stream: s,
            prepended: None,
        }
    }

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
