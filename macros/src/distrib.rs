use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
    Attribute, Ident, ItemEnum, Meta, MetaList, NestedMeta, Path,
};

pub fn get_name(parsed_attrs: &Vec<Meta>, side: String) -> Ident
{
    let mut name: Option<Ident> = None;
    for meta in parsed_attrs
    {
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

    name.unwrap()
}

pub fn remove_attr(parsed_attrs: &Vec<Meta>, attr_path: String) -> Vec<&Meta>
{
    parsed_attrs
        .into_iter()
        .filter(|m| m.path().get_ident().unwrap().to_string() != attr_path)
        .collect()
}

pub fn parse_server(server_enum: &ItemEnum, client_enum: &ItemEnum) -> TokenStream
{
    let parsed_attrs: Vec<Meta> = server_enum
        .attrs
        .iter()
        .map(|a| a.parse_meta().unwrap())
        .collect();

    let name = get_name(&parsed_attrs, "server".to_string());
    //
    // todo asset that the generics are the same
    let server_generics = &server_enum.generics;

    let server_enum_name = &server_enum.ident;
    let client_enum_name = &client_enum.ident;

    let removed_personal_attr = remove_attr(&parsed_attrs, "server".to_string());
    let mut custom = server_enum.clone();
    custom.attrs.clear();

    quote! {
        type PendingFuture = Pin<Box<dyn Future<Output = Result<WebSocketStream<TcpStream>, Error>>>>;

        pub struct #name #server_generics {
            listener: TcpListener,
            waiting_pings: HashMap<u64, SystemTime>,
            pending_conns: FuturesUnordered<PendingFuture>,
            connections: HashMap<u64, (PollState, WebSocketStream<TcpStream>)>,
            outgoing_buffers: HashMap<u64, VecDeque<#server_enum_name>>,
            incoming_buffer: VecDeque<(u64, #client_enum_name)>,
            ids: u64,
            timeout: Duration
        }
        // should only be server gen ident after struct name
        impl #server_generics #name #server_generics {
            pub fn new(server_config: ServerConfig) -> Self {
                Self {
                    listener: server_config.listener,
                    timeout: server_config.timeout,
                    waiting_pings: HashMap::new(),
                    connections: HashMap::new(),
                    outgoing_buffers: HashMap::new(),
                    pending_conns: FuturesUnordered::default(),
                    incoming_buffer: VecDeque::new(),
                    ids: 0,
                }
            }
        }

        // should only be server gen ident after struct name
        impl #server_generics Stream for #name  #server_generics {
            type Item = (u64, #client_enum_name);

            fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                // accept incomming connections
                if let Poll::Ready(Ok((socket,_))) = self.listener.poll_accept(cx) {
                    let socket_fut = Box::pin(accept_async(socket));
                    self.pending_conns.push(socket_fut);
                }

                // insert pending resolved connectios and
                if let Poll::Ready(Some(Ok(new_socket))) = self.pending_conns.poll_next_unpin(cx) {
                    let id = self.id;
                    self.id += 1;

                    self.connections.insert(id, (PollState::Ready, new_socket));
                }
                // poll all for incoming msg

                let mut new_req = Vec::new();
                let mut remove = Vec::new();
                for (id, (_, socket)) in self.connections.iter_mut()

                {
                    if let Poll::Ready(Some(Ok(data))) = socket.poll_next_unpin(cx)
                    {
                        match data
                        {
                            Message::Text(text) =>
                            {
                                let msg: #client_enum_name = serde_json::from_str(&text).unwrap();
                                new_req.push((*id, msg));
                            }
                            Message::Ping(_) =>
                            {
                                // always auto send pongs
                                self.waiting_pings.insert(id, SystemTime::now);
                                let _ = socket.send(Message::Pong(Vec::new()));
                            }
                            Message::Close(_) =>
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
                    self.connections.remove(&id);
                }

                for req in new_req {
                    self.incoming_buffer.push(req);
                }

                // progress sinks

                for (id, (poll_state, socket)) in &mut self.connections
                {
                    match poll_state
                    {
                        PollState::Ready =>
                        {
                            if let Poll::Ready(Ok(_)) = socket.poll_ready_unpin(cx)
                            {
                                *poll_state = PollState::Send;
                            }
                        }
                        PollState::Send =>
                        {
                            while let Some(entry) = self.outgoing_buffers.get_mut(&id).and_then(|b| b.pop_front())
                            {
                                let text = serde_json::to_string(&entry).unwrap();
                                let _ = socket.start_send_unpin(Message::Text(text));
                            }
                            *poll_state = PollState::Flush;
                        }
                        PollState::Flush =>
                        {
                            if let Poll::Ready(Ok(_)) = socket.poll_flush_unpin(cx)
                            {
                                *poll_state = PollState::Ready;
                            }
                        }
                    }
                }

                // disconnect timeouts
                let mut disconnect = Vec::new();
                for (id, time) in &self.waiting_pings
                {
                    if SystemTime::now().duration_since(time) > self.timeout{
                        disconnect.push(id);
                    }
                }

                for id in disconnect {
                    let _ = self.connections.remove(id).send(Message::Close(None));
                }

                if let Some(msg) = self.incoming_buffer.pop_front(){
                    return Poll::Ready(msg)
                }
                else {
                    Poll::Pending
                }
            }
        }

        #(#removed_personal_attr)*
        #custom
    }
}
pub fn parse_client(client_enum: &ItemEnum, server_enum: &ItemEnum) -> TokenStream
{
    let parsed_attrs: Vec<Meta> = client_enum
        .attrs
        .iter()
        .map(|a| a.parse_meta().unwrap())
        .collect();

    let name = get_name(&parsed_attrs, "client".to_string());
    let client_generics = &client_enum.generics;
    let removed_personal_attr = remove_attr(&parsed_attrs, "client".to_string());
    let mut custom = client_enum.clone();
    custom.attrs.clear();
    quote! {

        pub struct #name #client_generics { }

        #(#removed_personal_attr)*
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

pub(super) fn build(tokens: TokenStream) -> TokenStream
{
    // let ServerEnum { server_name, server_data } =
    // syn::parse2(tokens.clone()).unwrap(); let ServerEnum { server_data }=
    // syn::parse2(tokens).unwrap();

    let Data { server_data } = syn::parse2(tokens).unwrap();
    let token1 = &server_data[0];
    let token2 = &server_data[1];

    let (client, server) = identify(token1, token2);

    let parsed_server = parse_server(server, client);
    let parsed_client = parse_client(client, server);

    quote! {
        use std::task::{Poll, Context};
        use tokio::time::Duration;
        use tokio::net::{TcpListener, TcpStream};
        use tokio_tungstenite::{
            accept_async,
            tungstenite::{handshake::server::NoCallback, protocol::WebSocketConfig, Message},
            WebSocketStream
        };
        use std::collections::HashMap;
        use std::time::SystemTime;

        #parsed_client
        #parsed_server
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
