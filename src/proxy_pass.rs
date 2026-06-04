use std::collections::HashMap;
use std::sync::Arc;
use axum::body::Body;
use tokio::sync::Semaphore;
use crate::AppState;
use crate::Client;
use crate::utils::{
    reverse_proxy,
    headers_get,
};
use axum::extract::{Request, State, Path};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use hyper::Uri;

fn replace_request_path(path: &str, request: &mut Request) {
    let new_path = path;
    let new_path_query = request.uri().clone()
        .path_and_query()
        .and_then(|pq| pq.query())
        .map(|q| format!("{new_path}?{q}"))
        .unwrap_or_else(|| new_path.to_string());
    let new_uri = Uri::try_from(new_path_query).map_err(|_| StatusCode::BAD_REQUEST);
    match new_uri {
        Ok(new_uri) => {
            *request.uri_mut() = new_uri;
        }
        _ => {}
    }
}
pub async fn centos(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    if path.starts_with("/RPM-GPG-KEY") {
        replace_request_path(&path, &mut req);
    }
    let mut target = "vault.centos.org";
    {
        // 版本修复。 /centos/5/ox/xxxx => /centos/5.11/ox/xxxx
        let version_fix = HashMap::<i32, i32>::from([
            (5, 11),
            (6, 10),
        ]);
        let mut hit = false;
        let mut new_path = format!("/centos-vault{}", path);
        for (k, v) in version_fix.iter() {
            let version = format!("/{}/", k);
            if new_path.contains(version.as_str()) {
                new_path = new_path.replace(version.as_str(), format!("/{}.{}/", k, v).as_str());
                hit = true;
            }
        }
        if hit {
            target = "archive.kernel.org"; // 使用更久的归档地址
            replace_request_path(&new_path, &mut req);
        }
    }
    reverse_proxy(state.client, state.concurrency_limit, req, target).await
}

pub async fn debian(State(state): State<AppState>, req: Request) -> Result<Response, StatusCode> {
    let mut target = "archive.debian.org";
    if req.uri().path().starts_with("/debian-security") { // 归档地址没有安全更新
        target = "deb.debian.org";
    }
    reverse_proxy(state.client, state.concurrency_limit, req, target).await
}

pub async fn ubuntu(State(state): State<AppState>, req: Request) -> Result<Response, StatusCode> {
    reverse_proxy(state.client, state.concurrency_limit, req, "archive.ubuntu.com").await
}

pub async fn alpine(State(state): State<AppState>, req: Request) -> Result<Response, StatusCode> {
    reverse_proxy(state.client, state.concurrency_limit, req, "dl-cdn.alpinelinux.org").await
}

pub async fn rust_static(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "static.rust-lang.org").await
}

pub async fn rust_crates_index(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "index.crates.io").await
}

pub async fn pypi(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    // 去掉 /pypi 前缀，使 /pypi/simple -> pypi.org/simple
    let uri = req.uri().clone();
    let origin_path = uri.path();
    let new_path = path;
    let new_path_query = uri
        .path_and_query()
        .and_then(|pq| pq.query())
        .map(|q| format!("{new_path}?{q}"))
        .unwrap_or_else(|| new_path.to_string());
    tracing::info!("origin path: {:?} new path query: {:?}", origin_path, new_path_query);
    *req.uri_mut() = Uri::try_from(new_path_query).map_err(|_| StatusCode::BAD_REQUEST)?;
    reverse_proxy(state.client, state.concurrency_limit, req, "pypi.org").await
}

pub async fn composer(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "repo.packagist.org").await
}

pub async fn npm(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "registry.npmjs.org").await
}

pub async fn maven(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    let new_path = if path == "/" {
        "/".to_string()
    } else {
        format!("/maven2{}", path)
    };
    replace_request_path(&new_path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "repo.maven.apache.org").await
}

pub async fn docker(client: Client, concurrency_limit: Arc<Semaphore>, req:  Request) -> Result<Response, StatusCode> {
    let host = headers_get(req.headers(), "host").unwrap_or("localhost").to_string();
    let mut response = reverse_proxy(client, concurrency_limit, req, "registry-1.docker.io").await?;

    // 重写 WWW-Authenticate 头，将 auth.docker.io 替换为代理地址
    if response.status() == StatusCode::UNAUTHORIZED {
        if let Some(www_auth) = response.headers().get("www-authenticate").cloned() {
            if let Ok(auth_str) = www_auth.to_str() {
                let new_auth = auth_str.replace(
                    "https://auth.docker.io/token",
                    &format!("https://{host}/docker-token"),
                );
                if let Ok(new_value) = HeaderValue::from_str(&new_auth) {
                    response.headers_mut().insert("www-authenticate", new_value);
                }
            }
        }
    }

    Ok(response)
}

pub async fn docker_auth(State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path("/token", &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "auth.docker.io").await
}

pub async fn goproxy(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "proxy.golang.org").await
}

/**
当 GO_PROXY=xxx.com 时，go get 对 /sumdb/sum.golang.org/... 的请求被镜像站捕获，经以下流程处理：
GET https://xxx.com/sumdb/sum.golang.org/tile/8/0/0/0000000000000000000000000000000000000000000000000000000000000000
→ 302 Location: https://sum.golang.org/tile/8/0/0/0000000000000000000000000000000000000000000000000000000000000000
*/
pub async fn sum_goproxy(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    if path == "/supported" {
        // 返回空响应体标识支持校验
        return Ok(Response::new(Body::empty()));
    }
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "sum.golang.org").await
}

pub async fn jump_to(Path(target): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    if target.starts_with("://") || target.starts_with("s://") {
        let full_url = format!("http{}", target);
        let uri = full_url.parse::<Uri>().map_err(|e| {
            tracing::error!("bad url 1: {}, full_url: {}", e, full_url);
            StatusCode::BAD_REQUEST
        })?;
        let target_host = uri.host().ok_or_else(||  {
            tracing::error!("bad url 2: {}", full_url);
            StatusCode::BAD_REQUEST
        })?;
        let target_scheme = uri.scheme_str().ok_or_else(|| {
            tracing::error!("bad url 3: {}", full_url);
            StatusCode::BAD_REQUEST
        })?;
        let target_uri = format!("{}://{}", target_scheme, target_host);
        replace_request_path(uri.path(), &mut req);
        reverse_proxy(state.client, state.concurrency_limit, req, &target_uri).await
    } else {
        Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(axum::body::Body::from("URL must start with http:// or https://"))
            .unwrap())
    }
}


pub async fn dispatch(client: Client, concurrency_limit: Arc<Semaphore>, request: Request) -> Option<Result<Response, Request>> {
    let headers = request.headers();
    let user_agent = headers_get(headers, "user-agent");
    if let Some(user_agent) = user_agent {
        if user_agent.contains("docker") && request.uri().path() != "/docker-token" {
            // docker 请求由 dispatch 处理，失败时不需要 fallback
            return match docker(client, concurrency_limit, request).await {
                Ok(response) => Some(Ok(response)),
                Err(_) => None,
            };
        }
    }
    // 非 docker 请求，归还 request 以便 fallback
    Some(Err(request))
}
