// macros::client_server! {
//     #[server(ExampleServer)]
//     pub enum SeverMsg {
//         Field1(String),
//         Field2(u8),
//     }
//
//     #[client(ExampleCleint)]
//     pub enum ClientMsg {
//         Field1,
//         Field2,
//     }
// }
//
use std::pin::Pin;

use futures::{SinkExt, StreamExt};

auto_server_macros::client! {
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
