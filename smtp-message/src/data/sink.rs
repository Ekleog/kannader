use bytes::{Buf, Bytes};
use futures::prelude::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DataSinkState {
    Running,
    CrPassed,
    CrLfPassed,
}

pub struct DataSink<S> {
    sink: S,
    state: DataSinkState,
}

impl<S: Sink<Bytes> + Unpin> DataSink<S> {
    pub fn new(sink: S) -> DataSink<S> {
        DataSink {
            sink,
            state: DataSinkState::CrLfPassed,
        }
    }

    pub async fn send(&mut self, mut item: Bytes) -> Result<(), S::Error> {
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
                None => {
                    self.sink.send(item).await?;
                    return Ok(());
                }
                Some(pos) => {
                    // Send everything until and including the '.'
                    self.sink.send(item.slice(0..(pos + 1))).await?;
                    // Now send all the remaining stuff by going through the loop again
                    // The escaping is done by the fact the '.' was already sent once, and yet left
                    // in `item` to be sent again.
                    item.advance(pos);
                }
            }
        }
    }

    pub async fn end(mut self) -> Result<S, S::Error> {
        use self::DataSinkState::*;
        let bytes = match self.state {
            Running => Bytes::from_static(b"\r\n.\r\n"),
            CrPassed => Bytes::from_static(b"\r\n.\r\n"),
            CrLfPassed => Bytes::from_static(b".\r\n"),
        };
        self.sink.send(bytes).await?;
        Ok(self.sink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;

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
                let mut sink = DataSink::new(&mut v);
                block_on(async {
                    for i in inp.iter() {
                        sink.send(Bytes::from(*i)).await.unwrap();
                    }
                    sink.end().await.unwrap();
                });
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
