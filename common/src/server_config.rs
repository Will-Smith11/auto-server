use std::time::Duration;

use tokio::net::TcpListener;

#[derive(Debug)]
pub struct ServerConfig
{
    pub listener: TcpListener,
    pub timeout:  Duration,
}
