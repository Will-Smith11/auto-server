use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    Fields, Ident, ItemEnum, Meta, NestedMeta, Path,
};

pub fn get_name(parsed_attrs: &Vec<Meta>, side: String) -> Option<Ident>
{
    let mut name: Option<Ident> = None;
    for meta in parsed_attrs
    {
        if meta.path().get_ident().is_none()
        {
            continue
        }
        if meta.path().get_ident().unwrap().to_string() == side
        {
            match meta
            {
                Meta::List(list) =>
                {
                    let nested = list.nested.first().unwrap();
                    match nested
                    {
                        NestedMeta::Meta(a) =>
                        {
                            name = a.path().get_ident().cloned();
                            break
                        }
                        _ => panic!("shit"),
                    }
                }
                _ => panic!("not list"),
            }
        }
    }

    if name.is_none()
    {
        panic!("couldn't find name");
    }

    name
}

pub fn remove_attr(parsed_attrs: &Vec<Meta>, attr_path: String) -> Vec<&Meta>
{
    parsed_attrs
        .into_iter()
        .filter(|m| m.path().get_ident().is_some())
        .filter(|m| m.path().get_ident().unwrap().to_string() != attr_path)
        .collect()
}

pub fn make_set(i: &Ident) -> Ident
{
    Ident::new(&format!("set_{}", i), Span::call_site())
}

