//! Order read boundary. Implemented by the `order-read` migration unit.

use super::OrderState;
use axum::Router;

pub(super) fn router() -> Router<OrderState> {
    Router::new()
}
