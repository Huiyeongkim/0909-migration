use crate::group_purchase::GroupPurchaseRepository;
use axum::Router;

pub fn router(_group_purchases: GroupPurchaseRepository) -> Router {
    Router::new()
}
