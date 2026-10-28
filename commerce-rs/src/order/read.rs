use super::{Order, OrderState};
use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const ORDER_NOT_FOUND_MESSAGE: &str = "주문을 찾을 수 없습니다.";
const ORDER_ACCESS_DENIED_MESSAGE: &str = "본인의 주문내역만 조회할 수 있습니다.";
const ORDER_DETAIL_MESSAGE: &str = "주문 상세 조회가 완료 되었습니다.";
const CONSUMER_LIST_MESSAGE: &str = "주문 목록 조회(구매자)가 완료 되었습니다.";
const SELLER_LIST_MESSAGE: &str = "주문 목록 조회(판매자)가 완료 되었습니다.";

#[derive(Deserialize)]
struct ListQuery {
    page: Option<usize>,
    size: Option<usize>,
    sort: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderDetailInfo {
    order_id: Uuid,
    order_number: String,
    quantity: i32,
    price: i64,
    status: &'static str,
    member_id: Uuid,
    address: String,
    address_detail: String,
    postal_code: String,
    receiver_name: String,
    seller_id: Uuid,
    group_purchase_id: Uuid,
    created_at: String,
    updated_at: Option<String>,
    deleted_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderInfo {
    order_id: Uuid,
    order_number: String,
    status: &'static str,
    quantity: i32,
    price: i64,
    total_amount: i64,
    group_purchase_id: Uuid,
    group_purchase_name: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderPage {
    content: Vec<OrderInfo>,
    total_pages: usize,
    total_elements: usize,
    first: bool,
    last: bool,
    size: usize,
    number_of_elements: usize,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

impl From<Order> for OrderDetailInfo {
    fn from(order: Order) -> Self {
        Self {
            order_id: order.order_id,
            order_number: order.order_number,
            quantity: order.quantity,
            price: order.price,
            status: order.status,
            member_id: order.member_id,
            address: order.address,
            address_detail: order.address_detail,
            postal_code: order.postal_code,
            receiver_name: order.receiver_name,
            seller_id: order.seller_id,
            group_purchase_id: order.group_purchase_id,
            created_at: order.created_at,
            updated_at: order.updated_at,
            deleted_at: order.deleted_at,
        }
    }
}

pub(super) fn router() -> Router<OrderState> {
    Router::new()
        .route("/api/orders/consumer", get(list_consumer_orders))
        .route("/api/orders/seller", get(list_seller_orders))
        .route("/api/orders/{order_id}", get(get_order))
}

async fn get_order(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let order = state.orders.transact(|orders| {
        orders
            .iter()
            .find(|order| order.order_id == order_id && order.deleted_at.is_none())
            .cloned()
    });
    let Some(order) = order else {
        return error(StatusCode::NOT_FOUND, ORDER_NOT_FOUND_MESSAGE);
    };
    if order.member_id != member_id && order.seller_id != member_id {
        return error(StatusCode::FORBIDDEN, ORDER_ACCESS_DENIED_MESSAGE);
    }
    api_response(
        StatusCode::OK,
        Some(OrderDetailInfo::from(order)),
        ORDER_DETAIL_MESSAGE,
    )
}

async fn list_consumer_orders(
    State(state): State<OrderState>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    list_orders(state, headers, query, false).await
}

async fn list_seller_orders(
    State(state): State<OrderState>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    list_orders(state, headers, query, true).await
}

async fn list_orders(
    state: OrderState,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
    seller_view: bool,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
    };
    let page = query.page.unwrap_or(0);
    let size = query.size.unwrap_or(20);
    if size == 0 {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    }
    let descending = if seller_view {
        false
    } else {
        match query.sort.as_deref().map(parse_created_at_sort).transpose() {
            Ok(sort) => sort.unwrap_or(true),
            Err(()) => return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
        }
    };
    let (orders, total_elements) = state.orders.transact(|orders| {
        let mut orders = orders
            .iter()
            .filter(|order| {
                order.deleted_at.is_none()
                    && if seller_view {
                        order.seller_id == member_id
                    } else {
                        order.member_id == member_id
                    }
            })
            .cloned()
            .collect::<Vec<_>>();
        orders.sort_by(|left, right| {
            let order = left.created_at.cmp(&right.created_at);
            if descending {
                order.reverse()
            } else {
                order
            }
        });
        let total = orders.len();
        let content = orders
            .into_iter()
            .skip(page.saturating_mul(size))
            .take(size)
            .collect::<Vec<_>>();
        (content, total)
    });
    let content = orders
        .into_iter()
        .map(|order| OrderInfo {
            order_id: order.order_id,
            order_number: order.order_number,
            status: order.status,
            quantity: order.quantity,
            price: order.price,
            total_amount: order.price.wrapping_mul(i64::from(order.quantity)),
            group_purchase_id: order.group_purchase_id,
            group_purchase_name: state.group_purchases.order_title(order.group_purchase_id),
            created_at: order.created_at,
        })
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let total_pages = total_elements.div_ceil(size);
    api_response(
        StatusCode::OK,
        Some(OrderPage {
            content,
            total_pages,
            total_elements,
            first: page == 0,
            last: total_pages == 0 || page.saturating_add(1) >= total_pages,
            size,
            number_of_elements,
        }),
        if seller_view {
            SELLER_LIST_MESSAGE
        } else {
            CONSUMER_LIST_MESSAGE
        },
    )
}

fn parse_created_at_sort(value: &str) -> Result<bool, ()> {
    let (field, direction) = value.split_once(',').ok_or(())?;
    if field != "createdAt" {
        return Err(());
    }
    if direction.eq_ignore_ascii_case("desc") {
        Ok(true)
    } else if direction.eq_ignore_ascii_case("asc") {
        Ok(false)
    } else {
        Err(())
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

fn api_response<T: Serialize>(
    status: StatusCode,
    data: Option<T>,
    message: &'static str,
) -> Response {
    (
        status,
        Json(ApiResponse {
            status: status.as_u16(),
            data,
            message,
        }),
    )
        .into_response()
}

fn error(status: StatusCode, message: &'static str) -> Response {
    api_response::<()>(status, None, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository,
        group_purchase::GroupPurchaseRepository,
        local_auth::AuthRepository,
        order::{NewOrder, OrderRepository},
        LocalProductRepository,
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn order(buyer_id: Uuid, seller_id: Uuid, purchase_id: Uuid, created_at: &str) -> Order {
        let mut order = Order::create(NewOrder {
            quantity: 2,
            unit_price: 8_000,
            member_id: buyer_id,
            address: "서울시 테스트 주소".to_owned(),
            address_detail: "101호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "구매자".to_owned(),
            seller_id,
            group_purchase_id: purchase_id,
            idempotency_key: Uuid::new_v4().to_string(),
        });
        order.created_at = created_at.to_owned();
        order
    }

    fn state(
        orders: impl IntoIterator<Item = Order>,
        purchases: GroupPurchaseRepository,
    ) -> OrderState {
        let repository = OrderRepository::empty();
        repository.transact(|stored| stored.extend(orders));
        OrderState {
            orders: repository,
            carts: CartRepository::empty(),
            group_purchases: purchases,
            auth: AuthRepository::default(),
            products: Arc::new(LocalProductRepository::new([])),
        }
    }

    async fn get(state: OrderState, member_id: Uuid, uri: &str) -> (StatusCode, Value) {
        let response = router()
            .with_state(state)
            .oneshot(
                Request::builder()
                    .uri(uri)
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
    async fn detail_allows_buyer_and_seller_but_hides_deleted_and_foreign_orders() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let visible = order(buyer_id, seller_id, purchase_id, "2026-01-01T00:00:00Z");
        let visible_id = visible.order_id;
        let mut deleted = order(buyer_id, seller_id, purchase_id, "2026-01-02T00:00:00Z");
        deleted.deleted_at = Some("2026-01-03T00:00:00Z".to_owned());
        let deleted_id = deleted.order_id;
        let state = state([visible, deleted], GroupPurchaseRepository::empty());

        let (buyer_status, buyer) = get(
            state.clone(),
            buyer_id,
            &format!("/api/orders/{visible_id}"),
        )
        .await;
        assert_eq!(buyer_status, StatusCode::OK);
        assert_eq!(buyer["message"], ORDER_DETAIL_MESSAGE);
        assert_eq!(buyer["data"]["orderId"], visible_id.to_string());
        assert_eq!(buyer["data"]["price"], 8_000);
        assert_eq!(buyer["data"]["status"], "PENDING");
        assert_eq!(buyer["data"]["deletedAt"], Value::Null);

        let (seller_status, _) = get(
            state.clone(),
            seller_id,
            &format!("/api/orders/{visible_id}"),
        )
        .await;
        assert_eq!(seller_status, StatusCode::OK);

        let (foreign_status, foreign) = get(
            state.clone(),
            Uuid::new_v4(),
            &format!("/api/orders/{visible_id}"),
        )
        .await;
        assert_eq!(foreign_status, StatusCode::FORBIDDEN);
        assert_eq!(foreign["message"], ORDER_ACCESS_DENIED_MESSAGE);

        let (deleted_status, deleted) =
            get(state, buyer_id, &format!("/api/orders/{deleted_id}")).await;
        assert_eq!(deleted_status, StatusCode::NOT_FOUND);
        assert_eq!(deleted["message"], ORDER_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn consumer_list_filters_soft_deleted_orders_and_resolves_nullable_purchase_names() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let known_purchase = Uuid::new_v4();
        let missing_purchase = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let older = order(buyer_id, seller_id, known_purchase, "2026-01-01T00:00:00Z");
        let newer = order(
            buyer_id,
            seller_id,
            missing_purchase,
            "2026-01-03T00:00:00Z",
        );
        let mut deleted = order(buyer_id, seller_id, known_purchase, "2026-01-04T00:00:00Z");
        deleted.deleted_at = Some("2026-01-05T00:00:00Z".to_owned());
        let other_buyer = order(
            Uuid::new_v4(),
            seller_id,
            known_purchase,
            "2026-01-02T00:00:00Z",
        );
        let purchases = GroupPurchaseRepository::for_order_tests([(
            known_purchase,
            product_id,
            seller_id,
            8_000,
            0,
            10,
            "OPEN",
        )]);
        let state = state([older, newer, deleted, other_buyer], purchases);

        let (status, response) = get(state.clone(), buyer_id, "/api/orders/consumer").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["message"], CONSUMER_LIST_MESSAGE);
        assert_eq!(response["data"]["totalElements"], 2);
        assert_eq!(response["data"]["totalPages"], 1);
        assert_eq!(response["data"]["numberOfElements"], 2);
        assert_eq!(
            response["data"]["content"][0]["groupPurchaseId"],
            missing_purchase.to_string()
        );
        assert_eq!(
            response["data"]["content"][0]["groupPurchaseName"],
            Value::Null
        );
        assert_eq!(
            response["data"]["content"][1]["groupPurchaseName"],
            "테스트 공동구매"
        );
        assert_eq!(response["data"]["content"][1]["totalAmount"], 16_000);

        let (_, ascending) = get(state, buyer_id, "/api/orders/consumer?sort=createdAt,asc").await;
        assert_eq!(
            ascending["data"]["content"][0]["groupPurchaseId"],
            known_purchase.to_string()
        );
    }

    #[tokio::test]
    async fn seller_list_forces_created_at_ascending_then_pages() {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let older = order(buyer_id, seller_id, purchase_id, "2026-01-01T00:00:00Z");
        let older_id = older.order_id;
        let newer = order(buyer_id, seller_id, purchase_id, "2026-01-03T00:00:00Z");
        let other_seller = order(
            buyer_id,
            Uuid::new_v4(),
            purchase_id,
            "2025-01-01T00:00:00Z",
        );
        let state = state(
            [older, newer, other_seller],
            GroupPurchaseRepository::empty(),
        );

        let (status, response) = get(
            state,
            seller_id,
            "/api/orders/seller?page=0&size=1&sort=createdAt,desc",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["message"], SELLER_LIST_MESSAGE);
        assert_eq!(response["data"]["totalElements"], 2);
        assert_eq!(response["data"]["totalPages"], 2);
        assert!(response["data"]["first"].as_bool().unwrap());
        assert!(!response["data"]["last"].as_bool().unwrap());
        assert_eq!(response["data"]["numberOfElements"], 1);
        assert_eq!(
            response["data"]["content"][0]["orderId"],
            older_id.to_string()
        );
    }
}
