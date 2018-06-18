// This is a copy-paste of futures::stream::Forward
// The only change is it doesn't close the stream
// (and it has been `cargo fmt`'d to this repo's conventions)

use tokio::prelude::{stream::Fuse, *};

macro_rules! try_ready {
    ($e:expr) => {
        match $e {
            Ok(Async::Ready(t)) => t,
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(e) => return Err(From::from(e)),
        }
    };
}

/// Future for the `Stream::forward` combinator, which sends a stream of values
/// to a sink and then waits until the sink has fully flushed those values.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct ForwardNotClosing<T: Stream, U> {
    sink:     Option<U>,
    stream:   Option<Fuse<T>>,
    buffered: Option<T::Item>,
}

pub fn new<T, U>(stream: T, sink: U) -> ForwardNotClosing<T, U>
where
    U: Sink<SinkItem = T::Item>,
    T: Stream,
    T::Error: From<U::SinkError>,
{
    ForwardNotClosing {
        sink:     Some(sink),
        stream:   Some(stream.fuse()),
        buffered: None,
    }
}

impl<T, U> ForwardNotClosing<T, U>
where
    U: Sink<SinkItem = T::Item>,
    T: Stream,
    T::Error: From<U::SinkError>,
{
    /// Get a shared reference to the inner sink.
    /// If this combinator has already been polled to completion, None will be
    /// returned.
    pub fn sink_ref(&self) -> Option<&U> {
        self.sink.as_ref()
    }

    /// Get a mutable reference to the inner sink.
    /// If this combinator has already been polled to completion, None will be
    /// returned.
    pub fn sink_mut(&mut self) -> Option<&mut U> {
        self.sink.as_mut()
    }

    /// Get a shared reference to the inner stream.
    /// If this combinator has already been polled to completion, None will be
    /// returned.
    pub fn stream_ref(&self) -> Option<&T> {
        self.stream.as_ref().map(|x| x.get_ref())
    }

    /// Get a mutable reference to the inner stream.
    /// If this combinator has already been polled to completion, None will be
    /// returned.
    pub fn stream_mut(&mut self) -> Option<&mut T> {
        self.stream.as_mut().map(|x| x.get_mut())
    }

    fn take_result(&mut self) -> (T, U) {
        let sink = self
            .sink
            .take()
            .expect("Attempted to poll Forward after completion");
        let fuse = self
            .stream
            .take()
            .expect("Attempted to poll Forward after completion");
        (fuse.into_inner(), sink)
    }

    fn try_start_send(&mut self, item: T::Item) -> Poll<(), U::SinkError> {
        debug_assert!(self.buffered.is_none());
        if let AsyncSink::NotReady(item) = self
            .sink_mut()
            .take()
            .expect("Attempted to poll Forward after completion")
            .start_send(item)?
        {
            self.buffered = Some(item);
            return Ok(Async::NotReady);
        }
        Ok(Async::Ready(()))
    }
}

impl<T, U> Future for ForwardNotClosing<T, U>
where
    U: Sink<SinkItem = T::Item>,
    T: Stream,
    T::Error: From<U::SinkError>,
{
    type Item = (T, U);
    type Error = T::Error;

    fn poll(&mut self) -> Poll<(T, U), T::Error> {
        // If we've got an item buffered already, we need to write it to the
        // sink before we can do anything else
        if let Some(item) = self.buffered.take() {
            try_ready!(self.try_start_send(item))
        }

        loop {
            match self
                .stream_mut()
                .take()
                .expect("Attempted to poll Forward after completion")
                .poll()?
            {
                Async::Ready(Some(item)) => try_ready!(self.try_start_send(item)),
                Async::Ready(None) => {
                    try_ready!(
                        self.sink_mut()
                            .take()
                            .expect("Attempted to poll Forward after completion")
                            .poll_complete()
                    );
                    return Ok(Async::Ready(self.take_result()));
                }
                Async::NotReady => {
                    try_ready!(
                        self.sink_mut()
                            .take()
                            .expect("Attempted to poll Forward after completion")
                            .poll_complete()
                    );
                    return Ok(Async::NotReady);
                }
            }
        }
    }
}
