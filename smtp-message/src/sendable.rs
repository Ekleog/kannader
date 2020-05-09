use std::io;

pub trait Sendable {
    fn send_to(&self, writer: &mut dyn io::Write) -> io::Result<()>;
}
