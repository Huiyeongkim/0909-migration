use crate::{ProductDetail, ProductRepository};
use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const PURCHASE_NOT_FOUND_MESSAGE: &str = "공동구매를 찾을 수 없습니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";

#[derive(Clone)]
pub struct GroupPurchaseRepository {
    purchases: Arc<RwLock<Vec<GroupPurchase>>>,
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
    start_date: String,
    end_date: String,
    seller_id: Uuid,
    product_id: Uuid,
    image_url: Option<String>,
    status: String,
    created_at: String,
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

#[derive(Clone)]
struct GroupPurchaseState {
    purchases: GroupPurchaseRepository,
    products: Arc<dyn ProductRepository>,
}

impl GroupPurchaseRepository {
    pub fn empty() -> Self {
        Self::new([])
    }

    fn new(purchases: impl IntoIterator<Item = GroupPurchase>) -> Self {
        Self {
            purchases: Arc::new(RwLock::new(purchases.into_iter().collect())),
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
}

pub fn router(purchases: GroupPurchaseRepository, products: Arc<dyn ProductRepository>) -> Router {
    Router::new()
        .route("/api/purchases", get(public_list))
        .route("/api/purchases/seller/{seller_id}", get(seller_list))
        .route("/api/purchases/{purchase_id}", get(detail))
        .with_state(GroupPurchaseState {
            purchases,
            products,
        })
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
    let Some(product) = state.products.find_by_id(purchase.product_id) else {
        return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE);
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
        let Some(product) = state.products.find_by_id(purchase.product_id) else {
            return error(StatusCode::NOT_FOUND, PRODUCT_NOT_FOUND_MESSAGE);
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
            start_date: "2026-01-01T00:00:00Z".to_owned(),
            end_date: "2026-01-31T00:00:00Z".to_owned(),
            seller_id,
            product_id,
            image_url: Some("https://example.com/purchase.png".to_owned()),
            status: status.to_owned(),
            created_at: created_at.to_owned(),
        }
    }

    async fn send(
        purchases: Vec<GroupPurchase>,
        products: Vec<ProductDetail>,
        uri: &str,
    ) -> (StatusCode, Value) {
        let app = router(
            GroupPurchaseRepository::new(purchases),
            Arc::new(LocalProductRepository::new(products)),
        );
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
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