pub fn parse_server(server_enum: &ItemEnum, client_enum: &ItemEnum) -> TokenStream
{
    let parsed_attrs: Vec<Meta> = server_enum
        .attrs
        .iter()
        .filter_map(|a| a.parse_meta().ok())
        .collect();

    let name = get_name(&parsed_attrs, "server".to_string());
    if name.is_none()
    {
        panic!("failed to parse name");
    }
    let name = name.unwrap();
    //
    // todo asset that the generics are the same
    let server_generics = &server_enum.generics;

    let server_enum_name = &server_enum.ident;
    let client_enum_name = &client_enum.ident;

    let mut custom = server_enum.clone();
    custom.attrs.clear();

    quote! {

        type PendingFuture = Pin<Box<dyn std::future::Future<Output = Result<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, tokio_tungstenite::tungstenite::Error>> + Send>>;

        #[pin_project::pin_project]
        pub struct #name #server_generics {
            #[pin]
            listener: tokio::net::TcpListener,
            waiting_pings: std::collections::HashMap<u64, std::time::SystemTime>,
            #[pin]
            pending_conns: futures::stream::FuturesUnordered<PendingFuture>,
            connections: std::collections::HashMap<u64, (auto_server_common::PollState, tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>)>,
            outgoing_buffers: std::collections::HashMap<u64, std::collections::VecDeque<#server_enum_name>>,
            incoming_buffer: std::collections::VecDeque<(u64, #client_enum_name)>,
            ids: u64,
            timeout: std::time::Duration,
        }
        // should only be server gen ident after struct name
        impl #server_generics #name #server_generics {
            pub fn new(server_config: auto_server_common::server_config::ServerConfig) -> Self {
                Self {
                    listener: server_config.listener,
                    timeout: server_config.timeout,
                    waiting_pings: std::collections::HashMap::new(),
                    connections: std::collections::HashMap::new(),
                    outgoing_buffers: std::collections::HashMap::new(),
                    pending_conns: futures::stream::FuturesUnordered::default(),
                    incoming_buffer: std::collections::VecDeque::new(),
                    ids: 0,
                }
            }

            pub fn send_all(&mut self, msg: #server_enum_name) {
                for (_, (_, conn)) in self.connections.iter_mut() {
                    let mut buffer = self.outgoing_buffers.entry(conn.id()).or_default();
                    buffer.push_back(msg.clone());
                }
            }

            pub fn send(&mut self, id: u64, msg: #server_enum_name)
            {
                self.outgoing_buffers.entry(id).or_default().push_back(msg);
            }
        }

        // should only be server gen ident after struct name
        impl #server_generics futures::stream::Stream for #name  #server_generics {
            type Item = (u64, #client_enum_name);

            fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                // accept incomming connections
                let this = self.project();
                if let std::task::Poll::Ready(Ok((socket,_))) = this.listener.poll_accept(cx) {
                    let socket_fut = Box::pin(tokio_tungstenite::accept_async(socket));
                    this.pending_conns.push(socket_fut);
                }

                // insert pending resolved connectios and
                if let std::task::Poll::Ready(Some(Ok(new_socket))) = this.pending_conns.poll_next(cx) {
                    let id = *this.ids;
                    this.connections.insert(id, (auto_server_common::PollState::Ready, new_socket));
                    *this.ids += 1;
                }
                // poll all for incoming msg

                let mut new_req = Vec::new();
                let mut remove = Vec::new();
                for (id, (_, socket)) in this.connections.into_iter()

                {
                    if let std::task::Poll::Ready(Some(Ok(data))) = socket.poll_next_unpin(cx)
                    {
                        match data
                        {
                            tokio_tungstenite::tungstenite::Message::Text(text) =>
                            {
                                let msg: #client_enum_name = serde_json::from_str(&text).unwrap();
                                new_req.push((*id, msg));
                            }
                            tokio_tungstenite::tungstenite::Message::Ping(_) =>
                            {
                                // always auto send pongs
                                this.waiting_pings.insert(*id, std::time::SystemTime::now());
                                let _ = socket.send(tokio_tungstenite::tungstenite::Message::Pong(Vec::new()));
                            }
                            tokio_tungstenite::tungstenite::Message::Close(_) =>
                            {
                                remove.push(*id);
                            }
                            _ =>
                            {}
                        }
                    }
                }
                for id in remove
                {
                    this.connections.remove(&id);
                }

                for req in new_req {
                    this.incoming_buffer.push_back(req);
                }

                // progress sinks

                for (id, (poll_state, socket)) in this.connections.into_iter()
                {
                    match poll_state
                    {
                        auto_server_common::PollState::Ready =>
                        {
                            if let std::task::Poll::Ready(Ok(_)) = socket.poll_ready_unpin(cx)
                            {
                                *poll_state = auto_server_common::PollState::Send;
                            }
                        }
                        auto_server_common::PollState::Send =>
                        {
                            while let Some(entry) = this.outgoing_buffers.get_mut(&id).and_then(|b| b.pop_front())
                            {
                                let text = serde_json::to_string(&entry).unwrap();
                                let _ = socket.start_send_unpin(tokio_tungstenite::tungstenite::Message::Text(text));
                            }
                            *poll_state = auto_server_common::PollState::Flush;
                        }
                        auto_server_common::PollState::Flush =>
                        {
                            if let std::task::Poll::Ready(Ok(_)) = socket.poll_flush_unpin(cx)
                            {
                                *poll_state = auto_server_common::PollState::Ready;
                            }
                        }
                    }
                }

                // disconnect timeouts
                let mut disconnect = Vec::new();
                for (id, time) in this.waiting_pings.into_iter()
                {
                    if std::time::SystemTime::now().duration_since(time.clone()).unwrap().gt(this.timeout){
                        disconnect.push(*id);
                    }
                }

                for id in disconnect {
                    let _ = this.connections.remove(&id).unwrap().1.send(tokio_tungstenite::tungstenite::Message::Close(None));
                }

                if let Some(msg) = this.incoming_buffer.pop_front(){
                    return std::task::Poll::Ready(Some(msg))
                }
                else {
                    std::task::Poll::Pending
                }
            }
        }
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[serde(untagged)]
        #custom
    }
}
pub fn parse_client(client_enum: &ItemEnum, server_enum: &ItemEnum) -> TokenStream
{
    let parsed_attrs: Vec<Meta> = client_enum
        .attrs
        .iter()
        .filter_map(|a| a.parse_meta().ok())
        .collect();

    let name = get_name(&parsed_attrs, "client".to_string());
    if name.is_none()
    {
        panic!("failed to parse name");
    }

    let name = name.unwrap();
    let client_generics = &client_enum.generics;
    let client_enum_name = &client_enum.ident;
    let server_enum_name = &server_enum.ident;
    let mut custom = client_enum.clone();
    custom.attrs.clear();
    quote! {

        pub struct #name #client_generics {
            stream:          tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            ping_interval:   tokio::time::Interval,
            poll_state: auto_server_common::PollState,
            outgoing_buffer: std::collections::VecDeque<#client_enum_name>,
        }

        impl #client_generics #name #client_generics {
            pub async fn new(config: auto_server_common::client_config::ClientConfig) -> #name {
                let (stream,_)= tokio_tungstenite::connect_async(config.addr).await.unwrap();

                Self {
                    stream,
                    ping_interval: config.ping_interval,
                    poll_state: auto_server_common::PollState::Ready,
                    outgoing_buffer: std::collections::VecDeque::default(),
                }
            }

            pub fn send_msg(&mut self, msg: #client_enum_name)
            {
                self.outgoing_buffer.push_back(msg);
            }
        }

        impl #client_generics futures::Stream for #name #client_generics {
            type Item = #server_enum_name;

            fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                // deal with incomming msg
                if let std::task::Poll::Ready(Some(Ok(data))) = self.stream.poll_next_unpin(cx)
                {
                    match data {
                        tokio_tungstenite::tungstenite::Message::Text(msg) => {
                            return std::task::Poll::Ready(Some(serde_json::from_str(&msg).unwrap()))
                        }
                        _ => {
                        }
                    }
                }

                // progress sink
                match self.poll_state
                {
                    auto_server_common::PollState::Ready =>
                    {
                        if let std::task::Poll::Ready(Ok(_)) = self.stream.poll_ready_unpin(cx)
                        {
                            self.poll_state = auto_server_common::PollState::Send;
                        }
                    }
                    auto_server_common::PollState::Send =>
                    {
                        while let Some(msg) = self.outgoing_buffer.pop_front()
                        {
                            let _ = self.stream.start_send_unpin(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&msg).unwrap()));
                        }
                        self.poll_state = auto_server_common::PollState::Flush;
                    }
                    auto_server_common::PollState::Flush =>
                    {
                        if let std::task::Poll::Ready(Ok(_)) = self.stream.poll_flush_unpin(cx)
                        {
                            self.poll_state = auto_server_common::PollState::Ready;
                        }
                    }
                }

                std::task::Poll::Pending
            }
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[serde(untagged)]
        #custom
    }
}

