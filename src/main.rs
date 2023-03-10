extern crate serde_json;
use axum::response::Json;
use axum::{extract::State, routing::get, Router};
use hyper::body::Buf;
use hyper::client::HttpConnector;
use hyper::{Body, Client};
use serde_derive::Deserialize;
use serde_derive::Serialize;

use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use hyper::server::conn::AddrIncoming;
use std::env;
use tokio::net::{TcpListener, TcpSocket};

pub const POOL_SIZE: u32 = 10000;
pub const PORT: u16 = 8001;

#[derive(Clone)]
pub struct AppState {
    external_url: String,
    client: hyper::Client<HttpConnector, Body>,
}

#[derive(Serialize, Deserialize)]
pub struct IOCall {
    pub status: Option<i64>,
    pub msg: Option<String>,
}

pub async fn io_call(State(state): State<AppState>) -> Json<IOCall> {
    let external_url = state.external_url.parse().unwrap();
    let resp = state.client.get(external_url).await.unwrap();
    let body = hyper::body::aggregate(resp).await.unwrap();

    Json(serde_json::from_reader(body.reader()).unwrap())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

pub fn builder() -> hyper::server::Builder<AddrIncoming> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT));
    let listener = reuse_listener(addr).expect("couldn't bind to addr");
    let incoming = AddrIncoming::from_listener(listener).unwrap();

    println!(
        "Started axum server at {port} with pool size {pool_size}",
        port = PORT,
        pool_size = POOL_SIZE
    );

    axum::Server::builder(incoming)
        .http1_only(true)
        .tcp_nodelay(true)
}

fn reuse_listener(addr: SocketAddr) -> io::Result<TcpListener> {
    let socket = match addr {
        SocketAddr::V4(_) => TcpSocket::new_v4()?,
        SocketAddr::V6(_) => TcpSocket::new_v6()?,
    };

    #[cfg(unix)]
    {
        if let Err(e) = socket.set_reuseport(true) {
            eprintln!("error setting SO_REUSEPORT: {}", e);
        }
    }

    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    socket.listen(1024)
}

#[tokio::main]
async fn main() {
    // EXTERNAL_URL = http://172.30.120.12/
    let app_state = AppState {
        external_url: env::var("EXTERNAL_URL").expect("Set EXTERNAL_URL env variable"),
        client: Client::new(),
    };
    let app = Router::new()
        .route("/io", get(io_call))
        .route("/static", get(root))
        .with_state(app_state.clone());

    builder()
        .http1_pipeline_flush(true)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
