pub mod client_config;
pub mod server_config;

pub enum PollState
{
    Ready,
    Send,
    Flush,
}

impl PollState
{
    pub fn is_ready(&self) -> bool
    {
        matches!(self, Self::Ready)
    }

    pub fn is_send(&self) -> bool
    {
        matches!(self, Self::Send)
    }

    pub fn is_flush(&self) -> bool
    {
        matches!(self, Self::Flush)
    }
}
