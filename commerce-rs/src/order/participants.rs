//! Internal participant boundary. Implemented by the participant migration unit.

use super::OrderState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";

pub(super) fn router() -> Router<OrderState> {
    Router::new().route(
        "/internal/orders/{group_purchase_id}/participants",
        get(get_participants),
    )
}

async fn get_participants(
    State(state): State<OrderState>,
    Path(group_purchase_id): Path<String>,
) -> Response {
    let Ok(group_purchase_id) = Uuid::parse_str(&group_purchase_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": 400,
                "data": null,
                "message": INVALID_PARAMETER_MESSAGE,
            })),
        )
            .into_response();
    };
    let participants = state.orders.transact(|orders| {
        orders
            .iter()
            .filter(|order| {
                order.group_purchase_id == group_purchase_id
                    && order.status == "PAYMENT_COMPLETED"
                    && order.deleted_at.is_none()
            })
            .map(|order| order.member_id)
            .collect::<Vec<_>>()
    });
    Json(participants).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository, group_purchase::GroupPurchaseRepository, local_auth::AuthRepository,
        order::Order, LocalProductRepository,
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    fn order(
        member_id: Uuid,
        group_purchase_id: Uuid,
        status: &'static str,
        deleted: bool,
    ) -> Order {
        Order {
            order_id: Uuid::new_v4(),
            order_number: "ORD-261028-TEST0001".to_owned(),
            quantity: 1,
            price: 1_000,
            paid_price: 1_000,
            status,
            payment_method: None,
            paid_at: None,
            member_id,
            address: "서울시 테스트 주소".to_owned(),
            address_detail: "101호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "구매자".to_owned(),
            seller_id: Uuid::new_v4(),
            group_purchase_id,
            idempotency_key: Uuid::new_v4().to_string(),
            expired_at: "2026-10-28T00:10:00Z".to_owned(),
            created_at: "2026-10-28T00:00:00Z".to_owned(),
            updated_at: None,
            deleted_at: deleted.then(|| "2026-10-28T00:05:00Z".to_owned()),
        }
    }

    fn state(orders: impl IntoIterator<Item = Order>) -> OrderState {
        let repository = super::super::OrderRepository::empty();
        repository.transact(|stored| stored.extend(orders));
        OrderState {
            orders: repository,
            carts: CartRepository::empty(),
            group_purchases: GroupPurchaseRepository::empty(),
            auth: AuthRepository::default(),
            products: LocalProductRepository::new([]),
        }
    }

    async fn get(state: OrderState, path: &str) -> (StatusCode, serde_json::Value) {
        let response = router()
            .with_state(state)
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn returns_only_non_deleted_paid_participants_and_preserves_duplicates() {
        let purchase_id = Uuid::new_v4();
        let other_purchase = Uuid::new_v4();
        let repeated_member = Uuid::new_v4();
        let excluded_member = Uuid::new_v4();
        let state = state([
            order(repeated_member, purchase_id, "PAYMENT_COMPLETED", false),
            order(repeated_member, purchase_id, "PAYMENT_COMPLETED", false),
            order(excluded_member, purchase_id, "PENDING", false),
            order(excluded_member, purchase_id, "CONFIRMED", false),
            order(excluded_member, purchase_id, "PAYMENT_COMPLETED", true),
            order(excluded_member, other_purchase, "PAYMENT_COMPLETED", false),
        ]);

        let (status, body) = get(
            state,
            &format!("/internal/orders/{purchase_id}/participants"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!([repeated_member.to_string(), repeated_member.to_string()])
        );
    }

    #[tokio::test]
    async fn returns_an_empty_bare_array_when_no_paid_participant_exists() {
        let purchase_id = Uuid::new_v4();
        let (status, body) = get(
            state([]),
            &format!("/internal/orders/{purchase_id}/participants"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!([]));
    }

    #[tokio::test]
    async fn invalid_group_purchase_id_uses_the_standard_error_envelope() {
        let (status, body) = get(state([]), "/internal/orders/not-a-uuid/participants").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            json!({
                "status": 400,
                "data": null,
                "message": INVALID_PARAMETER_MESSAGE,
            })
        );
    }
}
