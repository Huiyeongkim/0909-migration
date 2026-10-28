//! Order-cancellation boundary. Implemented by the `order-cancel` unit.

use super::OrderState;
use axum::Router;

pub(super) fn router() -> Router<OrderState> {
    Router::new()
}
