use std::mem;
use tokio::prelude::*;

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

impl<S, Acc, Fun, Ret> FoldWithStream<S, Acc, Fun, Ret>
where
    S: Stream,
    Fun: FnMut(Acc, S::Item, S) -> Ret,
    Ret: Future<Item = (S, Acc), Error = S::Error>,
{
    pub fn new(s: S, init: Acc, f: Fun) -> FoldWithStream<S, Acc, Fun, Ret> {
        FoldWithStream {
            next: NextStep::Stream(s, init),
            f,
        }
    }
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
