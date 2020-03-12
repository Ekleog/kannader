use std::pin::Pin;

use futures::prelude::*;
use itertools::Itertools;

use smtp_message::{IsLastLine, ReplyCode, ReplyLine, SmtpString};

// TODO: (B) move to smtp_message's Reply builder id:tcHW
// Panics if `text` has a byte not in {9} \union [32; 126]
// TODO: (B) move sending logic to smtp_message::Reply
pub async fn send_reply<W>(
    mut writer: Pin<&mut W>,
    (code, text): (ReplyCode, SmtpString),
) -> Result<(), W::SinkError>
where
    W: Sink<ReplyLine>,
{
    let replies = text
        .byte_chunks(ReplyLine::MAX_LEN)
        .with_position()
        .map(move |t| {
            use itertools::Position::*;
            match t {
                First(t) | Middle(t) => ReplyLine::build(code, IsLastLine::No, t).unwrap(),
                Last(t) | Only(t) => ReplyLine::build(code, IsLastLine::Yes, t).unwrap(),
            }
        });
    let mut reply_stream = stream::iter(replies);

    await!(writer.send_all(&mut reply_stream))?;

    Ok(())
}
