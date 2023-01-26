use core::future::Future;
use std::{collections::VecDeque, error::Error, pin::Pin};

use common::{server_config::ServerConfig, PollState};
use futures::{stream::FuturesUnordered, SinkExt, Stream, StreamExt};

macros::serverize! {

    #[server(ExampleServer)]
    pub enum SeverMsg {
        Field1(String),
        Field2 { amount: u8 },
    }

    #[client(ExampleCleint)]
    pub enum ClientMsg {
        Field1,
        Field2,
    }
}
