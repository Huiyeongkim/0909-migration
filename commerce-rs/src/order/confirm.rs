use super::{OrderRepository, OrderSettlement, OrderState, OrderStore};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::patch,
    Json, Router,
};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const ORDER_NOT_FOUND_MESSAGE: &str = "주문을 찾을 수 없습니다.";
const ORDER_ACCESS_DENIED_MESSAGE: &str = "본인의 주문내역만 조회할 수 있습니다.";
const INVALID_STATUS_MESSAGE: &str = "공구 성공 상태의 주문만 구매 확정할 수 있습니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const CONFIRMED_MESSAGE: &str = "구매 확정되었습니다.";

#[derive(Serialize)]
struct ApiResponse {
    status: u16,
    data: Option<()>,
    message: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
enum ConfirmError {
    NotFound,
    Forbidden,
    InvalidStatus,
    SettlementWrite,
}

pub(super) fn router() -> Router<OrderState> {
    Router::new().route(
        "/api/orders/{order_id}/purchase-confirmed",
        patch(confirm_purchase),
    )
}

async fn confirm_purchase(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return response(status, message),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return response(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let result = if state.orders.pool().is_some() {
        state
            .orders
            .confirm_postgres(member_id, order_id)
            .await
            .map_err(|error| match error {
                super::postgres::ConfirmError::NotFound => ConfirmError::NotFound,
                super::postgres::ConfirmError::Forbidden => ConfirmError::Forbidden,
                super::postgres::ConfirmError::InvalidStatus => ConfirmError::InvalidStatus,
                super::postgres::ConfirmError::Database => ConfirmError::SettlementWrite,
            })
    } else {
        state.orders.confirm_purchase(member_id, order_id)
    };
    match result {
        Ok(()) => response(StatusCode::OK, CONFIRMED_MESSAGE),
        Err(ConfirmError::NotFound) => response(StatusCode::NOT_FOUND, ORDER_NOT_FOUND_MESSAGE),
        Err(ConfirmError::Forbidden) => {
            response(StatusCode::FORBIDDEN, ORDER_ACCESS_DENIED_MESSAGE)
        }
        Err(ConfirmError::InvalidStatus) => {
            response(StatusCode::BAD_REQUEST, INVALID_STATUS_MESSAGE)
        }
        Err(ConfirmError::SettlementWrite) => {
            response(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
        }
    }
}

impl OrderRepository {
    fn confirm_purchase(&self, member_id: Uuid, order_id: Uuid) -> Result<(), ConfirmError> {
        self.confirm_purchase_transaction(member_id, order_id, false)
    }

    fn confirm_purchase_transaction(
        &self,
        member_id: Uuid,
        order_id: Uuid,
        fail_settlement_write: bool,
    ) -> Result<(), ConfirmError> {
        self.transact_store(|store| {
            confirm_in_store(store, member_id, order_id, fail_settlement_write)
        })
    }
}

fn confirm_in_store(
    store: &mut OrderStore,
    member_id: Uuid,
    order_id: Uuid,
    fail_settlement_write: bool,
) -> Result<(), ConfirmError> {
    let order_index = store
        .orders
        .iter()
        .position(|order| order.order_id == order_id)
        .ok_or(ConfirmError::NotFound)?;
    let order = &store.orders[order_index];
    if order.member_id != member_id {
        return Err(ConfirmError::Forbidden);
    }
    if order.status != "PAYMENT_COMPLETED" {
        return Err(ConfirmError::InvalidStatus);
    }

    let settlement = OrderSettlement::confirmed(order);
    if fail_settlement_write {
        return Err(ConfirmError::SettlementWrite);
    }
    let updated_at = now();
    let order = &mut store.orders[order_index];
    order.status = "CONFIRMED";
    order.updated_at = Some(updated_at);
    store.settlements.push(settlement);
    Ok(())
}

impl OrderSettlement {
    fn confirmed(order: &super::Order) -> Self {
        let platform_fee_rate = 0.2;
        let platform_fee = (order.paid_price as f64 * platform_fee_rate) as i64;
        Self {
            order_settlement_id: Uuid::new_v4(),
            seller_id: order.seller_id,
            group_purchase_id: order.group_purchase_id,
            order_id: order.order_id,
            status: "COMPLETED",
            order_amount: order.paid_price,
            platform_fee_rate,
            platform_fee,
            settlement_amount: order.paid_price - platform_fee,
            created_at: now(),
            settled_at: None,
        }
    }
}

fn member_id(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let Some(value) = headers.get("x-member-id") else {
        return Err((StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE));
    };
    let value = value
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    Uuid::parse_str(value).map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))
}

fn response(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        Json(ApiResponse {
            status: status.as_u16(),
            data: None,
            message,
        }),
    )
        .into_response()
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting must succeed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository,
        group_purchase::GroupPurchaseRepository,
        local_auth::AuthRepository,
        order::{NewOrder, Order},
        LocalProductRepository,
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use std::thread;
    use tower::ServiceExt;

    fn order(member_id: Uuid, status: &'static str) -> Order {
        let mut order = Order::create(NewOrder {
            quantity: 3,
            unit_price: 6_667,
            member_id,
            address: "서울시 테스트 주소".to_owned(),
            address_detail: "101호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "구매자".to_owned(),
            seller_id: Uuid::new_v4(),
            group_purchase_id: Uuid::new_v4(),
            idempotency_key: Uuid::new_v4().to_string(),
        });
        order.status = status;
        order
    }

