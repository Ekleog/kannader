use tokio::prelude::*;

pub enum FutIn9<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9>
where
    F1: Future<Item = T, Error = E>,
    F2: Future<Item = T, Error = E>,
    F3: Future<Item = T, Error = E>,
    F4: Future<Item = T, Error = E>,
    F5: Future<Item = T, Error = E>,
    F6: Future<Item = T, Error = E>,
    F7: Future<Item = T, Error = E>,
    F8: Future<Item = T, Error = E>,
    F9: Future<Item = T, Error = E>,
{
    Fut1(F1),
    Fut2(F2),
    Fut3(F3),
    Fut4(F4),
    Fut5(F5),
    Fut6(F6),
    Fut7(F7),
    Fut8(F8),
    Fut9(F9),
}

impl<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9> Future
    for FutIn9<T, E, F1, F2, F3, F4, F5, F6, F7, F8, F9>
where
    F1: Future<Item = T, Error = E>,
    F2: Future<Item = T, Error = E>,
    F3: Future<Item = T, Error = E>,
    F4: Future<Item = T, Error = E>,
    F5: Future<Item = T, Error = E>,
    F6: Future<Item = T, Error = E>,
    F7: Future<Item = T, Error = E>,
    F8: Future<Item = T, Error = E>,
    F9: Future<Item = T, Error = E>,
{
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        use self::FutIn9::*;
        match *self {
            Fut1(ref mut f) => f.poll(),
            Fut2(ref mut f) => f.poll(),
            Fut3(ref mut f) => f.poll(),
            Fut4(ref mut f) => f.poll(),
            Fut5(ref mut f) => f.poll(),
            Fut6(ref mut f) => f.poll(),
            Fut7(ref mut f) => f.poll(),
            Fut8(ref mut f) => f.poll(),
            Fut9(ref mut f) => f.poll(),
        }
    }
}
