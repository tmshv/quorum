use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::limits::valid_room;
use crate::og::{self, Cards};

pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

const INDEX: &str = "index.html";
/// Search engines get the front page only; link-preview bots may read rooms
/// (Twitterbot, for one, honours robots.txt and shows no card otherwise).
const ROBOTS: &str = "User-agent: *\nAllow: /$\nDisallow: /\n\n\
User-agent: Twitterbot\nUser-agent: facebookexternalhit\nUser-agent: LinkedInBot\n\
User-agent: Slackbot-LinkExpanding\nUser-agent: Discordbot\nUser-agent: TelegramBot\n\
Allow: /\n";

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

#[derive(Clone)]
struct Site {
    assets: Arc<dyn Assets>,
    cards: Arc<Cards>,
}

/// The static part of the app: index, files, `/assets/*`, robots.txt and the
/// link-preview cards (`/og.png`, `/og/<room>.png`).
pub fn routes(assets: Arc<dyn Assets>) -> Router {
    let site = Site {
        assets,
        cards: Arc::new(Cards::default()),
    };
    Router::new()
        .route("/og.png", get(home_card).fallback(method_not_allowed))
        .route("/og/{file}", get(room_card).fallback(method_not_allowed))
        .route("/", get(root).fallback(method_not_allowed))
        .route("/robots.txt", get(robots).fallback(method_not_allowed))
        .route("/assets/{*path}", get(asset).fallback(method_not_allowed))
        .route("/{seg}", get(page).fallback(method_not_allowed))
        .route("/{seg}/", get(page).fallback(method_not_allowed))
        .with_state(site)
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

async fn asset(State(site): State<Site>, Path(path): Path<String>) -> Response {
    match site.assets.get(&format!("assets/{path}")) {
        Some(data) => file_response(&path, data, "public, max-age=31536000, immutable"),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn root(State(site): State<Site>, headers: HeaderMap) -> Response {
    index(&*site.assets, None, &headers)
}

async fn page(State(site): State<Site>, Path(seg): Path<String>, headers: HeaderMap) -> Response {
    if seg != INDEX
        && let Some(data) = site.assets.get(&seg)
    {
        return file_response(&seg, data, "public, max-age=3600");
    }
    let room = valid_room(&seg).then_some(seg.as_str());
    index(&*site.assets, room, &headers)
}

fn index(assets: &dyn Assets, room: Option<&str>, headers: &HeaderMap) -> Response {
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
        with_meta(&data, room, origin(headers).as_deref()),
    )
        .into_response()
}

const DESCRIPTION: &str =
    "A shared code pad for interviews. Open a room, send the link, type together.";
const ROOM_DESCRIPTION: &str =
    "Join the room and type code together. A shared code pad for interviews; nothing is stored.";

/// `scheme://host` for absolute link-preview URLs, from `Host` and the
/// proxy's `X-Forwarded-Proto`. None when the host looks wrong, in which case
/// the tags fall back to relative URLs.
fn origin(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let ok = |c: u8| c.is_ascii_alphanumeric() || b".-:[]".contains(&c);
    if host.is_empty() || host.len() > 255 || !host.bytes().all(ok) {
        return None;
    }
    let https = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .is_some_and(|p| p.trim().eq_ignore_ascii_case("https"));
    Some(format!("{}://{host}", if https { "https" } else { "http" }))
}

/// Link-preview tags for the front page or a room, inserted before `</head>`.
/// `room` must already be a valid room name, and `origin` a checked origin,
/// so neither needs escaping.
fn meta_tags(room: Option<&str>, origin: Option<&str>) -> String {
    let base = origin.unwrap_or("");
    let (title, description, url, image) = match room {
        Some(r) => (
            format!("{r} · quorum"),
            ROOM_DESCRIPTION,
            format!("{base}/{r}"),
            format!("{base}/og/{r}.png"),
        ),
        None => (
            "quorum".to_owned(),
            DESCRIPTION,
            format!("{base}/"),
            format!("{base}/og.png"),
        ),
    };
    let alt = match room {
        Some(r) => format!("quorum room {r}"),
        None => "quorum".to_owned(),
    };
    let mut tags = format!(
        "<meta property=\"og:type\" content=\"website\" />\
<meta property=\"og:site_name\" content=\"quorum\" />\
<meta property=\"og:title\" content=\"{title}\" />\
<meta property=\"og:description\" content=\"{description}\" />\
<meta property=\"og:url\" content=\"{url}\" />\
<meta property=\"og:image\" content=\"{image}\" />\
<meta property=\"og:image:type\" content=\"image/png\" />\
<meta property=\"og:image:width\" content=\"{w}\" />\
<meta property=\"og:image:height\" content=\"{h}\" />\
<meta property=\"og:image:alt\" content=\"{alt}\" />\
<meta name=\"twitter:card\" content=\"summary_large_image\" />",
        w = og::WIDTH,
        h = og::HEIGHT,
    );
    if room.is_some() {
        tags.push_str("<meta name=\"robots\" content=\"noindex\" />");
    }
    tags
}

/// The index page with link-preview tags and, for a room, its name in the title.
fn with_meta(html: &[u8], room: Option<&str>, origin: Option<&str>) -> Vec<u8> {
    let Ok(html) = std::str::from_utf8(html) else {
        return html.to_vec();
    };
    let Some(at) = html.find("</head>") else {
        return html.as_bytes().to_vec();
    };
    let head = match room {
        Some(r) => html[..at].replacen(
            "<title>quorum</title>",
            &format!("<title>{r} · quorum</title>"),
            1,
        ),
        None => html[..at].to_owned(),
    };
    format!("{head}{}{}", meta_tags(room, origin), &html[at..]).into_bytes()
}

async fn home_card(State(site): State<Site>) -> Response {
    card(&site, None).await
}

async fn room_card(State(site): State<Site>, Path(file): Path<String>) -> Response {
    match file.strip_suffix(".png") {
        Some(room) if valid_room(room) => card(&site, Some(room)).await,
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn card(site: &Site, room: Option<&str>) -> Response {
    let Some(png) = site.cards.get(room).await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, "1")],
        )
            .into_response();
    };
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        png.to_vec(),
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
        assert!(body.starts_with("User-agent: *\nAllow: /$\nDisallow: /\n\n"));
        assert!(body.contains("User-agent: Twitterbot\n"));
        assert!(body.ends_with("Allow: /\n"));
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

    const PAGE: &[u8] = b"<html><head><title>quorum</title></head><body></body></html>";

    fn page_assets() -> Arc<dyn Assets> {
        let mut m: HashMap<&'static str, &'static [u8]> = HashMap::new();
        m.insert("index.html", PAGE);
        Arc::new(MapAssets(m))
    }

    async fn get_with(
        a: Arc<dyn Assets>,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut req = Request::builder().method(Method::GET).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let res = routes(a)
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        (
            parts.status,
            parts.headers,
            body.collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    #[tokio::test]
    async fn room_page_has_absolute_preview_tags() {
        let hd = [("host", "quorum.example"), ("x-forwarded-proto", "https")];
        let (s, _, body) = get_with(page_assets(), "/kernel-panic-42", &hd).await;
        let body = String::from_utf8(body).unwrap();
        assert_eq!(s, StatusCode::OK);
        assert!(body.contains("<title>kernel-panic-42 · quorum</title>"));
        assert!(body.contains(r#"<meta property="og:image" content="https://quorum.example/og/kernel-panic-42.png" />"#));
        assert!(body.contains(
            r#"<meta property="og:url" content="https://quorum.example/kernel-panic-42" />"#
        ));
        assert!(body.contains(r#"<meta name="twitter:card" content="summary_large_image" />"#));
        assert!(body.contains(r#"<meta name="robots" content="noindex" />"#));
        assert!(body.find("og:image").unwrap() < body.find("</head>").unwrap());
    }

    #[tokio::test]
    async fn front_page_uses_the_generic_card_and_stays_indexable() {
        let (_, _, body) = get_with(page_assets(), "/", &[("host", "localhost:20481")]).await;
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains("<title>quorum</title>"));
        assert!(body.contains(r#"content="http://localhost:20481/og.png""#));
        assert!(!body.contains("noindex"));
    }

    #[tokio::test]
    async fn bad_host_falls_back_to_relative_urls() {
        let (_, _, body) = get_with(page_assets(), "/room-1", &[("host", "evil\"><script>")]).await;
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains(r#"<meta property="og:image" content="/og/room-1.png" />"#));
        assert!(!body.contains("<script>"));
    }

    #[tokio::test]
    async fn non_room_segment_gets_generic_tags() {
        let (_, _, body) = get_with(page_assets(), "/Not%20A%20Room", &[]).await;
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains("<title>quorum</title>"));
        assert!(body.contains(r#"content="/og.png""#));
    }

    #[tokio::test]
    async fn cards_are_pngs() {
        for uri in ["/og.png", "/og/kernel-panic-42.png"] {
            let (s, hd, body) = get_with(page_assets(), uri, &[]).await;
            assert_eq!(s, StatusCode::OK, "{uri}");
            assert_eq!(h(&hd, "content-type"), "image/png");
            assert_eq!(h(&hd, "cache-control"), "public, max-age=86400");
            assert_eq!(&body[..4], b"\x89PNG");
        }
    }

    #[tokio::test]
    async fn bad_card_names_are_404() {
        for uri in [
            "/og/Kernel.png",
            "/og/room-1.jpg",
            "/og/room-1",
            "/og/-x.png",
        ] {
            let (s, _, _) = get_with(page_assets(), uri, &[]).await;
            assert_eq!(s, StatusCode::NOT_FOUND, "{uri}");
        }
    }
}
