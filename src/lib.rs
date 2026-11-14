pub mod hub;
pub mod limits;
pub mod net;
pub mod rate;
pub mod room;
pub mod web;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use crate::hub::Hub;
use crate::web::Assets;

pub const VERSION: &str = match option_env!("QUORUM_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// The whole app: static routes plus the WebSocket endpoint.
pub fn app(_hub: Arc<Hub>, assets: Arc<dyn Assets>) -> Router {
    // Stub until the WebSocket handler lands.
    web::routes(assets).route("/ws/{room}", get(|| async { StatusCode::NOT_IMPLEMENTED }))
}
