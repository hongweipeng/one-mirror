mod proxy_pass;
mod utils;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse};
use axum::{Router, middleware, response::Html, routing::any, routing::get};
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use structopt::StructOpt;
use tokio::sync::Semaphore;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

type Client = hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, Body>;

// 共享状态：HTTP 客户端 + 并发信号量
#[derive(Clone)]
struct AppState {
    client: Client,
    // 限制同时进行的上游代理请求数量
    concurrency_limit: Arc<Semaphore>,
}

#[derive(StructOpt)]
#[structopt(name = "server", about = "A http server written in Rust", version=env!("CARGO_PKG_VERSION"))]
struct Opts {
    #[structopt(long, default_value = "[::]")]
    pub server_host: String,
    #[structopt(long, default_value = "13400")]
    pub server_port: u16,
    // 上游代理请求的最大并发数
    #[structopt(long, default_value = "1000")]
    pub max_concurrency: usize,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let https = HttpsConnector::new();
    let client: Client =
        hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
            .build(https);

    let opts = Opts::from_args();
    let state = AppState {
        client,
        concurrency_limit: Arc::new(Semaphore::new(opts.max_concurrency)),
    };

    // build our application with a route
    let app = Router::new()
        .route("/", get(handler))
        .route("/centos{*path}", any(proxy_pass::centos))
        .route("/debian/pve{*path}", any(proxy_pass::pve))
        .route("/debian{*path}", any(proxy_pass::debian))
        .route("/ubuntu{*path}", any(proxy_pass::ubuntu))
        .route("/alpine{*path}", any(proxy_pass::alpine))
        .route("/pypi{*path}", any(proxy_pass::pypi))
        .route("/rust-static{*path}", any(proxy_pass::rust_static))
        .route("/crates.io-index{*path}", any(proxy_pass::rust_crates_index))
        .route("/composer{*path}", any(proxy_pass::composer))
        .route("/npm{*path}", any(proxy_pass::npm))
        .route("/maven{*path}", any(proxy_pass::maven))
        .route("/docker-token", any(proxy_pass::docker_auth))
        .route("/goproxy/sumdb/sum.golang.org{*path}", any(proxy_pass::sum_goproxy))
        .route("/goproxy{*path}", any(proxy_pass::goproxy))
        .route("/github.com{*path}", any(proxy_pass::github))
        .route("/avatar{*path}", any(proxy_pass::gravatar))
        .route("/http{*target}", any(proxy_pass::jump_to))
        .fallback(handler_404)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            print_request_response,
        ))
        .with_state(state);

    // run it
    let bind_str = format!("{}:{}", opts.server_host, opts.server_port);
    let listener = tokio::net::TcpListener::bind(bind_str)
        .await
        .unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handler() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

// 全局 404 处理
async fn handler_404(method: Method, uri: Uri) -> (StatusCode, String) {
    (
        StatusCode::NOT_FOUND,
        format!("`{} {}` Not Found", method, uri),
    )
}

async fn print_request_response(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request_uri = request.uri().clone();
    let request_headers = request.headers().clone();
    let request_method = request.method().clone();

    let response = match proxy_pass::dispatch(state.client.clone(), state.concurrency_limit.clone(), request).await {
        Some(Ok(response)) => response,
        Some(Err(request)) => next.run(request).await,
        None => {
            return Err((StatusCode::BAD_REQUEST, "proxy dispatch failed".to_string()));
        }
    };

    let response_status = response.status();
    let response_headers = response.headers().clone();
    tracing::info!(
        "request_uri: {:?} method: {:?} request_headers: {:?} response_status: {:?} response_headers: {:?}",
        request_uri,
        request_method,
        request_headers,
        response_status,
        response_headers,
    );

    Ok(response)
}
