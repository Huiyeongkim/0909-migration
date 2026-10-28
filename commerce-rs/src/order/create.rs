use super::{NewOrder, Order, OrderState};
use crate::{
    cart::OrderCartError,
    group_purchase::participate::{
        OrderParticipation, OrderParticipationError, OrderParticipationTransactionError,
    },
};
use axum::{
    extract::{rejection::JsonRejection, Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const MEMBER_NOT_FOUND_MESSAGE: &str = "회원을 찾을 수 없습니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const CART_NOT_FOUND_MESSAGE: &str = "장바구니에서 해당 공동구매를 찾을 수 없습니다.";
const NOT_CART_OWNER_MESSAGE: &str = "카트에 대한 권한이 없습니다.";
const CART_EMPTY_MESSAGE: &str = "장바구니가 비어있습니다.";
const PURCHASE_NOT_FOUND_MESSAGE: &str = "공동구매를 찾을 수 없습니다.";
const PURCHASE_NOT_OPEN_MESSAGE: &str = "공동 구매가 시작하지 않았습니다.";
const PURCHASE_EXPIRED_MESSAGE: &str = "종료된 공동 구매입니다.";
const PURCHASE_REACHED_MESSAGE: &str = "공동구매 참여 인원이 최대입니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";
const DUPLICATE_REQUEST_MESSAGE: &str = "이미 일부 처리된 요청입니다.";
const DIRECT_CREATED_MESSAGE: &str = "주문이 생성되었습니다.";
const CART_CREATED_MESSAGE: &str = "장바구니 주문이 생성되었습니다.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectOrderRequest {
    quantity: Option<i32>,
    address: Option<String>,
    address_detail: Option<String>,
    postal_code: Option<String>,
    receiver_name: Option<String>,
    seller_id: Option<Uuid>,
    group_purchase_id: Option<Uuid>,
    request_id: Option<String>,
}

struct ValidDirectOrder {
    quantity: i32,
    address: String,
    address_detail: String,
    postal_code: String,
    receiver_name: String,
    group_purchase_id: Uuid,
    request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartOrderRequest {
    cart_ids: Option<Vec<Uuid>>,
    address: Option<String>,
    address_detail: Option<String>,
    postal_code: Option<String>,
    receiver_name: Option<String>,
    request_id: Option<String>,
}

struct ValidCartOrder {
    cart_ids: Vec<Uuid>,
    address: String,
    address_detail: String,
    postal_code: String,
    receiver_name: String,
    request_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderRegisterInfo {
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
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

#[derive(Debug)]
struct CreateError {
    status: StatusCode,
    message: &'static str,
}

impl From<&Order> for OrderRegisterInfo {
    fn from(order: &Order) -> Self {
        Self {
            order_id: order.order_id,
            order_number: order.order_number.clone(),
            quantity: order.quantity,
            price: order.price,
            status: order.status,
            member_id: order.member_id,
            address: order.address.clone(),
            address_detail: order.address_detail.clone(),
            postal_code: order.postal_code.clone(),
            receiver_name: order.receiver_name.clone(),
            seller_id: order.seller_id,
            group_purchase_id: order.group_purchase_id,
            created_at: order.created_at.clone(),
            updated_at: order.updated_at.clone(),
            deleted_at: order.deleted_at.clone(),
        }
    }
}

pub(super) fn router() -> Router<OrderState> {
    Router::new()
        .route("/api/orders", post(create_direct_order))
        .route("/api/orders/cart", post(create_cart_order))
}

async fn create_direct_order(
    State(state): State<OrderState>,
    headers: HeaderMap,
    payload: Result<Json<DirectOrderRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err(error) => return error.into_response(),
    };
    let Ok(Json(request)) = payload else {
        return create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE).into_response();
    };
    let request = match validate_direct_order(request) {
        Some(request) => request,
        None => {
            return create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE)
                .into_response();
        }
    };
    if let Err(error) = validate_member(&state, member_id).await {
        return error.into_response();
    }

    let result = state.orders.transact(|orders| {
        if let Some(existing) = orders.iter().find(|order| {
            order.member_id == member_id && order.idempotency_key == request.request_id
        }) {
            return Ok(existing.clone());
        }
        state
            .group_purchases
            .with_order_participation(
                &[OrderParticipation {
                    group_purchase_id: request.group_purchase_id,
                    quantity: request.quantity,
                }],
                |product_id| state.products.find_by_id(product_id).is_some(),
                |purchases| {
                    let purchase = &purchases[0];
                    let order = Order::create(NewOrder {
                        quantity: request.quantity,
                        unit_price: purchase.discounted_price,
                        member_id,
                        address: request.address.clone(),
                        address_detail: request.address_detail.clone(),
                        postal_code: request.postal_code.clone(),
                        receiver_name: request.receiver_name.clone(),
                        seller_id: purchase.seller_id,
                        group_purchase_id: purchase.group_purchase_id,
                        idempotency_key: request.request_id.clone(),
                    });
                    orders.push(order.clone());
                    Ok::<_, CreateError>(order)
                },
            )
            .map_err(participation_transaction_error)
    });

    match result {
        Ok(order) => api_response(
            StatusCode::CREATED,
            Some(OrderRegisterInfo::from(&order)),
            DIRECT_CREATED_MESSAGE,
        ),
        Err(error) => error.into_response(),
    }
}

async fn create_cart_order(
    State(state): State<OrderState>,
    headers: HeaderMap,
    payload: Result<Json<CartOrderRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err(error) => return error.into_response(),
    };
    let Ok(Json(request)) = payload else {
        return create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE).into_response();
    };
    let request = match validate_cart_order(request) {
        Some(request) => request,
        None => {
            return create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE)
                .into_response();
        }
    };
    if let Err(error) = validate_member(&state, member_id).await {
        return error.into_response();
    }

    let order_keys = request
        .cart_ids
        .iter()
        .map(|cart_id| format!("{}-{cart_id}", request.request_id))
        .collect::<Vec<_>>();
    let result = state.orders.transact(|orders| {
        let existing = order_keys
            .iter()
            .filter_map(|key| {
                orders
                    .iter()
                    .find(|order| order.member_id == member_id && order.idempotency_key == *key)
                    .cloned()
            })
            .collect::<Vec<_>>();
        if existing.len() == order_keys.len() {
            return Ok(existing);
        }
        if !existing.is_empty() {
            return Err(create_error(
                StatusCode::CONFLICT,
                DUPLICATE_REQUEST_MESSAGE,
            ));
        }

        state
            .carts
            .with_order_carts(member_id, &request.cart_ids, |carts| {
                let participations = carts
                    .iter()
                    .map(|cart| OrderParticipation {
                        group_purchase_id: cart.group_purchase_id,
                        quantity: cart.quantity,
                    })
                    .collect::<Vec<_>>();
                state
                    .group_purchases
                    .with_order_participation(
                        &participations,
                        |product_id| state.products.find_by_id(product_id).is_some(),
                        |purchases| {
                            let created = carts
                                .iter()
                                .zip(purchases)
                                .map(|(cart, purchase)| {
                                    Order::create(NewOrder {
                                        quantity: cart.quantity,
                                        unit_price: purchase.discounted_price,
                                        member_id,
                                        address: request.address.clone(),
                                        address_detail: request.address_detail.clone(),
                                        postal_code: request.postal_code.clone(),
                                        receiver_name: request.receiver_name.clone(),
                                        seller_id: purchase.seller_id,
                                        group_purchase_id: purchase.group_purchase_id,
                                        idempotency_key: format!(
                                            "{}-{}",
                                            request.request_id, cart.cart_id
                                        ),
                                    })
                                })
                                .collect::<Vec<_>>();
                            orders.extend(created.iter().cloned());
                            Ok::<_, CreateError>(created)
                        },
                    )
                    .map_err(participation_transaction_error)
            })
            .map_err(|error| match error {
                Ok(OrderCartError::NotFound) => {
                    create_error(StatusCode::NOT_FOUND, CART_NOT_FOUND_MESSAGE)
                }
                Ok(OrderCartError::Forbidden) => {
                    create_error(StatusCode::FORBIDDEN, NOT_CART_OWNER_MESSAGE)
                }
                Ok(OrderCartError::Empty) => {
                    create_error(StatusCode::BAD_REQUEST, CART_EMPTY_MESSAGE)
                }
                Err(error) => error,
            })
    });

    match result {
        Ok(orders) => api_response(
            StatusCode::CREATED,
            Some(
                orders
                    .iter()
                    .map(OrderRegisterInfo::from)
                    .collect::<Vec<_>>(),
            ),
            CART_CREATED_MESSAGE,
        ),
        Err(error) => error.into_response(),
    }
}

