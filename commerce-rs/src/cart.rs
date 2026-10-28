use crate::group_purchase::GroupPurchaseRepository;
use axum::{
    extract::{rejection::JsonRejection, rejection::QueryRejection, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const PURCHASE_NOT_FOUND_MESSAGE: &str = "공동구매를 찾을 수 없습니다.";
const PURCHASE_UNAVAILABLE_MESSAGE: &str = "유효하지 않은 공동구매입니다.";
const CART_NOT_FOUND_MESSAGE: &str = "장바구니에서 해당 공동구매를 찾을 수 없습니다.";
const NOT_CART_OWNER_MESSAGE: &str = "카트에 대한 권한이 없습니다.";
const CART_ADDED_MESSAGE: &str = "장바구니에 공동구매상품이 추가되었습니다.";
const CART_DELETED_MESSAGE: &str = "장바구니에서 공동구매상품이 제거되었습니다.";
const CART_UPDATED_MESSAGE: &str = "장바구니에 담은 공동구매상품의 개수가 변경되었습니다.";
const CART_LIST_MESSAGE: &str = "장바구니에 담긴 공동구매상품 리스트";
const CART_FLUSHED_MESSAGE: &str = "장바구니를 비웠습니다.";

#[derive(Clone, Default)]
pub(crate) struct CartRepository {
    carts: Arc<RwLock<Vec<Cart>>>,
}

#[derive(Clone)]
struct Cart {
    cart_id: Uuid,
    member_id: Uuid,
    group_purchase_id: Uuid,
    quantity: i32,
    created_at: String,
    updated_at: Option<String>,
}

#[derive(Clone)]
pub(crate) struct OrderCart {
    pub(crate) cart_id: Uuid,
    pub(crate) group_purchase_id: Uuid,
    pub(crate) quantity: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OrderCartError {
    NotFound,
    Forbidden,
    Empty,
}

#[derive(Clone)]
struct CartState {
    carts: CartRepository,
    group_purchases: GroupPurchaseRepository,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartAddRequest {
    group_purchase_id: Option<Uuid>,
    quantity: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartChangeRequest {
    cart_id: Option<Uuid>,
    quantity: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartDeleteRequest {
    cart_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct CartPageQuery {
    page: Option<usize>,
    size: Option<usize>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CartInfo {
    cart_id: Uuid,
    member_id: Uuid,
    group_purchase_id: Uuid,
    quantity: i32,
    created_at: String,
    updated_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CartPage {
    content: Vec<CartInfo>,
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

enum ChangeResult {
    Changed(Cart),
    NotFound,
    Forbidden,
}

impl From<Cart> for CartInfo {
    fn from(cart: Cart) -> Self {
        Self {
            cart_id: cart.cart_id,
            member_id: cart.member_id,
            group_purchase_id: cart.group_purchase_id,
            quantity: cart.quantity,
            created_at: cart.created_at,
            updated_at: cart.updated_at,
        }
    }
}

impl CartRepository {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn insert_test_cart(
        &self,
        member_id: Uuid,
        group_purchase_id: Uuid,
        quantity: i32,
    ) -> Uuid {
        self.add(member_id, group_purchase_id, quantity).cart_id
    }

    #[cfg(test)]
    pub(crate) fn test_quantity(&self, cart_id: Uuid) -> Option<i32> {
        self.carts
            .read()
            .unwrap()
            .iter()
            .find(|cart| cart.cart_id == cart_id)
            .map(|cart| cart.quantity)
    }

    fn add(&self, member_id: Uuid, group_purchase_id: Uuid, quantity: i32) -> Cart {
        let mut carts = self.carts.write().unwrap();
        if let Some(cart) = carts
            .iter_mut()
            .find(|cart| cart.member_id == member_id && cart.group_purchase_id == group_purchase_id)
        {
            cart.quantity = cart.quantity.wrapping_add(quantity);
            cart.updated_at = Some(now());
            return cart.clone();
        }
        let timestamp = now();
        let cart = Cart {
            cart_id: Uuid::new_v4(),
            member_id,
            group_purchase_id,
            quantity,
            created_at: timestamp.clone(),
            updated_at: Some(timestamp),
        };
        carts.push(cart.clone());
        cart
    }

    fn update(&self, cart_id: Uuid, member_id: Uuid, quantity: i32) -> ChangeResult {
        let mut carts = self.carts.write().unwrap();
        let Some(cart) = carts.iter_mut().find(|cart| cart.cart_id == cart_id) else {
            return ChangeResult::NotFound;
        };
        if cart.member_id != member_id {
            return ChangeResult::Forbidden;
        }
        cart.quantity = quantity;
        cart.updated_at = Some(now());
        ChangeResult::Changed(cart.clone())
    }

    fn soft_delete(&self, cart_id: Uuid, member_id: Uuid) -> ChangeResult {
        let mut carts = self.carts.write().unwrap();
        let Some(cart) = carts.iter_mut().find(|cart| cart.cart_id == cart_id) else {
            return ChangeResult::NotFound;
        };
        if cart.member_id != member_id {
            return ChangeResult::Forbidden;
        }
        cart.quantity = 0;
        cart.updated_at = Some(now());
        ChangeResult::Changed(cart.clone())
    }

    fn page(&self, member_id: Uuid, page: usize, size: usize) -> (Vec<Cart>, usize) {
        let carts = self.carts.read().unwrap();
        let matching = carts
            .iter()
            .filter(|cart| cart.member_id == member_id && cart.quantity > 0)
            .cloned()
            .collect::<Vec<_>>();
        let total = matching.len();
        let start = page.saturating_mul(size);
        let content = matching.into_iter().skip(start).take(size).collect();
        (content, total)
    }

    fn flush(&self, member_id: Uuid) {
        for cart in self
            .carts
            .write()
            .unwrap()
            .iter_mut()
            .filter(|cart| cart.member_id == member_id)
        {
            cart.quantity = 0;
        }
    }

    pub(crate) fn with_order_carts<T, E>(
        &self,
        member_id: Uuid,
        cart_ids: &[Uuid],
        create_orders: impl FnOnce(&[OrderCart]) -> Result<T, E>,
    ) -> Result<T, Result<OrderCartError, E>> {
        let mut carts = self.carts.write().unwrap();
        let selected = cart_ids
            .iter()
            .map(|cart_id| {
                carts
                    .iter()
                    .find(|cart| cart.cart_id == *cart_id)
                    .cloned()
                    .ok_or(OrderCartError::NotFound)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Ok)?;
        if selected.iter().any(|cart| cart.member_id != member_id) {
            return Err(Ok(OrderCartError::Forbidden));
        }
        if selected.iter().any(|cart| cart.quantity <= 0) {
            return Err(Ok(OrderCartError::Empty));
        }
        let order_carts = selected
            .iter()
            .map(|cart| OrderCart {
                cart_id: cart.cart_id,
                group_purchase_id: cart.group_purchase_id,
                quantity: cart.quantity,
            })
            .collect::<Vec<_>>();
        let result = create_orders(&order_carts).map_err(Err)?;
        for selected_cart in selected {
            let cart = carts
                .iter_mut()
                .find(|cart| cart.cart_id == selected_cart.cart_id)
                .expect("validated cart must remain present while the repository is locked");
            cart.quantity = 0;
            cart.updated_at = Some(now());
        }
        Ok(result)
    }
}

pub fn router(carts: CartRepository, group_purchases: GroupPurchaseRepository) -> Router {
    Router::new()
        .route(
            "/api/carts",
            get(list).post(add).patch(update).delete(delete),
        )
        .route("/api/carts/all", axum::routing::delete(flush))
        .with_state(CartState {
            carts,
            group_purchases,
        })
}

async fn add(
    State(state): State<CartState>,
    headers: HeaderMap,
    payload: Result<Json<CartAddRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Json(request)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let (Some(group_purchase_id), Some(quantity)) = (request.group_purchase_id, request.quantity)
    else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    if quantity < 1 {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    }
    match state.group_purchases.cart_availability(group_purchase_id) {
        None => return error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE),
        Some(false) => return error(StatusCode::BAD_REQUEST, PURCHASE_UNAVAILABLE_MESSAGE),
        Some(true) => {}
    }
    let cart = state.carts.add(member_id, group_purchase_id, quantity);
    api_response(
        StatusCode::CREATED,
        Some(CartInfo::from(cart)),
        CART_ADDED_MESSAGE,
    )
}

async fn update(
    State(state): State<CartState>,
    headers: HeaderMap,
    payload: Result<Json<CartChangeRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Json(request)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let (Some(cart_id), Some(quantity)) = (request.cart_id, request.quantity) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    if quantity < 1 {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    }
    match state.carts.update(cart_id, member_id, quantity) {
        ChangeResult::Changed(cart) => api_response(
            StatusCode::OK,
            Some(CartInfo::from(cart)),
            CART_UPDATED_MESSAGE,
        ),
        ChangeResult::NotFound => error(StatusCode::NOT_FOUND, CART_NOT_FOUND_MESSAGE),
        ChangeResult::Forbidden => error(StatusCode::FORBIDDEN, NOT_CART_OWNER_MESSAGE),
    }
}

async fn delete(
    State(state): State<CartState>,
    headers: HeaderMap,
    payload: Result<Json<CartDeleteRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Json(request)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some(cart_id) = request.cart_id else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    match state.carts.soft_delete(cart_id, member_id) {
        ChangeResult::Changed(_) => api_response::<()>(StatusCode::OK, None, CART_DELETED_MESSAGE),
        ChangeResult::NotFound => error(StatusCode::NOT_FOUND, CART_NOT_FOUND_MESSAGE),
        ChangeResult::Forbidden => error(StatusCode::FORBIDDEN, NOT_CART_OWNER_MESSAGE),
    }
}

async fn list(
    State(state): State<CartState>,
    headers: HeaderMap,
    query: Result<Query<CartPageQuery>, QueryRejection>,
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
    let (content, total_elements) = state.carts.page(member_id, page, size);
    let content = content.into_iter().map(CartInfo::from).collect::<Vec<_>>();
    let number_of_elements = content.len();
    let total_pages = total_elements.div_ceil(size);
    api_response(
        StatusCode::OK,
        Some(CartPage {
            content,
            total_pages,
            total_elements,
            first: page == 0,
            last: total_pages == 0 || page.saturating_add(1) >= total_pages,
            size,
            number_of_elements,
        }),
        CART_LIST_MESSAGE,
    )
}

async fn flush(State(state): State<CartState>, headers: HeaderMap) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    state.carts.flush(member_id);
    api_response::<()>(StatusCode::OK, None, CART_FLUSHED_MESSAGE)
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

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
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
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn send(
        state: CartState,
        method: &str,
        uri: &str,
        member_id: Option<Uuid>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(member_id) = member_id {
            request = request.header("x-member-id", member_id.to_string());
        }
        let body = match body {
            Some(body) => {
                request = request.header("content-type", "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        let app = Router::new()
            .route(
                "/api/carts",
                get(list).post(add).patch(update).delete(delete),
            )
            .route("/api/carts/all", axum::routing::delete(flush))
            .with_state(state);
        let response = app.oneshot(request.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn test_state(purchases: impl IntoIterator<Item = (Uuid, &'static str)>) -> CartState {
        CartState {
            carts: CartRepository::default(),
            group_purchases: GroupPurchaseRepository::for_cart_tests(purchases),
        }
    }

    async fn add_cart(
        state: CartState,
        member_id: Uuid,
        purchase_id: Uuid,
        quantity: i32,
    ) -> Value {
        let (status, body) = send(
            state,
            "POST",
            "/api/carts",
            Some(member_id),
            Some(json!({"groupPurchaseId": purchase_id, "quantity": quantity})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        body
    }

    #[tokio::test]
    async fn add_requires_an_open_purchase_and_accumulates_existing_cart() {
        let member_id = Uuid::new_v4();
        let open_id = Uuid::new_v4();
        let scheduled_id = Uuid::new_v4();
        let state = test_state([(open_id, "OPEN"), (scheduled_id, "SCHEDULED")]);
        let first = add_cart(state.clone(), member_id, open_id, 2).await;
        let cart_id = first["data"]["cartId"].as_str().unwrap().to_owned();
        assert_eq!(first["data"]["quantity"], 2);
        let second = add_cart(state.clone(), member_id, open_id, 3).await;
        assert_eq!(second["data"]["cartId"], cart_id);
        assert_eq!(second["data"]["quantity"], 5);

        let (status, body) = send(
            state.clone(),
            "POST",
            "/api/carts",
            Some(member_id),
            Some(json!({"groupPurchaseId": scheduled_id, "quantity": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], PURCHASE_UNAVAILABLE_MESSAGE);
        let (status, body) = send(
            state,
            "POST",
            "/api/carts",
            Some(member_id),
            Some(json!({"groupPurchaseId": Uuid::new_v4(), "quantity": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PURCHASE_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn update_and_delete_enforce_owner_and_soft_zero() {
        let owner_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let state = test_state([(purchase_id, "OPEN")]);
        let created = add_cart(state.clone(), owner_id, purchase_id, 2).await;
        let cart_id = created["data"]["cartId"].as_str().unwrap();
        let (status, body) = send(
            state.clone(),
            "PATCH",
            "/api/carts",
            Some(Uuid::new_v4()),
            Some(json!({"cartId": cart_id, "quantity": 4})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["message"], NOT_CART_OWNER_MESSAGE);
        let (status, body) = send(
            state.clone(),
            "PATCH",
            "/api/carts",
            Some(owner_id),
            Some(json!({"cartId": cart_id, "quantity": 4})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["quantity"], 4);
        let (status, body) = send(
            state.clone(),
            "DELETE",
            "/api/carts",
            Some(Uuid::new_v4()),
            Some(json!({"cartId": cart_id})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["message"], NOT_CART_OWNER_MESSAGE);
        let (status, body) = send(
            state.clone(),
            "DELETE",
            "/api/carts",
            Some(owner_id),
            Some(json!({"cartId": cart_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"], Value::Null);
        let (_, list) = send(state, "GET", "/api/carts", Some(owner_id), None).await;
        assert_eq!(list["data"]["totalElements"], 0);

        let scheduled_id = Uuid::new_v4();
        let scheduled_state = test_state([(scheduled_id, "SCHEDULED")]);
        let scheduled_cart = scheduled_state.carts.add(owner_id, scheduled_id, 1);
        let (status, body) = send(
            scheduled_state,
            "PATCH",
            "/api/carts",
            Some(owner_id),
            Some(json!({"cartId": scheduled_cart.cart_id, "quantity": 2})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["quantity"], 2);
    }

    #[tokio::test]
    async fn list_pages_positive_carts_and_flushes_only_the_member() {
        let member_id = Uuid::new_v4();
        let other_member_id = Uuid::new_v4();
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let state = test_state([(first_id, "OPEN"), (second_id, "OPEN")]);
        add_cart(state.clone(), member_id, first_id, 1).await;
        add_cart(state.clone(), member_id, second_id, 2).await;
        add_cart(state.clone(), other_member_id, first_id, 3).await;
        let (status, page) = send(
            state.clone(),
            "GET",
            "/api/carts?page=1&size=1",
            Some(member_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["data"]["totalElements"], 2);
        assert_eq!(page["data"]["totalPages"], 2);
        assert_eq!(page["data"]["numberOfElements"], 1);
        assert_eq!(page["data"]["first"], false);
        assert_eq!(page["data"]["last"], true);
        let (status, _) = send(
            state.clone(),
            "DELETE",
            "/api/carts/all",
            Some(member_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, member_page) =
            send(state.clone(), "GET", "/api/carts", Some(member_id), None).await;
        let (_, other_page) = send(state, "GET", "/api/carts", Some(other_member_id), None).await;
        assert_eq!(member_page["data"]["totalElements"], 0);
        assert_eq!(other_page["data"]["totalElements"], 1);
    }

    #[tokio::test]
    async fn validates_header_body_quantity_and_missing_cart() {
        let purchase_id = Uuid::new_v4();
        let state = test_state([(purchase_id, "OPEN")]);
        let (status, body) = send(
            state.clone(),
            "POST",
            "/api/carts",
            None,
            Some(json!({"groupPurchaseId": purchase_id, "quantity": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["message"], NO_LOGIN_INFO_MESSAGE);
        let (status, body) = send(
            state.clone(),
            "POST",
            "/api/carts",
            Some(Uuid::new_v4()),
            Some(json!({"groupPurchaseId": purchase_id, "quantity": 0})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_PARAMETER_MESSAGE);
        let (status, body) = send(
            state,
            "PATCH",
            "/api/carts",
            Some(Uuid::new_v4()),
            Some(json!({"cartId": Uuid::new_v4(), "quantity": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], CART_NOT_FOUND_MESSAGE);

        let invalid_query_state = CartState {
            carts: CartRepository::default(),
            group_purchases: GroupPurchaseRepository::for_cart_tests([(purchase_id, "OPEN")]),
        };
        let (status, body) = send(
            invalid_query_state,
            "GET",
            "/api/carts?page=-1",
            Some(Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_PARAMETER_MESSAGE);
    }
}
