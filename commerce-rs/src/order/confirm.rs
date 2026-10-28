//! Purchase-confirmation boundary. Implemented by the `order-confirm` unit.

use super::OrderState;
use axum::Router;

pub(super) fn router() -> Router<OrderState> {
    Router::new()
}