async fn validate_member(state: &OrderState, member_id: Uuid) -> Result<(), CreateError> {
    match state.auth.member_exists(member_id).await {
        Ok(true) => {}
        Ok(false) => {
            return Err(create_error(
                StatusCode::NOT_FOUND,
                MEMBER_NOT_FOUND_MESSAGE,
            ))
        }
        Err(()) => {
            return Err(create_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                INTERNAL_ERROR_MESSAGE,
            ));
        }
    }
    Ok(())
}

fn validate_direct_order(request: DirectOrderRequest) -> Option<ValidDirectOrder> {
    let quantity = request.quantity?;
    if quantity < 1 {
        return None;
    }
    request.seller_id?;
    Some(ValidDirectOrder {
        quantity,
        address: required_string_range(request.address, 5, 100)?,
        address_detail: required_string_range(request.address_detail, 1, 100)?,
        postal_code: required_string_range(request.postal_code, 1, 50)?,
        receiver_name: required_string_range(request.receiver_name, 1, 100)?,
        group_purchase_id: request.group_purchase_id?,
        request_id: required_string_range(request.request_id, 1, 255)?,
    })
}

fn validate_cart_order(request: CartOrderRequest) -> Option<ValidCartOrder> {
    let cart_ids = request.cart_ids?;
    let unique_ids = cart_ids.iter().copied().collect::<HashSet<_>>();
    if cart_ids.is_empty() || unique_ids.len() != cart_ids.len() {
        return None;
    }
    Some(ValidCartOrder {
        cart_ids,
        address: required_string(request.address, 100)?,
        address_detail: required_string(request.address_detail, 100)?,
        postal_code: required_string(request.postal_code, 50)?,
        receiver_name: required_string(request.receiver_name, 100)?,
        request_id: required_string(request.request_id, 255)?,
    })
}

