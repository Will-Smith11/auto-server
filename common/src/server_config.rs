use std::time::Duration;

use tokio::net::TcpListener;

pub struct ServerConfig
{
    pub listener: TcpListener,
    pub timeout:  Duration,
}
