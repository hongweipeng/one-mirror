use std::sync::Arc;
use tokio::sync::Semaphore;
use crate::Client;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

pub fn headers_get<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    match headers.get(key) {
        Some(value) => {
            value.to_str().ok()
        },
        None => None,
    }
}

const MAX_REDIRECTS: usize = 5;

pub async fn reverse_proxy(
    client: Client,
    concurrency_limit: Arc<Semaphore>,
    mut req: Request,
    target: &str,
) -> Result<Response, StatusCode> {
    let uri = req.uri().clone();
    let path = uri.path();
    let method = req.method().clone();
    let path_query = uri.path_and_query().map(|v| v.as_str()).unwrap_or(path);

    req.headers_mut().remove("host"); // 删除 host 让框架自动识别
    let uri = if target.starts_with("http") {
        format!("{target}{path_query}")
    } else {
        format!("https://{target}{path_query}")
    };
    tracing::debug!("target uri: {:?} method: {:?} headers: {:?}", uri, method.as_str(), req.headers());
    *req.uri_mut() = Uri::try_from(uri).map_err(|_| StatusCode::BAD_REQUEST)?;

    // 在移动 req 之前克隆 headers，用于后续重定向
    let original_headers = req.headers().clone();

    // 获取并发许可，超出限制的请求将等待
    let _permit = concurrency_limit.acquire().await.map_err(|_| {
        tracing::error!("concurrency semaphore closed");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let mut response = client
        .request(req)
        .await
        .map_err(|e| {
            tracing::error!("forward error: {}", e);
            StatusCode::BAD_REQUEST
        })?
        .into_response();
    // 自动跟随 30x 重定向
    for _ in 0..MAX_REDIRECTS {
        let status = response.status();
        if !status.is_redirection() {
            break;
        }
        tracing::debug!("forward redirect status: {:?} headers: {:?}", status, response.headers());
        let location = response
            .headers()
            .get("location");
        if location.is_none() {
            // 没有 location，原样返回
            break
        }
        let location = location.and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                tracing::error!("redirect without location header");
                StatusCode::BAD_GATEWAY
            })?;

        let redirect_req = Request::builder()
            .method(method.clone())
            .uri(location);

        // 复制原始请求头（排除 host）
        let redirect_req = {
            let builder = redirect_req;
            let builder = original_headers.iter().fold(builder, |b, (name, value)| {
                if name != "host" {
                    b.header(name, value)
                } else {
                    b
                }
            });
            builder.body(Body::empty()).map_err(|e| {
                tracing::error!("build redirect request error: {}", e);
                StatusCode::BAD_REQUEST
            })?
        };

        response = client
            .request(redirect_req)
            .await
            .map_err(|e| {
                tracing::error!("redirect forward error: {}", e);
                StatusCode::BAD_REQUEST
            })?
            .into_response();
    }

    Ok(response)
}
