use std::{pin::Pin, task::Context};

use futures::{prelude::*, task::Poll};

pub struct Prependable<S: Stream> {
    stream: S,
    prepended: Option<S::Item>,
}

impl<S: Stream> Prependable<S> {
    pub fn new(s: S) -> Prependable<S> {
        Prependable {
            stream: s,
            prepended: None,
        }
    }

    pub fn prepend(self: &mut Self, item: S::Item) -> Result<(), ()> {
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

    fn poll_next(mut self: Pin<&mut Self>, lw: &mut Context) -> Poll<Option<S::Item>> {
        // As `self.prepended` is never taken out of a `Pin<>`, this
        // should be OK, but that would definitely need checking. If
        // it weren't, `S::Item` would have to be asserted `Unpin`.
        let try_take = unsafe { self.as_mut().get_unchecked_mut().prepended.take() };
        if let Some(item) = try_take {
            Poll::Ready(Some(item))
        } else {
            // See the documentation of the `std::pin` module for why this is safe.
            unsafe {
                self.as_mut()
                    .map_unchecked_mut(|s| &mut s.stream)
                    .poll_next(lw)
            }
        }
    }
}
