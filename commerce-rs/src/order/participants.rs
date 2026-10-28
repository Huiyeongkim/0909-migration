//! Internal participant boundary. Implemented by the participant migration unit.

use super::OrderState;
use axum::Router;

pub(super) fn router() -> Router<OrderState> {
    Router::new()
}