    fn state(order: Order) -> OrderState {
        let orders = OrderRepository::empty();
        orders.transact(|stored| stored.push(order));
        OrderState {
            orders,
            carts: CartRepository::empty(),
            group_purchases: GroupPurchaseRepository::empty(),
            auth: AuthRepository::default(),
            products: LocalProductRepository::new([]),
        }
    }

    async fn patch(state: OrderState, member_id: Uuid, order_id: &str) -> (StatusCode, Value) {
        let response = router()
            .with_state(state)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/orders/{order_id}/purchase-confirmed"))
                    .header("x-member-id", member_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn confirms_paid_order_and_creates_completed_settlement() {
        let buyer_id = Uuid::new_v4();
        let mut paid = order(buyer_id, "PAYMENT_COMPLETED");
        paid.deleted_at = Some("2026-10-28T00:00:00Z".to_owned());
        let order_id = paid.order_id;
        let seller_id = paid.seller_id;
        let purchase_id = paid.group_purchase_id;
        let state = state(paid);

        let (status, body) = patch(state.clone(), buyer_id, &order_id.to_string()).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({"status": 200, "data": null, "message": CONFIRMED_MESSAGE})
        );
        let (repeat_status, repeat) = patch(state.clone(), buyer_id, &order_id.to_string()).await;
        assert_eq!(repeat_status, StatusCode::BAD_REQUEST);
        assert_eq!(repeat["message"], INVALID_STATUS_MESSAGE);
        state.orders.transact_store(|store| {
            assert_eq!(store.orders[0].status, "CONFIRMED");
            assert!(store.orders[0].updated_at.is_some());
            assert_eq!(store.settlements.len(), 1);
            let settlement = &store.settlements[0];
            assert_eq!(settlement.order_id, order_id);
            assert_ne!(settlement.order_settlement_id, Uuid::nil());
            assert_eq!(settlement.seller_id, seller_id);
            assert_eq!(settlement.group_purchase_id, purchase_id);
            assert_eq!(settlement.order_amount, 20_001);
            assert_eq!(settlement.platform_fee_rate, 0.2);
            assert_eq!(settlement.platform_fee, 4_000);
            assert_eq!(settlement.settlement_amount, 16_001);
            assert_eq!(settlement.status, "COMPLETED");
            assert!(OffsetDateTime::parse(&settlement.created_at, &Rfc3339).is_ok());
            assert!(settlement.settled_at.is_none());
        });
    }

    #[tokio::test]
    async fn rejects_missing_foreign_and_non_paid_orders_without_a_settlement() {
        let buyer_id = Uuid::new_v4();
        let pending = order(buyer_id, "PENDING");
        let order_id = pending.order_id;
        let state = state(pending);

        let (missing_status, missing) =
            patch(state.clone(), buyer_id, &Uuid::new_v4().to_string()).await;
        assert_eq!(missing_status, StatusCode::NOT_FOUND);
        assert_eq!(missing["message"], ORDER_NOT_FOUND_MESSAGE);

        let (foreign_status, foreign) =
            patch(state.clone(), Uuid::new_v4(), &order_id.to_string()).await;
        assert_eq!(foreign_status, StatusCode::FORBIDDEN);
        assert_eq!(foreign["message"], ORDER_ACCESS_DENIED_MESSAGE);

        let (state_status, invalid) = patch(state.clone(), buyer_id, &order_id.to_string()).await;
        assert_eq!(state_status, StatusCode::BAD_REQUEST);
        assert_eq!(invalid["message"], INVALID_STATUS_MESSAGE);
        state.orders.transact_store(|store| {
            assert_eq!(store.orders[0].status, "PENDING");
            assert!(store.settlements.is_empty());
        });
    }

    #[test]
    fn serializes_concurrent_confirmation_and_rolls_back_a_failed_settlement_write() {
        let buyer_id = Uuid::new_v4();
        let paid = order(buyer_id, "PAYMENT_COMPLETED");
        let order_id = paid.order_id;
        let repository = OrderRepository::empty();
        repository.transact(|orders| orders.push(paid));

        let results = thread::scope(|scope| {
            let first = {
                let repository = repository.clone();
                scope.spawn(move || repository.confirm_purchase(buyer_id, order_id))
            };
            let second = {
                let repository = repository.clone();
                scope.spawn(move || repository.confirm_purchase(buyer_id, order_id))
            };
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(ConfirmError::InvalidStatus)))
                .count(),
            1
        );
        repository.transact_store(|store| assert_eq!(store.settlements.len(), 1));

        let another = order(buyer_id, "PAYMENT_COMPLETED");
        let another_id = another.order_id;
        repository.transact(|orders| orders.push(another));
        let result = repository.confirm_purchase_transaction(buyer_id, another_id, true);
        assert_eq!(result, Err(ConfirmError::SettlementWrite));
        repository.transact_store(|store| {
            let order = store
                .orders
                .iter()
                .find(|order| order.order_id == another_id)
                .unwrap();
            assert_eq!(order.status, "PAYMENT_COMPLETED");
            assert_eq!(store.settlements.len(), 1);
        });
    }
}