fn required_string(value: Option<String>, max_len: usize) -> Option<String> {
    required_string_range(value, 1, max_len)
}

fn required_string_range(value: Option<String>, min_len: usize, max_len: usize) -> Option<String> {
    value.filter(|value| {
        !value.trim().is_empty() && (min_len..=max_len).contains(&value.chars().count())
    })
}

fn member_id(headers: &HeaderMap) -> Result<Uuid, CreateError> {
    let Some(value) = headers.get("x-member-id") else {
        return Err(create_error(
            StatusCode::UNAUTHORIZED,
            NO_LOGIN_INFO_MESSAGE,
        ));
    };
    let value = value
        .to_str()
        .map_err(|_| create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    Uuid::parse_str(value)
        .map_err(|_| create_error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))
}

fn participation_transaction_error(
    error: OrderParticipationTransactionError<CreateError>,
) -> CreateError {
    match error {
        OrderParticipationTransactionError::Participation(error) => match error {
            OrderParticipationError::NotFound => {
                create_error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE)
            }
            OrderParticipationError::NotOpen => {
                create_error(StatusCode::BAD_REQUEST, PURCHASE_NOT_OPEN_MESSAGE)
            }
            OrderParticipationError::Expired => {
                create_error(StatusCode::BAD_REQUEST, PURCHASE_EXPIRED_MESSAGE)
            }
            OrderParticipationError::CapacityReached => {
                create_error(StatusCode::BAD_REQUEST, PURCHASE_REACHED_MESSAGE)
            }
            OrderParticipationError::ProductNotFound => {
                create_error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE)
            }
        },
        OrderParticipationTransactionError::Commit(error) => error,
    }
}

