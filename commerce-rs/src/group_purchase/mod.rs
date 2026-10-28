use crate::{ProductDetail, ProductRepository};

mod likes;
pub(crate) mod participate;
mod search;
use axum::{
    extract::{rejection::JsonRejection, rejection::QueryRejection, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const PURCHASE_NOT_FOUND_MESSAGE: &str = "공동구매를 찾을 수 없습니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const PRODUCT_OWNER_MESSAGE: &str = "본인이 등록한 상품이 아닙니다.";
const PURCHASE_OWNER_MESSAGE: &str = "본인이 등록한 공동구매만 삭제할 수 있습니다.";
const INVALID_QUANTITY_RANGE_MESSAGE: &str = "잘못된 수량입니다.";
const INVALID_DATE_RANGE_MESSAGE: &str = "잘못된 날짜 범위입니다.";
const INVALID_OPEN_UPDATE_MESSAGE: &str = "공동 구매가 OPEN 상태입니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const PURCHASE_CREATED_MESSAGE: &str = "공동 구매가 생성 되었습니다.";
const PURCHASE_UPDATED_MESSAGE: &str = "공동구매 정보가 수정되었습니다.";
const PURCHASE_DELETED_MESSAGE: &str = "공동구매가 삭제되었습니다";

#[derive(Clone)]
pub struct GroupPurchaseRepository {
    purchases: Arc<RwLock<Vec<GroupPurchase>>>,
    likes: Arc<RwLock<HashSet<(Uuid, Uuid)>>>,
    success_times: Arc<RwLock<HashMap<Uuid, String>>>,
}

#[derive(Clone)]
struct GroupPurchase {
    group_purchase_id: Uuid,
    min_quantity: i32,
    max_quantity: i32,
    title: String,
    description: Option<String>,
    discounted_price: i64,
    current_quantity: i32,
    like_count: i32,
    start_date: String,
    end_date: String,
    seller_id: Uuid,
    product_id: Uuid,
    image_url: Option<String>,
    status: String,
    created_at: String,
    updated_at: Option<String>,
}

struct GroupPurchaseWrite {
    min_quantity: i32,
    max_quantity: i32,
    title: String,
    description: Option<String>,
    discounted_price: i64,
    start_date: String,
    end_date: String,
    product_id: Uuid,
    image_url: Option<String>,
}

struct NewGroupPurchase {
    write: GroupPurchaseWrite,
    seller_id: Uuid,
}

enum UpdateGroupPurchaseResult {
    Updated(Box<GroupPurchase>),
    NotFound,
    Forbidden,
    InvalidOpenUpdate,
}

enum DeleteGroupPurchaseResult {
    Deleted,
    NotFound,
    Forbidden,
    InvalidStatus,
}

#[derive(Clone, Copy)]
enum SortField {
    GroupPurchaseId,
    MinQuantity,
    MaxQuantity,
    Title,
    DiscountedPrice,
    CurrentQuantity,
    StartDate,
    EndDate,
    Status,
    CreatedAt,
}

#[derive(Clone, Copy)]
struct PurchaseSort {
    field: SortField,
    descending: bool,
}

#[derive(Deserialize)]
struct ListQuery {
    page: Option<usize>,
    size: Option<usize>,
    sort: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupPurchaseWriteRequest {
    min_quantity: Option<i32>,
    max_quantity: Option<i32>,
    title: Option<String>,
    description: Option<String>,
    discounted_price: Option<i64>,
    start_date: Option<String>,
    end_date: Option<String>,
    product_id: Option<Uuid>,
    image_url: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupPurchaseDetail {
    group_purchase_id: Uuid,
    min_quantity: i32,
    max_quantity: i32,
    title: String,
    description: Option<String>,
    price: i64,
    discounted_price: i64,
    current_quantity: i32,
    start_date: String,
    end_date: String,
    seller_id: Uuid,
    product_id: Uuid,
    original_url: Option<String>,
    image_url: Option<String>,
    category: String,
    status: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupPurchaseThumbnail {
    group_purchase_id: Uuid,
    min_quantity: i32,
    max_quantity: i32,
    title: String,
    discounted_price: i64,
    original_price: i64,
    current_quantity: i32,
    start_date: String,
    end_date: String,
    category: String,
    status: String,
    image_url: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupPurchasePage {
    content: Vec<GroupPurchaseThumbnail>,
    total_pages: usize,
    total_elements: usize,
    first: bool,
    last: bool,
    size: usize,
    number_of_elements: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupPurchaseInfo {
    group_purchase_id: Uuid,
    min_quantity: i32,
    max_quantity: i32,
    discounted_price: i64,
    title: String,
    description: Option<String>,
    status: String,
    image_url: Option<String>,
    start_date: String,
    end_date: String,
    seller_id: Uuid,
    product_id: Uuid,
    created_at: String,
    updated_at: Option<String>,
}

impl From<GroupPurchase> for GroupPurchaseInfo {
    fn from(purchase: GroupPurchase) -> Self {
        Self {
            group_purchase_id: purchase.group_purchase_id,
            min_quantity: purchase.min_quantity,
            max_quantity: purchase.max_quantity,
            discounted_price: purchase.discounted_price,
            title: purchase.title,
            description: purchase.description,
            status: purchase.status,
            image_url: purchase.image_url,
            start_date: purchase.start_date,
            end_date: purchase.end_date,
            seller_id: purchase.seller_id,
            product_id: purchase.product_id,
            created_at: purchase.created_at,
            updated_at: purchase.updated_at,
        }
    }
}

#[derive(Clone)]
struct GroupPurchaseState {
    purchases: GroupPurchaseRepository,
    products: ProductRepository,
}

impl GroupPurchaseRepository {
    pub fn empty() -> Self {
        Self::new([])
    }

    pub(crate) fn cart_availability(&self, purchase_id: Uuid) -> Option<bool> {
        self.purchases
            .read()
            .unwrap()
            .iter()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .map(|purchase| purchase.status == "OPEN")
    }

    #[cfg(test)]
    pub(crate) fn order_current_quantity(&self, purchase_id: Uuid) -> Option<i32> {
        self.purchases
            .read()
            .unwrap()
            .iter()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .map(|purchase| purchase.current_quantity)
    }

    #[cfg(test)]
    pub(crate) fn for_cart_tests(
        purchases: impl IntoIterator<Item = (Uuid, &'static str)>,
    ) -> Self {
        Self::new(
            purchases
                .into_iter()
                .map(|(purchase_id, status)| GroupPurchase {
                    group_purchase_id: purchase_id,
                    min_quantity: 1,
                    max_quantity: 10,
                    title: "테스트 공동구매".to_owned(),
                    description: None,
                    discounted_price: 1_000,
                    current_quantity: 0,
                    like_count: 0,
                    start_date: "2026-01-01T00:00:00Z".to_owned(),
                    end_date: "2026-01-31T00:00:00Z".to_owned(),
                    seller_id: Uuid::new_v4(),
                    product_id: Uuid::new_v4(),
                    image_url: None,
                    status: status.to_owned(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                    updated_at: None,
                }),
        )
    }

    #[cfg(test)]
    pub(crate) fn for_order_tests(
        purchases: impl IntoIterator<Item = (Uuid, Uuid, Uuid, i64, i32, i32, &'static str)>,
    ) -> Self {
        Self::new(purchases.into_iter().map(
            |(
                purchase_id,
                product_id,
                seller_id,
                discounted_price,
                current_quantity,
                max_quantity,
                status,
            )| GroupPurchase {
                group_purchase_id: purchase_id,
                min_quantity: 1,
                max_quantity,
                title: "테스트 공동구매".to_owned(),
                description: None,
                discounted_price,
                current_quantity,
                like_count: 0,
                start_date: "2026-01-01T00:00:00Z".to_owned(),
                end_date: "2099-12-31T00:00:00Z".to_owned(),
                seller_id,
                product_id,
                image_url: None,
                status: status.to_owned(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                updated_at: None,
            },
        ))
    }

    #[cfg(test)]
    pub(crate) fn order_test_quantity(&self, purchase_id: Uuid) -> Option<i32> {
        self.find_by_id(purchase_id)
            .map(|purchase| purchase.current_quantity)
    }

    pub(crate) fn order_title(&self, purchase_id: Uuid) -> Option<String> {
        self.find_by_id(purchase_id).map(|purchase| purchase.title)
    }

    fn new(purchases: impl IntoIterator<Item = GroupPurchase>) -> Self {
        let purchases = purchases.into_iter().collect::<Vec<_>>();
        let success_times = purchases
            .iter()
            .filter_map(|purchase| {
                (purchase.status == "SUCCESS")
                    .then(|| {
                        purchase
                            .updated_at
                            .clone()
                            .map(|time| (purchase.group_purchase_id, time))
                    })
                    .flatten()
            })
            .collect();
        Self {
            purchases: Arc::new(RwLock::new(purchases)),
            likes: Arc::new(RwLock::new(HashSet::new())),
            success_times: Arc::new(RwLock::new(success_times)),
        }
    }

    fn find_by_id(&self, purchase_id: Uuid) -> Option<GroupPurchase> {
        self.purchases
            .read()
            .unwrap()
            .iter()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .cloned()
    }

    fn find_all(
        &self,
        seller_id: Option<Uuid>,
        page: usize,
        size: usize,
        sort: Option<PurchaseSort>,
    ) -> (Vec<GroupPurchase>, usize) {
        let mut purchases = self
            .purchases
            .read()
            .unwrap()
            .iter()
            .filter(|purchase| seller_id.is_none_or(|id| purchase.seller_id == id))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(sort) = sort {
            purchases.sort_by(|left, right| purchase_order(left, right, sort));
        }
        let total = purchases.len();
        let content = purchases
            .into_iter()
            .skip(page.saturating_mul(size))
            .take(size)
            .collect();
        (content, total)
    }

    fn create(&self, new_purchase: NewGroupPurchase) -> GroupPurchase {
        let timestamp = now();
        let purchase = GroupPurchase {
            group_purchase_id: Uuid::new_v4(),
            min_quantity: new_purchase.write.min_quantity,
            max_quantity: new_purchase.write.max_quantity,
            title: new_purchase.write.title,
            description: new_purchase.write.description,
            discounted_price: new_purchase.write.discounted_price,
            current_quantity: 0,
            like_count: 0,
            start_date: new_purchase.write.start_date,
            end_date: new_purchase.write.end_date,
            seller_id: new_purchase.seller_id,
            product_id: new_purchase.write.product_id,
            image_url: new_purchase.write.image_url,
            status: "SCHEDULED".to_owned(),
            created_at: timestamp.clone(),
            updated_at: Some(timestamp),
        };
        self.purchases.write().unwrap().push(purchase.clone());
        purchase
    }

    fn update(
        &self,
        purchase_id: Uuid,
        seller_id: Uuid,
        write: GroupPurchaseWrite,
    ) -> UpdateGroupPurchaseResult {
        let mut purchases = self.purchases.write().unwrap();
        let Some(purchase) = purchases
            .iter_mut()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
        else {
            return UpdateGroupPurchaseResult::NotFound;
        };
        if purchase.seller_id != seller_id {
            return UpdateGroupPurchaseResult::Forbidden;
        }
        if open_fields_changed(purchase, &write) {
            return UpdateGroupPurchaseResult::InvalidOpenUpdate;
        }

        purchase.min_quantity = write.min_quantity;
        purchase.max_quantity = write.max_quantity;
        purchase.title = write.title;
        purchase.description = write.description;
        purchase.discounted_price = write.discounted_price;
        purchase.start_date = write.start_date;
        purchase.end_date = write.end_date;
        purchase.product_id = write.product_id;
        purchase.image_url = write.image_url;
        purchase.updated_at = Some(now());
        UpdateGroupPurchaseResult::Updated(Box::new(purchase.clone()))
    }

    fn delete(&self, purchase_id: Uuid, seller_id: Uuid) -> DeleteGroupPurchaseResult {
        let mut purchases = self.purchases.write().unwrap();
        let Some(index) = purchases
            .iter()
            .position(|purchase| purchase.group_purchase_id == purchase_id)
        else {
            return DeleteGroupPurchaseResult::NotFound;
        };
        if purchases[index].status != "SCHEDULED" {
            return DeleteGroupPurchaseResult::InvalidStatus;
        }
        if purchases[index].seller_id != seller_id {
            return DeleteGroupPurchaseResult::Forbidden;
        }
        self.likes
            .write()
            .unwrap()
            .retain(|(_, liked_purchase_id)| *liked_purchase_id != purchase_id);
        purchases.remove(index);
        DeleteGroupPurchaseResult::Deleted
    }
}

pub fn router(purchases: GroupPurchaseRepository, products: ProductRepository) -> Router {
    Router::new()
        .route("/api/purchases", get(public_list).post(create))
        .route("/api/purchases/seller/{seller_id}", get(seller_list))
        .route(
            "/api/purchases/{purchase_id}",
            get(detail).patch(update).delete(delete_purchase),
        )
        .route(
            "/api/purchases/{purchase_id}/likes",
            post(likes::like).delete(likes::unlike),
        )
        .merge(search::router())
        .with_state(GroupPurchaseState {
            purchases,
            products,
        })
}

async fn create(
    State(state): State<GroupPurchaseState>,
    headers: HeaderMap,
    payload: Result<Json<GroupPurchaseWriteRequest>, JsonRejection>,
) -> Response {
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return error(status, message),
    };
    let Json(request) = match payload {
        Ok(request) => request,
        Err(_) => return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
    };
    let write = match validate_write_request(request) {
        Ok(write) => write,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let product = match state.products.find_by_id(write.product_id).await {
        Ok(Some(product)) => product,
        Ok(None) => return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE),
        Err(error_value) => {
            eprintln!("product database error: {error_value}");
            return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE);
        }
    };
    if product.seller_id != seller_id {
        return error(StatusCode::FORBIDDEN, PRODUCT_OWNER_MESSAGE);
    }
    if write.min_quantity > write.max_quantity {
        return error(StatusCode::BAD_REQUEST, INVALID_QUANTITY_RANGE_MESSAGE);
    }
    if parse_date(&write.start_date) > parse_date(&write.end_date) {
        return error(StatusCode::BAD_REQUEST, INVALID_DATE_RANGE_MESSAGE);
    }

    let purchase = state
        .purchases
        .create(NewGroupPurchase { write, seller_id });
    api_response(
        StatusCode::CREATED,
        Some(GroupPurchaseInfo::from(purchase)),
        PURCHASE_CREATED_MESSAGE,
    )
}

async fn update(
    State(state): State<GroupPurchaseState>,
    Path(purchase_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<GroupPurchaseWriteRequest>, JsonRejection>,
) -> Response {
    let Ok(purchase_id) = Uuid::parse_str(&purchase_id) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return error(status, message),
    };
    let Json(request) = match payload {
        Ok(request) => request,
        Err(_) => return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
    };
    let write = match validate_write_request(request) {
        Ok(write) => write,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };

    let Some(existing) = state.purchases.find_by_id(purchase_id) else {
        return error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE);
    };
    if existing.seller_id != seller_id {
        return error(StatusCode::FORBIDDEN, PURCHASE_OWNER_MESSAGE);
    }
    if open_fields_changed(&existing, &write) {
        return error(StatusCode::BAD_REQUEST, INVALID_OPEN_UPDATE_MESSAGE);
    }
    match state.products.find_by_id(write.product_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE),
        Err(error_value) => {
            eprintln!("product database error: {error_value}");
            return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE);
        }
    }

    match state.purchases.update(purchase_id, seller_id, write) {
        UpdateGroupPurchaseResult::Updated(purchase) => api_response(
            StatusCode::OK,
            Some(GroupPurchaseInfo::from(*purchase)),
            PURCHASE_UPDATED_MESSAGE,
        ),
        UpdateGroupPurchaseResult::NotFound => {
            error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE)
        }
        UpdateGroupPurchaseResult::Forbidden => {
            error(StatusCode::FORBIDDEN, PURCHASE_OWNER_MESSAGE)
        }
        UpdateGroupPurchaseResult::InvalidOpenUpdate => {
            error(StatusCode::BAD_REQUEST, INVALID_OPEN_UPDATE_MESSAGE)
        }
    }
}

async fn delete_purchase(
    State(state): State<GroupPurchaseState>,
    Path(purchase_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(purchase_id) = Uuid::parse_str(&purchase_id) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return error(status, message),
    };

    match state.purchases.delete(purchase_id, seller_id) {
        DeleteGroupPurchaseResult::Deleted => {
            api_response::<()>(StatusCode::OK, None, PURCHASE_DELETED_MESSAGE)
        }
        DeleteGroupPurchaseResult::NotFound => {
            error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE)
        }
        DeleteGroupPurchaseResult::Forbidden => {
            error(StatusCode::FORBIDDEN, PURCHASE_OWNER_MESSAGE)
        }
        DeleteGroupPurchaseResult::InvalidStatus => {
            error(StatusCode::BAD_REQUEST, INVALID_OPEN_UPDATE_MESSAGE)
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

fn validate_write_request(
    request: GroupPurchaseWriteRequest,
) -> Result<GroupPurchaseWrite, &'static str> {
    let min_quantity = request.min_quantity.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let max_quantity = request.max_quantity.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let title = request.title.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let discounted_price = request.discounted_price.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let start_date = request.start_date.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let end_date = request.end_date.ok_or(INVALID_PARAMETER_MESSAGE)?;
    let product_id = request.product_id.ok_or(INVALID_PARAMETER_MESSAGE)?;

    if min_quantity < 1
        || max_quantity < 1
        || title.trim().is_empty()
        || title.chars().count() > 100
        || discounted_price < 0
    {
        return Err(INVALID_PARAMETER_MESSAGE);
    }
    let start =
        OffsetDateTime::parse(&start_date, &Rfc3339).map_err(|_| INVALID_PARAMETER_MESSAGE)?;
    OffsetDateTime::parse(&end_date, &Rfc3339).map_err(|_| INVALID_PARAMETER_MESSAGE)?;
    if start < OffsetDateTime::now_utc() {
        return Err(INVALID_PARAMETER_MESSAGE);
    }

    Ok(GroupPurchaseWrite {
        min_quantity,
        max_quantity,
        title,
        description: request.description,
        discounted_price,
        start_date,
        end_date,
        product_id,
        image_url: request.image_url,
    })
}

fn open_fields_changed(purchase: &GroupPurchase, write: &GroupPurchaseWrite) -> bool {
    purchase.status == "OPEN"
        && (purchase.max_quantity != write.max_quantity
            || purchase.discounted_price != write.discounted_price
            || purchase.product_id != write.product_id)
}

fn parse_date(value: &str) -> OffsetDateTime {
    OffsetDateTime::parse(value, &Rfc3339).unwrap()
}

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

async fn detail(
    State(state): State<GroupPurchaseState>,
    Path(purchase_id): Path<String>,
) -> Response {
    let Ok(purchase_id) = Uuid::parse_str(&purchase_id) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some(purchase) = state.purchases.find_by_id(purchase_id) else {
        return error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE);
    };
    let product = match state.products.find_by_id(purchase.product_id).await {
        Ok(Some(product)) => product,
        Ok(None) => return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE),
        Err(error_value) => {
            eprintln!("product database error: {error_value}");
            return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE);
        }
    };
    api_response(
        StatusCode::OK,
        Some(detail_info(purchase, product)),
        "공동구매가 상세 조회되었습니다.",
    )
}

async fn public_list(
    State(state): State<GroupPurchaseState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    list(state, query, None, "공동구매 목록 조회되었습니다.").await
}

async fn seller_list(
    State(state): State<GroupPurchaseState>,
    Path(seller_id): Path<String>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    let Ok(seller_id) = Uuid::parse_str(&seller_id) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    list(
        state,
        query,
        Some(seller_id),
        "공동구매 판매자별 목록 조회되었습니다",
    )
    .await
}

async fn list(
    state: GroupPurchaseState,
    query: Result<Query<ListQuery>, QueryRejection>,
    seller_id: Option<Uuid>,
    message: &'static str,
) -> Response {
    let Ok(Query(query)) = query else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let page = query.page.unwrap_or(0);
    let size = query.size.unwrap_or(10);
    if size == 0 {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    }
    let (purchases, total_elements) = state.purchases.find_all(
        seller_id,
        page,
        size,
        query.sort.as_deref().and_then(parse_sort),
    );
    let mut content = Vec::with_capacity(purchases.len());
    for purchase in purchases {
        let product = match state.products.find_by_id(purchase.product_id).await {
            Ok(Some(product)) => product,
            Ok(None) => return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE),
            Err(error_value) => {
                eprintln!("product database error: {error_value}");
                return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE);
            }
        };
        content.push(thumbnail_info(purchase, product));
    }
    let total_pages = total_elements.div_ceil(size);
    let number_of_elements = content.len();
    api_response(
        StatusCode::OK,
        Some(GroupPurchasePage {
            content,
            total_pages,
            total_elements,
            first: page == 0,
            last: page.saturating_add(1) >= total_pages,
            size,
            number_of_elements,
        }),
        message,
    )
}

fn parse_sort(value: &str) -> Option<PurchaseSort> {
    let mut parts = value.split(',');
    let field = match parts.next()? {
        "groupPurchaseId" => SortField::GroupPurchaseId,
        "minQuantity" => SortField::MinQuantity,
        "maxQuantity" => SortField::MaxQuantity,
        "title" => SortField::Title,
        "discountedPrice" => SortField::DiscountedPrice,
        "currentQuantity" => SortField::CurrentQuantity,
        "startDate" => SortField::StartDate,
        "endDate" => SortField::EndDate,
        "status" => SortField::Status,
        "createdAt" => SortField::CreatedAt,
        _ => return None,
    };
    Some(PurchaseSort {
        field,
        descending: parts
            .next()
            .is_some_and(|direction| direction.eq_ignore_ascii_case("desc")),
    })
}

fn purchase_order(left: &GroupPurchase, right: &GroupPurchase, sort: PurchaseSort) -> Ordering {
    let ordering = match sort.field {
        SortField::GroupPurchaseId => left.group_purchase_id.cmp(&right.group_purchase_id),
        SortField::MinQuantity => left.min_quantity.cmp(&right.min_quantity),
        SortField::MaxQuantity => left.max_quantity.cmp(&right.max_quantity),
        SortField::Title => left.title.cmp(&right.title),
        SortField::DiscountedPrice => left.discounted_price.cmp(&right.discounted_price),
        SortField::CurrentQuantity => left.current_quantity.cmp(&right.current_quantity),
        SortField::StartDate => left.start_date.cmp(&right.start_date),
        SortField::EndDate => left.end_date.cmp(&right.end_date),
        SortField::Status => left.status.cmp(&right.status),
        SortField::CreatedAt => left.created_at.cmp(&right.created_at),
    };
    if sort.descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn detail_info(purchase: GroupPurchase, product: ProductDetail) -> GroupPurchaseDetail {
    GroupPurchaseDetail {
        group_purchase_id: purchase.group_purchase_id,
        min_quantity: purchase.min_quantity,
        max_quantity: purchase.max_quantity,
        title: purchase.title,
        description: purchase.description,
        price: product.price,
        discounted_price: purchase.discounted_price,
        current_quantity: purchase.current_quantity,
        start_date: purchase.start_date,
        end_date: purchase.end_date,
        seller_id: purchase.seller_id,
        product_id: purchase.product_id,
        original_url: product.original_link,
        image_url: purchase.image_url,
        category: product.category,
        status: purchase.status,
        created_at: purchase.created_at,
    }
}

fn thumbnail_info(purchase: GroupPurchase, product: ProductDetail) -> GroupPurchaseThumbnail {
    GroupPurchaseThumbnail {
        group_purchase_id: purchase.group_purchase_id,
        min_quantity: purchase.min_quantity,
        max_quantity: purchase.max_quantity,
        title: purchase.title,
        discounted_price: purchase.discounted_price,
        original_price: product.price,
        current_quantity: purchase.current_quantity,
        start_date: purchase.start_date,
        end_date: purchase.end_date,
        category: product.category,
        status: purchase.status,
        image_url: purchase.image_url,
        created_at: purchase.created_at,
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

fn error(status: StatusCode, message: &'static str) -> Response {
    api_response::<()>(status, None, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalProductRepository;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    fn product(product_id: Uuid, seller_id: Uuid) -> ProductDetail {
        ProductDetail {
            product_id,
            name: "사과".to_owned(),
            price: 10_000,
            category: "FOOD".to_owned(),
            description: "상품 설명".to_owned(),
            stock: 10,
            original_link: Some("https://example.com/apple".to_owned()),
            image_url: None,
            seller_id,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    fn purchase(
        purchase_id: Uuid,
        product_id: Uuid,
        seller_id: Uuid,
        status: &str,
        created_at: &str,
    ) -> GroupPurchase {
        GroupPurchase {
            group_purchase_id: purchase_id,
            min_quantity: 2,
            max_quantity: 20,
            title: format!("{status} 사과 공동구매"),
            description: Some("공동구매 설명".to_owned()),
            discounted_price: 8_000,
            current_quantity: 3,
            like_count: 0,
            start_date: "2026-01-01T00:00:00Z".to_owned(),
            end_date: "2026-01-31T00:00:00Z".to_owned(),
            seller_id,
            product_id,
            image_url: Some("https://example.com/purchase.png".to_owned()),
            status: status.to_owned(),
            created_at: created_at.to_owned(),
            updated_at: None,
        }
    }

    async fn send(
        purchases: Vec<GroupPurchase>,
        products: Vec<ProductDetail>,
        uri: &str,
    ) -> (StatusCode, Value) {
        let app = router(
            GroupPurchaseRepository::new(purchases),
            LocalProductRepository::new(products),
        );
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    async fn write(
        purchases: GroupPurchaseRepository,
        products: Arc<LocalProductRepository>,
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
        let response = router(purchases, (*products).clone())
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn write_body(product_id: Uuid) -> Value {
        serde_json::json!({
            "minQuantity": 2,
            "maxQuantity": 20,
            "title": "사과 공동구매",
            "description": "공동구매 설명",
            "discountedPrice": 8_000,
            "startDate": "2099-01-01T00:00:00Z",
            "endDate": "2099-01-31T00:00:00Z",
            "productId": product_id,
            "imageUrl": "https://example.com/purchase.png"
        })
    }

    #[tokio::test]
    async fn creates_a_scheduled_purchase_for_the_product_owner() {
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::empty();
        let products = Arc::new(LocalProductRepository::new([product(
            product_id, seller_id,
        )]));

        let (status, body) = write(
            purchases.clone(),
            products,
            "POST",
            "/api/purchases",
            Some(seller_id),
            Some(write_body(product_id)),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["message"], PURCHASE_CREATED_MESSAGE);
        assert_eq!(body["data"]["status"], "SCHEDULED");
        assert_eq!(body["data"]["sellerId"], seller_id.to_string());
        assert_eq!(body["data"]["productId"], product_id.to_string());
        assert_eq!(purchases.find_all(None, 0, 10, None).1, 1);
    }

    #[tokio::test]
    async fn create_validates_header_body_ranges_and_product_ownership() {
        let seller_id = Uuid::new_v4();
        let other_seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let products = Arc::new(LocalProductRepository::new([product(
            product_id,
            other_seller_id,
        )]));

        let (missing_header, missing_header_body) = write(
            GroupPurchaseRepository::empty(),
            products.clone(),
            "POST",
            "/api/purchases",
            None,
            Some(write_body(product_id)),
        )
        .await;
        assert_eq!(missing_header, StatusCode::UNAUTHORIZED);
        assert_eq!(missing_header_body["message"], NO_LOGIN_INFO_MESSAGE);

        let (forbidden, forbidden_body) = write(
            GroupPurchaseRepository::empty(),
            products.clone(),
            "POST",
            "/api/purchases",
            Some(seller_id),
            Some(write_body(product_id)),
        )
        .await;
        assert_eq!(forbidden, StatusCode::FORBIDDEN);
        assert_eq!(forbidden_body["message"], PRODUCT_OWNER_MESSAGE);

        let owned_product_id = Uuid::new_v4();
        let owned_products = Arc::new(LocalProductRepository::new([product(
            owned_product_id,
            seller_id,
        )]));
        let mut invalid_field = write_body(owned_product_id);
        invalid_field["minQuantity"] = serde_json::json!(0);
        let (invalid, invalid_body) = write(
            GroupPurchaseRepository::empty(),
            owned_products.clone(),
            "POST",
            "/api/purchases",
            Some(seller_id),
            Some(invalid_field),
        )
        .await;
        assert_eq!(invalid, StatusCode::BAD_REQUEST);
        assert_eq!(invalid_body["message"], INVALID_PARAMETER_MESSAGE);

        let mut invalid_range = write_body(owned_product_id);
        invalid_range["minQuantity"] = serde_json::json!(21);
        let (invalid, invalid_body) = write(
            GroupPurchaseRepository::empty(),
            owned_products.clone(),
            "POST",
            "/api/purchases",
            Some(seller_id),
            Some(invalid_range),
        )
        .await;
        assert_eq!(invalid, StatusCode::BAD_REQUEST);
        assert_eq!(invalid_body["message"], INVALID_QUANTITY_RANGE_MESSAGE);

        let mut invalid_dates = write_body(owned_product_id);
        invalid_dates["startDate"] = serde_json::json!("2099-02-01T00:00:00Z");
        let (invalid, invalid_body) = write(
            GroupPurchaseRepository::empty(),
            owned_products,
            "POST",
            "/api/purchases",
            Some(seller_id),
            Some(invalid_dates),
        )
        .await;
        assert_eq!(invalid, StatusCode::BAD_REQUEST);
        assert_eq!(invalid_body["message"], INVALID_DATE_RANGE_MESSAGE);
    }

    #[tokio::test]
    async fn create_rejects_a_missing_product_reference() {
        let product_id = Uuid::new_v4();
        let (status, body) = write(
            GroupPurchaseRepository::empty(),
            Arc::new(LocalProductRepository::new([])),
            "POST",
            "/api/purchases",
            Some(Uuid::new_v4()),
            Some(write_body(product_id)),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn update_enforces_owner_open_fields_and_product_reference() {
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([purchase(
            purchase_id,
            product_id,
            seller_id,
            "OPEN",
            "2026-01-01T00:00:00Z",
        )]);
        let products = Arc::new(LocalProductRepository::new([product(
            product_id, seller_id,
        )]));
        let mut allowed = write_body(product_id);
        allowed["title"] = serde_json::json!("수정된 제목");
        allowed["minQuantity"] = serde_json::json!(3);

        let (status, body) = write(
            purchases.clone(),
            products.clone(),
            "PATCH",
            &format!("/api/purchases/{purchase_id}"),
            Some(seller_id),
            Some(allowed),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["title"], "수정된 제목");
        assert_eq!(body["data"]["status"], "OPEN");

        let mut forbidden_open_change = write_body(product_id);
        forbidden_open_change["maxQuantity"] = serde_json::json!(21);
        let (status, body) = write(
            purchases.clone(),
            products.clone(),
            "PATCH",
            &format!("/api/purchases/{purchase_id}"),
            Some(seller_id),
            Some(forbidden_open_change),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_OPEN_UPDATE_MESSAGE);

        let (status, body) = write(
            purchases,
            products,
            "PATCH",
            &format!("/api/purchases/{purchase_id}"),
            Some(Uuid::new_v4()),
            Some(write_body(product_id)),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["message"], PURCHASE_OWNER_MESSAGE);
    }

    #[tokio::test]
    async fn scheduled_update_requires_an_existing_product_but_not_its_owner() {
        let seller_id = Uuid::new_v4();
        let other_seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let other_product_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([purchase(
            purchase_id,
            product_id,
            seller_id,
            "SCHEDULED",
            "2026-01-01T00:00:00Z",
        )]);
        let products = Arc::new(LocalProductRepository::new([
            product(product_id, seller_id),
            product(other_product_id, other_seller_id),
        ]));

        let (status, body) = write(
            purchases.clone(),
            products.clone(),
            "PATCH",
            &format!("/api/purchases/{purchase_id}"),
            Some(seller_id),
            Some(write_body(other_product_id)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["productId"], other_product_id.to_string());

        let (status, body) = write(
            purchases,
            products,
            "PATCH",
            &format!("/api/purchases/{purchase_id}"),
            Some(seller_id),
            Some(write_body(Uuid::new_v4())),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn delete_only_removes_a_scheduled_purchase_owned_by_the_seller() {
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let scheduled_id = Uuid::new_v4();
        let open_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(
                scheduled_id,
                product_id,
                seller_id,
                "SCHEDULED",
                "2026-01-01T00:00:00Z",
            ),
            purchase(
                open_id,
                product_id,
                seller_id,
                "OPEN",
                "2026-01-02T00:00:00Z",
            ),
        ]);
        let products = Arc::new(LocalProductRepository::new([product(
            product_id, seller_id,
        )]));

        let (status, body) = write(
            purchases.clone(),
            products.clone(),
            "DELETE",
            &format!("/api/purchases/{open_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_OPEN_UPDATE_MESSAGE);

        let (status, body) = write(
            purchases.clone(),
            products.clone(),
            "DELETE",
            &format!("/api/purchases/{scheduled_id}"),
            Some(Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["message"], PURCHASE_OWNER_MESSAGE);

        let (status, body) = write(
            purchases.clone(),
            products,
            "DELETE",
            &format!("/api/purchases/{scheduled_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"], PURCHASE_DELETED_MESSAGE);
        assert!(purchases.find_by_id(scheduled_id).is_none());
    }

    #[tokio::test]
    async fn update_and_delete_return_not_found_for_a_missing_purchase() {
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let missing_id = Uuid::new_v4();
        let products = Arc::new(LocalProductRepository::new([product(
            product_id, seller_id,
        )]));

        let (update_status, update_body) = write(
            GroupPurchaseRepository::empty(),
            products.clone(),
            "PATCH",
            &format!("/api/purchases/{missing_id}"),
            Some(seller_id),
            Some(write_body(product_id)),
        )
        .await;
        assert_eq!(update_status, StatusCode::NOT_FOUND);
        assert_eq!(update_body["message"], PURCHASE_NOT_FOUND_MESSAGE);

        let (delete_status, delete_body) = write(
            GroupPurchaseRepository::empty(),
            products,
            "DELETE",
            &format!("/api/purchases/{missing_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(delete_status, StatusCode::NOT_FOUND);
        assert_eq!(delete_body["message"], PURCHASE_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn detail_joins_product_and_preserves_all_fields() {
        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();

        let (status, body) = send(
            vec![purchase(
                purchase_id,
                product_id,
                seller_id,
                "OPEN",
                "2026-01-02T00:00:00Z",
            )],
            vec![product(product_id, seller_id)],
            &format!("/api/purchases/{purchase_id}"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"], "공동구매가 상세 조회되었습니다.");
        assert_eq!(body["data"]["price"], 10_000);
        assert_eq!(body["data"]["discountedPrice"], 8_000);
        assert_eq!(body["data"]["originalUrl"], "https://example.com/apple");
        assert_eq!(body["data"]["category"], "FOOD");
        assert_eq!(body["data"]["status"], "OPEN");
    }

    #[tokio::test]
    async fn detail_distinguishes_missing_purchase_and_product() {
        let seller_id = Uuid::new_v4();
        let missing_purchase_id = Uuid::new_v4();
        let (status, body) = send(
            vec![],
            vec![],
            &format!("/api/purchases/{missing_purchase_id}"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PURCHASE_NOT_FOUND_MESSAGE);

        let purchase_id = Uuid::new_v4();
        let (status, body) = send(
            vec![purchase(
                purchase_id,
                Uuid::new_v4(),
                seller_id,
                "SCHEDULED",
                "2026-01-01T00:00:00Z",
            )],
            vec![],
            &format!("/api/purchases/{purchase_id}"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn public_list_includes_all_statuses_and_applies_paging_and_product_join() {
        let seller_id = Uuid::new_v4();
        let statuses = ["SCHEDULED", "OPEN", "SUCCESS", "FAILED"];
        let product_ids = statuses.map(|_| Uuid::new_v4());
        let purchases = statuses
            .iter()
            .enumerate()
            .map(|(index, status)| {
                purchase(
                    Uuid::new_v4(),
                    product_ids[index],
                    seller_id,
                    status,
                    &format!("2026-01-0{}T00:00:00Z", index + 1),
                )
            })
            .collect();
        let products = product_ids
            .into_iter()
            .map(|id| product(id, seller_id))
            .collect();

        let (status, body) = send(
            purchases,
            products,
            "/api/purchases?page=1&size=2&sort=createdAt,asc",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["totalElements"], 4);
        assert_eq!(body["data"]["totalPages"], 2);
        assert_eq!(body["data"]["content"][0]["status"], "SUCCESS");
        assert_eq!(body["data"]["content"][1]["status"], "FAILED");
        assert_eq!(body["data"]["content"][0]["originalPrice"], 10_000);
    }

    #[tokio::test]
    async fn seller_list_filters_without_auth_and_rejects_invalid_inputs() {
        let seller_id = Uuid::new_v4();
        let other_seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let other_product_id = Uuid::new_v4();
        let purchases = vec![
            purchase(
                Uuid::new_v4(),
                product_id,
                seller_id,
                "OPEN",
                "2026-01-01T00:00:00Z",
            ),
            purchase(
                Uuid::new_v4(),
                other_product_id,
                other_seller_id,
                "FAILED",
                "2026-01-02T00:00:00Z",
            ),
        ];
        let products = vec![
            product(product_id, seller_id),
            product(other_product_id, other_seller_id),
        ];

        let (status, body) = send(
            purchases.clone(),
            products.clone(),
            &format!("/api/purchases/seller/{seller_id}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"], "공동구매 판매자별 목록 조회되었습니다");
        assert_eq!(body["data"]["totalElements"], 1);

        let (status, body) = send(
            purchases.clone(),
            products.clone(),
            "/api/purchases/seller/nope",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_PARAMETER_MESSAGE);

        let (status, body) = send(purchases, products, "/api/purchases?size=0").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_PARAMETER_MESSAGE);
    }

    #[tokio::test]
    async fn list_fails_when_a_product_reference_is_missing() {
        let seller_id = Uuid::new_v4();
        let (status, body) = send(
            vec![purchase(
                Uuid::new_v4(),
                Uuid::new_v4(),
                seller_id,
                "OPEN",
                "2026-01-01T00:00:00Z",
            )],
            vec![],
            "/api/purchases",
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }
}
