use bytes::Bytes;
use tokio::prelude::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DataSinkState {
    Running,
    CrPassed,
    CrLfPassed,
}

pub struct DataSink<S: Sink<SinkItem = Bytes>> {
    sink:  S,
    state: DataSinkState,
}

impl<S: Sink<SinkItem = Bytes>> DataSink<S> {
    pub fn new(sink: S) -> DataSink<S> {
        DataSink {
            sink,
            state: DataSinkState::CrLfPassed,
        }
    }

    pub fn end(self) -> impl Future<Item = S, Error = S::SinkError> {
        use self::DataSinkState::*;
        let bytes = match self.state {
            Running => Bytes::from_static(b"\r\n.\r\n"),
            CrPassed => Bytes::from_static(b"\r\n.\r\n"),
            CrLfPassed => Bytes::from_static(b".\r\n"),
        };
        self.sink.send(bytes)
    }
}

impl<S: Sink<SinkItem = Bytes>> Sink for DataSink<S> {
    type SinkItem = Bytes;
    type SinkError = S::SinkError;

    fn start_send(&mut self, mut item: Bytes) -> Result<AsyncSink<Bytes>, Self::SinkError> {
        use self::DataSinkState::*;
        loop {
            let mut breakat = None;
            for (pos, c) in item.iter().enumerate() {
                match (self.state, c) {
                    (_, b'\r') => self.state = CrPassed,
                    (CrPassed, b'\n') => self.state = CrLfPassed,
                    (CrLfPassed, b'.') => {
                        self.state = Running;
                        breakat = Some(pos);
                        break;
                    }
                    (_, _) => self.state = Running,
                }
            }
            match breakat {
                None => return self.sink.start_send(item),
                Some(pos) => {
                    // Send everything until and including the '.'
                    if self.sink.start_send(item.slice_to(pos + 1))?.is_not_ready() {
                        return Ok(AsyncSink::NotReady(item));
                    }
                    // Now send all the remaining stuff by going through the loop again
                    // The escaping is done by the fact the '.' was already sent once, and yet left
                    // in `item` to be sent again.
                    item.advance(pos);
                }
            }
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.sink.poll_complete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_data_sink() {
        let tests: &[(&[&[u8]], &[u8])] = &[
            (&[b"foo", b" bar"], b"foo bar\r\n.\r\n"),
            (&[b""], b".\r\n"),
            (&[b"."], b"..\r\n.\r\n"),
            (&[b"foo\r"], b"foo\r\r\n.\r\n"),
            (&[b"foo bar\r", b"\n"], b"foo bar\r\n.\r\n"),
        ];
        for &(inp, out) in tests {
            let mut v = Vec::new();
            {
                let sink = DataSink::new(&mut v);
                sink.send_all(stream::iter_ok(inp.iter().map(|x| Bytes::from(*x))))
                    .wait()
                    .unwrap()
                    .0
                    .end()
                    .wait()
                    .unwrap();
            }
            assert_eq!(
                v.into_iter()
                    .flat_map(|x| x.into_iter())
                    .collect::<Vec<_>>(),
                out.to_vec()
            );
        }
    }
}