fn create_error(status: StatusCode, message: &'static str) -> CreateError {
    CreateError { status, message }
}

impl IntoResponse for CreateError {
    fn into_response(self) -> Response {
        api_response::<()>(self.status, None, self.message)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository, group_purchase::GroupPurchaseRepository, local_auth::AuthRepository,
        LocalProductRepository, ProductDetail,
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use std::sync::Arc;
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    use tower::ServiceExt;

    struct Fixture {
        state: OrderState,
        member_id: Uuid,
        cart_id: Uuid,
        purchase_id: Uuid,
        seller_id: Uuid,
    }

    async fn fixture(quantity: i32, current: i32, max: i32) -> Fixture {
        let member_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let auth = AuthRepository::default();
        auth.insert_test_member(member_id).await;
        let carts = CartRepository::empty();
        let cart_id = carts.insert_test_cart(member_id, purchase_id, quantity);
        let group_purchases = GroupPurchaseRepository::for_order_tests([(
            purchase_id,
            product_id,
            seller_id,
            8_000,
            current,
            max,
            "OPEN",
        )]);
        let products = Arc::new(LocalProductRepository::new([product(
            product_id, seller_id,
        )]));
        Fixture {
            state: OrderState {
                orders: super::super::OrderRepository::empty(),
                carts,
                group_purchases,
                auth,
                products,
            },
            member_id,
            cart_id,
            purchase_id,
            seller_id,
        }
    }

    fn product(product_id: Uuid, seller_id: Uuid) -> ProductDetail {
        ProductDetail {
            product_id,
            name: "사과".to_owned(),
            price: 10_000,
            category: "FOOD".to_owned(),
            description: "상품 설명".to_owned(),
            stock: 100,
            original_link: None,
            image_url: None,
            seller_id,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    fn body(cart_ids: &[Uuid], request_id: &str) -> Value {
        json!({
            "cartIds": cart_ids,
            "address": "서울시 테스트 주소",
            "addressDetail": "101호",
            "postalCode": "12345",
            "receiverName": "구매자",
            "requestId": request_id
        })
    }

    fn direct_body(
        purchase_id: Uuid,
        requested_seller_id: Uuid,
        request_id: &str,
        quantity: i32,
    ) -> Value {
        json!({
            "quantity": quantity,
            "address": "서울시 테스트 주소",
            "addressDetail": "101호",
            "postalCode": "12345",
            "receiverName": "구매자",
            "sellerId": requested_seller_id,
            "groupPurchaseId": purchase_id,
            "requestId": request_id
        })
    }

    async fn send(state: OrderState, member_id: Uuid, body: Value) -> (StatusCode, Value) {
        send_to(state, member_id, "/api/orders/cart", body).await
    }

    async fn send_to(
        state: OrderState,
        member_id: Uuid,
        uri: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("x-member-id", member_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = router().with_state(state).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn direct_order_uses_purchase_seller_and_snapshots_pending_price_and_expiry() {
        let fixture = fixture(1, 2, 10).await;
        let untrusted_seller = Uuid::new_v4();
        let before = OffsetDateTime::now_utc();
        let (status, response) = send_to(
            fixture.state.clone(),
            fixture.member_id,
            "/api/orders",
            direct_body(fixture.purchase_id, untrusted_seller, "direct-request", 3),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response["message"], DIRECT_CREATED_MESSAGE);
        assert_eq!(response["data"]["sellerId"], fixture.seller_id.to_string());
        assert_ne!(response["data"]["sellerId"], untrusted_seller.to_string());
        assert_eq!(response["data"]["price"], 8_000);
        assert_eq!(response["data"]["quantity"], 3);
        assert_eq!(response["data"]["status"], "PENDING");
        let order_number = response["data"]["orderNumber"].as_str().unwrap();
        assert!(order_number.starts_with("ORD-"));
        assert_eq!(order_number.len(), 19);
        assert!(order_number[4..10]
            .chars()
            .all(|character| character.is_ascii_digit()));
        assert!(order_number[11..]
            .chars()
            .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit()));
        fixture.state.orders.transact(|orders| {
            let order = &orders[0];
            assert_eq!(order.paid_price, 24_000);
            let expires = OffsetDateTime::parse(&order.expired_at, &Rfc3339).unwrap();
            let created = OffsetDateTime::parse(&order.created_at, &Rfc3339).unwrap();
            assert_eq!(expires - created, time::Duration::minutes(10));
            assert!(created >= before);
        });
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(5)
        );
    }

    #[tokio::test]
    async fn direct_idempotency_is_member_scoped_and_concurrent_retries_share_one_order() {
        let fixture = fixture(1, 0, 20).await;
        let request = direct_body(fixture.purchase_id, Uuid::new_v4(), "shared-key", 2);
        let first_attempt = send_to(
            fixture.state.clone(),
            fixture.member_id,
            "/api/orders",
            request.clone(),
        );
        let second_attempt = send_to(
            fixture.state.clone(),
            fixture.member_id,
            "/api/orders",
            request.clone(),
        );
        let ((first_status, first), (second_status, second)) =
            tokio::join!(first_attempt, second_attempt);

        assert_eq!(first_status, StatusCode::CREATED);
        assert_eq!(second_status, StatusCode::CREATED);
        assert_eq!(first["data"]["orderId"], second["data"]["orderId"]);
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(2)
        );

        let other_member = Uuid::new_v4();
        fixture.state.auth.insert_test_member(other_member).await;
        let (status, other) =
            send_to(fixture.state.clone(), other_member, "/api/orders", request).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_ne!(first["data"]["orderId"], other["data"]["orderId"]);
        fixture
            .state
            .orders
            .transact(|orders| assert_eq!(orders.len(), 2));
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(4)
        );
    }

    #[tokio::test]
    async fn direct_validation_and_product_failure_leave_participation_unchanged() {
        let fixture = fixture(1, 4, 10).await;
        let mut invalid = direct_body(fixture.purchase_id, fixture.seller_id, "invalid-request", 1);
        invalid["requestId"] = Value::Null;
        let (status, _) = send_to(
            fixture.state.clone(),
            fixture.member_id,
            "/api/orders",
            invalid,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let state = OrderState {
            products: Arc::new(LocalProductRepository::new([])),
            ..fixture.state.clone()
        };
        let (status, response) = send_to(
            state.clone(),
            fixture.member_id,
            "/api/orders",
            direct_body(fixture.purchase_id, fixture.seller_id, "missing-product", 2),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(response["message"], PRODUCT_NOT_FOUND_MESSAGE);
        assert_eq!(
            state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(4)
        );
        state.orders.transact(|orders| assert!(orders.is_empty()));
    }

    #[tokio::test]
    async fn direct_capacity_failure_does_not_create_or_participate() {
        let fixture = fixture(1, 9, 10).await;
        let (status, response) = send_to(
            fixture.state.clone(),
            fixture.member_id,
            "/api/orders",
            direct_body(fixture.purchase_id, fixture.seller_id, "over-capacity", 2),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["message"], PURCHASE_REACHED_MESSAGE);
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(9)
        );
        fixture
            .state
            .orders
            .transact(|orders| assert!(orders.is_empty()));
    }

    #[tokio::test]
    async fn cart_order_snapshots_price_pending_expiry_and_soft_zeros_after_success() {
        let fixture = fixture(2, 1, 10).await;
        let before = OffsetDateTime::now_utc();
        let (status, response) = send(
            fixture.state.clone(),
            fixture.member_id,
            body(&[fixture.cart_id], "cart-request"),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(response["message"], CART_CREATED_MESSAGE);
        assert_eq!(response["data"][0]["price"], 8_000);
        assert_eq!(response["data"][0]["quantity"], 2);
        assert_eq!(response["data"][0]["status"], "PENDING");
        assert_eq!(fixture.state.carts.test_quantity(fixture.cart_id), Some(0));
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(3)
        );
        fixture.state.orders.transact(|orders| {
            let order = &orders[0];
            assert_eq!(order.paid_price, 16_000);
            let expires = OffsetDateTime::parse(&order.expired_at, &Rfc3339).unwrap();
            let created = OffsetDateTime::parse(&order.created_at, &Rfc3339).unwrap();
            assert_eq!(expires - created, time::Duration::minutes(10));
            assert!(created >= before);
            assert!(order
                .idempotency_key
                .ends_with(&fixture.cart_id.to_string()));
        });
    }

    #[tokio::test]
    async fn cart_retry_returns_same_order_without_reparticipating_or_requiring_positive_cart() {
        let fixture = fixture(2, 0, 10).await;
        let request = body(&[fixture.cart_id], "same-request");
        let (_, first) = send(fixture.state.clone(), fixture.member_id, request.clone()).await;
        let (status, second) = send(fixture.state.clone(), fixture.member_id, request).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(first["data"][0]["orderId"], second["data"][0]["orderId"]);
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(2)
        );
        fixture
            .state
            .orders
            .transact(|orders| assert_eq!(orders.len(), 1));
    }

    #[tokio::test]
    async fn cart_order_rejects_unknown_member_missing_receiver_and_foreign_cart() {
        let fixture = fixture(1, 0, 10).await;
        let (status, response) = send(
            fixture.state.clone(),
            Uuid::new_v4(),
            body(&[fixture.cart_id], "request"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(response["message"], MEMBER_NOT_FOUND_MESSAGE);

        let mut invalid = body(&[fixture.cart_id], "request");
        invalid["receiverName"] = Value::Null;
        let (status, _) = send(fixture.state.clone(), fixture.member_id, invalid).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let other_member = Uuid::new_v4();
        fixture.state.auth.insert_test_member(other_member).await;
        let (status, response) = send(
            fixture.state,
            other_member,
            body(&[fixture.cart_id], "request"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(response["message"], NOT_CART_OWNER_MESSAGE);
    }

    #[tokio::test]
    async fn multi_cart_capacity_failure_rolls_back_prior_participation_and_keeps_carts() {
        let fixture = fixture(2, 0, 10).await;
        let first_product = Uuid::new_v4();
        let second_product = Uuid::new_v4();
        let second_purchase = Uuid::new_v4();
        let seller = Uuid::new_v4();
        let second_cart =
            fixture
                .state
                .carts
                .insert_test_cart(fixture.member_id, second_purchase, 2);
        let group_purchases = GroupPurchaseRepository::for_order_tests([
            (
                fixture.purchase_id,
                first_product,
                seller,
                8_000,
                0,
                10,
                "OPEN",
            ),
            (second_purchase, second_product, seller, 7_000, 1, 2, "OPEN"),
        ]);
        let products = Arc::new(LocalProductRepository::new([
            product(first_product, seller),
            product(second_product, seller),
        ]));
        let state = OrderState {
            group_purchases: group_purchases.clone(),
            products,
            ..fixture.state.clone()
        };

        let (status, response) = send(
            state.clone(),
            fixture.member_id,
            body(&[fixture.cart_id, second_cart], "batch"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["message"], PURCHASE_REACHED_MESSAGE);
        assert_eq!(
            group_purchases.order_test_quantity(fixture.purchase_id),
            Some(0)
        );
        assert_eq!(state.carts.test_quantity(fixture.cart_id), Some(2));
        assert_eq!(state.carts.test_quantity(second_cart), Some(2));
        state.orders.transact(|orders| assert!(orders.is_empty()));
    }
}
