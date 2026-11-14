pub mod hub;
pub mod limits;
pub mod net;
pub mod rate;
pub mod room;
pub mod web;
pub mod ws;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::hub::Hub;
use crate::web::Assets;

pub const VERSION: &str = match option_env!("QUORUM_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// The whole app: static routes plus the WebSocket endpoint.
pub fn app(hub: Arc<Hub>, assets: Arc<dyn Assets>) -> Router {
    app_with(hub, assets, ws::Timing::default())
}

/// `app` with explicit liveness timings (for tests).
pub fn app_with(hub: Arc<Hub>, assets: Arc<dyn Assets>, timing: ws::Timing) -> Router {
    let ws = Router::new()
        .route("/ws/{room}", get(ws::handler))
        .with_state(ws::AppState { hub, timing });
    web::routes(assets).merge(ws)
}

/// Serves `app` on `listener` until `shutdown` resolves.
pub async fn serve(
    listener: tokio::net::TcpListener,
    hub: Arc<Hub>,
    assets: Arc<dyn Assets>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    serve_with(listener, hub, assets, ws::Timing::default(), shutdown).await
}

/// `serve` with explicit liveness timings (for tests).
pub async fn serve_with(
    listener: tokio::net::TcpListener,
    hub: Arc<Hub>,
    assets: Arc<dyn Assets>,
    timing: ws::Timing,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app =
        app_with(hub, assets, timing).into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}
