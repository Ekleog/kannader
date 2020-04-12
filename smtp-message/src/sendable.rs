use std::io;

// TODO: (B) use everywhere send_to is currently defined
pub trait Sendable {
    fn send_to(&self, writer: &mut dyn io::Write) -> io::Result<()>;
}
