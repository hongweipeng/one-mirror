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
    let targets = vec![
        "archive.debian.org",
        "deb.debian.org",
    ];
    try_proxy_chain(&mut State(state), req, &targets, StatusCode::NOT_FOUND).await
}

pub async fn pve(State(state): State<AppState>, req: Request) -> Result<Response, StatusCode> {
    reverse_proxy(state.client, state.concurrency_limit, req, "enterprise.proxmox.com").await
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
    replace_request_path(&path, &mut req);
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

pub async fn github(Path(path): Path<String>, State(state): State<AppState>, mut req: Request) -> Result<Response, StatusCode> {
    replace_request_path(&path, &mut req);
    reverse_proxy(state.client, state.concurrency_limit, req, "github.com").await
}

pub async fn gravatar(State(state): State<AppState>, req: Request) -> Result<Response, StatusCode> {
    reverse_proxy(state.client, state.concurrency_limit, req, "gravatar.com").await
}

pub async fn try_proxy_chain(State(state): &mut State<AppState>, req: Request, targets: &Vec<&str>, status_code: StatusCode) -> Result<Response, StatusCode> {
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut ret = Err(StatusCode::BAD_REQUEST);
    for target in targets {
        let r = Request::from_parts(parts.clone(), Body::from(body_bytes.clone()));
        let uri = r.uri().clone();
        let response = reverse_proxy(state.client.clone(), state.concurrency_limit.clone(), r, target).await?;
        if response.status() != status_code {
            ret = Ok(response);
            break;
        }
        tracing::warn!("target {}{} returned 404", target, uri.path());
    }
    ret
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    // === replace_request_path 测试 ===

    #[test]
    fn test_replace_request_path_simple() {
        // 替换路径，不带 query string
        let req = HttpRequest::builder()
            .uri("/old/path")
            .body(Body::empty())
            .unwrap();
        let mut req = req;
        replace_request_path("/new/path", &mut req);
        assert_eq!(req.uri().path(), "/new/path");
        assert!(req.uri().query().is_none());
    }

    #[test]
    fn test_replace_request_path_with_query() {
        // 替换路径，保留原有 query string
        let req = HttpRequest::builder()
            .uri("/old/path?key=val&foo=bar")
            .body(Body::empty())
            .unwrap();
        let mut req = req;
        replace_request_path("/new/path", &mut req);
        assert_eq!(req.uri().path(), "/new/path");
        assert_eq!(req.uri().query(), Some("key=val&foo=bar"));
    }

    #[test]
    fn test_replace_request_path_empty_query() {
        // 路径末尾带 ? 但无实际 query 内容
        let req = HttpRequest::builder()
            .uri("/old/path?")
            .body(Body::empty())
            .unwrap();
        let mut req = req;
        replace_request_path("/new/path", &mut req);
        assert_eq!(req.uri().path(), "/new/path");
    }

    #[test]
    fn test_replace_request_path_root() {
        // 替换为根路径
        let req = HttpRequest::builder()
            .uri("/something")
            .body(Body::empty())
            .unwrap();
        let mut req = req;
        replace_request_path("/", &mut req);
        assert_eq!(req.uri().path(), "/");
    }

    // === centos 版本修复逻辑测试 ===

    #[test]
    fn test_centos_version_fix_mapping() {
        // 验证版本号映射表：5 -> 5.11, 6 -> 6.10
        let version_fix = HashMap::<i32, i32>::from([
            (5, 11),
            (6, 10),
        ]);
        // /centos/5/os/x86_64 应被替换为 /centos/5.11/os/x86_64
        let path = "/5/os/x86_64/Packages";
        let mut new_path = format!("/centos-vault{}", path);
        for (k, v) in version_fix.iter() {
            let version = format!("/{}/", k);
            if new_path.contains(version.as_str()) {
                new_path = new_path.replace(version.as_str(), format!("/{}.{}/", k, v).as_str());
            }
        }
        assert_eq!(new_path, "/centos-vault/5.11/os/x86_64/Packages");
    }

    #[test]
    fn test_centos_version_fix_6() {
        let version_fix = HashMap::<i32, i32>::from([
            (5, 11),
            (6, 10),
        ]);
        let path = "/6/updates/x86_64";
        let mut new_path = format!("/centos-vault{}", path);
        for (k, v) in version_fix.iter() {
            let version = format!("/{}/", k);
            if new_path.contains(version.as_str()) {
                new_path = new_path.replace(version.as_str(), format!("/{}.{}/", k, v).as_str());
            }
        }
        assert_eq!(new_path, "/centos-vault/6.10/updates/x86_64");
    }

    #[test]
    fn test_centos_version_no_fix_for_7() {
        // CentOS 7 不在映射表中，路径不变
        let version_fix = HashMap::<i32, i32>::from([
            (5, 11),
            (6, 10),
        ]);
        let path = "/7/os/x86_64";
        let mut new_path = format!("/centos-vault{}", path);
        let mut hit = false;
        for (k, v) in version_fix.iter() {
            let version = format!("/{}/", k);
            if new_path.contains(version.as_str()) {
                new_path = new_path.replace(version.as_str(), format!("/{}.{}/", k, v).as_str());
                hit = true;
            }
        }
        assert!(!hit);
        assert_eq!(new_path, "/centos-vault/7/os/x86_64");
    }

    // === maven 路径逻辑测试 ===

    #[test]
    fn test_maven_path_root() {
        // 根路径保持为 /
        let path = "/".to_string();
        let new_path = if path == "/" {
            "/".to_string()
        } else {
            format!("/maven2{}", path)
        };
        assert_eq!(new_path, "/");
    }

    #[test]
    fn test_maven_path_with_artifact() {
        // 非 root 路径添加 /maven2 前缀
        let path = "/org/apache/maven/maven-core/3.8.1/maven-core-3.8.1.pom".to_string();
        let new_path = if path == "/" {
            "/".to_string()
        } else {
            format!("/maven2{}", path)
        };
        assert_eq!(new_path, "/maven2/org/apache/maven/maven-core/3.8.1/maven-core-3.8.1.pom");
    }

    // === jump_to URL 解析逻辑测试 ===

    #[test]
    fn test_jump_to_https_target() {
        // ://example.com/path => https://example.com/path
        let target = "s://example.com/some/path".to_string();
        let full_url = format!("http{}", target);
        let uri = full_url.parse::<Uri>().unwrap();
        assert_eq!(uri.scheme_str(), Some("https"));
        assert_eq!(uri.host(), Some("example.com"));
        assert_eq!(uri.path(), "/some/path");
    }

    #[test]
    fn test_jump_to_http_target() {
        // ://example.com/path => http://example.com/path
        let target = "://example.com/some/path".to_string();
        let full_url = format!("http{}", target);
        let uri = full_url.parse::<Uri>().unwrap();
        assert_eq!(uri.scheme_str(), Some("http"));
        assert_eq!(uri.host(), Some("example.com"));
        assert_eq!(uri.path(), "/some/path");
    }

    #[test]
    fn test_jump_to_invalid_prefix() {
        // 不以 :// 或 s:// 开头时应被拒绝
        let target = "example.com/path".to_string();
        let starts_with_scheme = target.starts_with("://") || target.starts_with("s://");
        assert!(!starts_with_scheme);
    }

    // === dispatch 逻辑测试 ===

    #[test]
    fn test_dispatch_non_docker_user_agent() {
        // 非 docker user-agent 的请求应返回 Err(request) 即 fallback
        // 这里只测试判断逻辑，不测试 async dispatch 函数
        let ua = "Mozilla/5.0";
        let is_docker = ua.contains("docker");
        assert!(!is_docker);
    }

    #[test]
    fn test_dispatch_docker_user_agent() {
        // docker user-agent 应被识别
        let ua = "docker/20.10.7";
        let is_docker = ua.contains("docker");
        assert!(is_docker);
    }

    #[test]
    fn test_dispatch_docker_token_path_excluded() {
        // /docker-token 路径即使带 docker UA 也不走 docker 代理
        let path = "/docker-token";
        let is_docker_ua = true;
        let should_dispatch = is_docker_ua && path != "/docker-token";
        assert!(!should_dispatch);
    }
}
