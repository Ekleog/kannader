mod prependable;

use futures::prelude::*;

pub use self::prependable::Prependable;

pub trait StreamExt: Stream {
    fn prependable(self) -> Prependable<Self>
    where
        Self: Sized,
    {
        Prependable::new(self)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
