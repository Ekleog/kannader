mod concatandrecover;
mod foldwithstream;
mod forwardnotclosing;
mod prependable;

use tokio::prelude::*;

pub use self::{
    concatandrecover::ConcatAndRecover, foldwithstream::FoldWithStream,
    forwardnotclosing::ForwardNotClosing, prependable::Prependable,
};

pub trait StreamExt: Stream {
    fn prependable(self) -> Prependable<Self>
    where
        Self: Sized,
    {
        Prependable::new(self)
    }

    fn concat_and_recover(self) -> ConcatAndRecover<Self>
    where
        Self: Sized,
        Self::Item: Default + IntoIterator + Extend<<Self::Item as IntoIterator>::Item>,
    {
        ConcatAndRecover::new(self)
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
        FoldWithStream::new(self, init, f)
    }

    fn forward_not_closing<S>(self, sink: S) -> ForwardNotClosing<Self, S>
    where
        Self: Sized,
        S: Sink<SinkItem = Self::Item>,
        Self::Error: From<S::SinkError>,
    {
        self::forwardnotclosing::new(self, sink)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
