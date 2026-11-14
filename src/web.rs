use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

const INDEX: &str = "index.html";
const ROBOTS: &str = "User-agent: *\nAllow: /$\nDisallow: /\n";

/// A read-only set of static files, keyed by path relative to the dist root.
pub trait Assets: Send + Sync + 'static {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>>;
}

#[derive(RustEmbed)]
#[folder = "web/app/dist"]
#[allow_missing = true]
struct Dist;

/// The front end built into the binary (empty when `web/app/dist` is absent).
pub struct Embedded;

impl Assets for Embedded {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        Dist::get(path).map(|f| f.data)
    }
}

/// In-memory assets, for tests (including the integration tests).
pub struct MapAssets(pub HashMap<&'static str, &'static [u8]>);

impl Assets for MapAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        self.0.get(path).map(|b| Cow::Borrowed(*b))
    }
}

type Shared = Arc<dyn Assets>;

/// The static part of the app: index, files, `/assets/*` and robots.txt.
pub fn routes(assets: Arc<dyn Assets>) -> Router {
    Router::new()
        .route("/", get(root).fallback(method_not_allowed))
        .route("/robots.txt", get(robots).fallback(method_not_allowed))
        .route("/assets/{*path}", get(asset).fallback(method_not_allowed))
        .route("/{seg}", get(page).fallback(method_not_allowed))
        .route("/{seg}/", get(page).fallback(method_not_allowed))
        .with_state(assets)
}

async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "GET, HEAD")],
    )
        .into_response()
}

async fn robots() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        ROBOTS,
    )
        .into_response()
}

fn content_type(path: &str) -> HeaderValue {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    HeaderValue::from_str(mime.as_ref()).expect("mime types are valid header values")
}

fn file_response(path: &str, data: Cow<'static, [u8]>, cache_control: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type(path)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            ),
        ],
        data.into_owned(),
    )
        .into_response()
}

async fn asset(State(assets): State<Shared>, Path(path): Path<String>) -> Response {
    match assets.get(&format!("assets/{path}")) {
        Some(data) => file_response(&path, data, "public, max-age=31536000, immutable"),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn root(State(assets): State<Shared>) -> Response {
    index(&*assets)
}

async fn page(State(assets): State<Shared>, Path(seg): Path<String>) -> Response {
    if seg != INDEX
        && let Some(data) = assets.get(&seg)
    {
        return file_response(&seg, data, "public, max-age=3600");
    }
    index(&*assets)
}

fn index(assets: &dyn Assets) -> Response {
    let Some(data) = assets.get(INDEX) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "front end not built: run just web",
        )
            .into_response();
    };
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONTENT_SECURITY_POLICY, CSP),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        data.into_owned(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn assets(with_index: bool) -> Arc<dyn Assets> {
        let mut m: HashMap<&'static str, &'static [u8]> = HashMap::new();
        if with_index {
            m.insert("index.html", b"<html>quorum</html>");
        }
        m.insert("favicon.svg", b"<svg/>");
        m.insert("assets/app-1.js", b"console.log(1)");
        Arc::new(MapAssets(m))
    }

    async fn call(
        a: Arc<dyn Assets>,
        method: Method,
        uri: &str,
    ) -> (StatusCode, axum::http::HeaderMap, String) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let res = routes(a).oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            parts.headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    fn h<'a>(headers: &'a axum::http::HeaderMap, k: &str) -> &'a str {
        headers.get(k).unwrap().to_str().unwrap()
    }

    #[tokio::test]
    async fn root_serves_index_with_csp() {
        let (s, hd, body) = call(assets(true), Method::GET, "/").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "<html>quorum</html>");
        assert_eq!(h(&hd, "cache-control"), "no-cache");
        assert_eq!(h(&hd, "content-security-policy"), CSP);
        assert!(CSP.contains("script-src 'self';"));
        assert_eq!(h(&hd, "x-content-type-options"), "nosniff");
        assert_eq!(h(&hd, "referrer-policy"), "no-referrer");
        assert!(h(&hd, "content-type").starts_with("text/html"));
    }

    #[tokio::test]
    async fn room_path_serves_index() {
        let (s, _, body) = call(assets(true), Method::GET, "/kernel-panic-42").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "<html>quorum</html>");
    }

    #[tokio::test]
    async fn trailing_slash_serves_index() {
        let (s, _, body) = call(assets(true), Method::GET, "/kernel-panic-42/").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "<html>quorum</html>");
    }

    #[tokio::test]
    async fn deep_path_is_404() {
        let (s, _, _) = call(assets(true), Method::GET, "/a/b").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn favicon_served_as_file() {
        let (s, hd, body) = call(assets(true), Method::GET, "/favicon.svg").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "<svg/>");
        assert_eq!(h(&hd, "cache-control"), "public, max-age=3600");
        assert_eq!(h(&hd, "content-type"), "image/svg+xml");
    }

    #[tokio::test]
    async fn asset_is_immutable() {
        let (s, hd, body) = call(assets(true), Method::GET, "/assets/app-1.js").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "console.log(1)");
        assert_eq!(
            h(&hd, "cache-control"),
            "public, max-age=31536000, immutable"
        );
        assert!(h(&hd, "content-type").contains("javascript"));
    }

    #[tokio::test]
    async fn missing_asset_404() {
        let (s, _, _) = call(assets(true), Method::GET, "/assets/nope.js").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn robots() {
        let (s, _, body) = call(assets(true), Method::GET, "/robots.txt").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, "User-agent: *\nAllow: /$\nDisallow: /\n");
    }

    #[tokio::test]
    async fn post_is_405() {
        let (s, hd, _) = call(assets(true), Method::POST, "/").await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(h(&hd, "allow"), "GET, HEAD");
        let (s, hd, _) = call(assets(true), Method::POST, "/room-1").await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(h(&hd, "allow"), "GET, HEAD");
    }

    #[tokio::test]
    async fn missing_index_503() {
        let (s, _, body) = call(assets(false), Method::GET, "/").await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body, "front end not built: run just web");
    }
}