/// reuturns (client, server)
pub fn identify<'a>(token1: &'a ItemEnum, token2: &'a ItemEnum) -> (&'a ItemEnum, &'a ItemEnum)
{
    let res = token1
        .attrs
        .iter()
        .map(|i| &i.path)
        .filter(|p| p.get_ident().map(|i| i.to_string()) == Some("client".to_string()))
        .collect::<Vec<&Path>>();

    if res.is_empty()
    {
        (token2, token1)
    }
    else
    {
        (token1, token2)
    }
}

pub enum GenOps
{
    Both,
    Client,
    Server,
}

pub(super) fn build(tokens: TokenStream, opts: GenOps) -> TokenStream
{
    let Data { server_data } = syn::parse2(tokens).unwrap();
    let token1 = &server_data[0];
    let token2 = &server_data[1];

    let (client, server) = identify(token1, token2);

    let stream = match opts
    {
        GenOps::Both =>
        {
            let gen_server = parse_server(server, client);
            let gen_client = parse_client(client, server);
            vec![gen_server, gen_client]
        }
        GenOps::Client =>
        {
            let mut custom = server.clone();
            custom.attrs.clear();

            let c_server = quote! {
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            #[serde(untagged)]
            #custom
                };

            vec![parse_client(client, server), c_server]
        }
        GenOps::Server =>
        {
            let mut custom = client.clone();
            custom.attrs.clear();

            let c_client = quote! {
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            #[serde(untagged)]
            #custom
                };

            vec![parse_server(server, client), c_client]
        }
    };

    quote! {
        #(#stream)*
    }
}

pub struct Data
{
    pub server_data: [ItemEnum; 2],
}
impl Parse for Data
{
    fn parse(input: ParseStream<'_>) -> syn::Result<Self>
    {
        // because of
        let item1: ItemEnum = input.parse()?;
        let item2: ItemEnum = input.parse()?;

        Ok(Self { server_data: [item1, item2] })
    }
}
