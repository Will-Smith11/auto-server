use std::pin::Pin;

use futures::{SinkExt, StreamExt};

auto_server_macros::server! {
    #[server(MemesServer)]
    pub enum MemesMessage{
        Field1(String),
        Field2(u8),
    }

    #[client(ExampleCleint)]
    pub enum ClientMsg {
        Field1,
        Field2,
    }
}
