use itertools::Itertools;
use smtp_message::{IsLastLine, ReplyCode, ReplyLine, SmtpString};
use tokio::prelude::*;

// TODO(low): move to smtp_message::Reply builder
// Panics if `text` has a byte not in {9} \union [32; 126]
pub fn send_reply<'a, W>(
    writer: W,
    (code, text): (ReplyCode, SmtpString),
) -> impl Future<Item = W, Error = W::SinkError> + 'a
where
    W: 'a + Sink<SinkItem = ReplyLine>,
    W::SinkError: 'a,
{
    let replies = text.byte_chunks(ReplyLine::MAX_LEN)
        .with_position()
        .map(move |t| {
            use itertools::Position::*;
            match t {
                First(t) | Middle(t) => ReplyLine::build(code, IsLastLine::No, t).unwrap(),
                Last(t) | Only(t) => ReplyLine::build(code, IsLastLine::Yes, t).unwrap(),
            }
        });
    // TODO: do not use send_all as it closes the writer, use start_send and
    // poll_complete instead (or even refactor to move this logic into
    // smtp_message::ReplyLine?)
    writer.send_all(stream::iter_ok(replies)).map(|(w, _)| w)
}
