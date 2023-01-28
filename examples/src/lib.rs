use core::future::Future;
use std::{collections::VecDeque, pin::Pin};

use common::{server_config::ServerConfig, PollState};
use futures::{stream::FuturesUnordered, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};

macros::serverize! {
    #[server(ExampleServer)]
    pub enum SeverMsg {
        Field1(String),
        Field2(u8),
    }

    #[client(ExampleCleint)]
    pub enum ClientMsg {
        Field1,
        Field2,
    }
}
