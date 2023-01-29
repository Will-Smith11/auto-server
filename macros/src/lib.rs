mod distrib;
use proc_macro::TokenStream;

/// the server macro has two main attributes that needed to be specified within
/// this macro to verifiy the completion these attributes are `#[client_msg]`
/// and `#[server_msg]` that respectfully need to be on the enums that
/// contain the messages you want to be sent a basic example of this is
/// ```rust
/// serverize! {
///
///     #[client(ClientExample)]
///     pub enum Reqests {
///         GetBeer { amount: u8 },
///         Echo(String),
///     }
///
///     #[server(ServerExample)]
///     pub enum Response {
///         Beers { amount: u8 },
///         Echo { msg: String },
///     }
/// }
/// ```
/// we auto apply the serialize and deserialize traits onto each enum field.
/// there will be an error thrown otherwise.
///
/// currently we don't have any formal verification system inplace, however
/// it might be added in the future.
#[proc_macro]
pub fn client_server(input: TokenStream) -> TokenStream
{
    distrib::build(input.into(), distrib::GenOps::Both).into()
}

#[proc_macro]
pub fn server(input: TokenStream) -> TokenStream
{
    distrib::build(input.into(), distrib::GenOps::Server).into()
}

#[proc_macro]
pub fn client(input: TokenStream) -> TokenStream
{
    distrib::build(input.into(), distrib::GenOps::Client).into()
}
