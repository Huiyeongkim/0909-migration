use super::GroupPurchaseState;
use axum::Router;

pub(super) fn router() -> Router<GroupPurchaseState> {
    Router::new()
}
