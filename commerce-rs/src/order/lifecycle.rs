//! Order expiration and local-payment boundary. Implemented by the lifecycle unit.

use super::OrderState;
use axum::Router;

pub(super) fn router() -> Router<OrderState> {
    Router::new()
}
