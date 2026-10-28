use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

const PRODUCT_FOUND_MESSAGE: &str = "상품 조회가 완료되었습니다.";
const PRODUCT_LIST_FOUND_MESSAGE: &str = "내가 등록한 상품 목록 조회가 완료되었습니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";
const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductDetail {
    product_id: Uuid,
    name: String,
    price: i64,
    category: String,
    description: String,
    stock: i32,
    original_link: Option<String>,
    image_url: Option<String>,
    seller_id: Uuid,
    created_at: String,
    updated_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductListItem {
    product_id: Uuid,
    name: String,
    price: i64,
    image_url: Option<String>,
    category: String,
    created_at: String,
    updated_at: Option<String>,
}

impl From<ProductDetail> for ProductListItem {
    fn from(product: ProductDetail) -> Self {
        Self {
            product_id: product.product_id,
            name: product.name,
            price: product.price,
            image_url: product.image_url,
            category: product.category,
            created_at: product.created_at,
            updated_at: product.updated_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductPage {
    content: Vec<ProductListItem>,
    total_pages: usize,
    total_elements: usize,
    first: bool,
    last: bool,
    size: usize,
    number_of_elements: usize,
}

struct ProductListOptions {
    page: usize,
    size: usize,
    descending: bool,
}

trait ProductRepository: Send + Sync {
    fn find_by_id(&self, product_id: Uuid) -> Option<ProductDetail>;
    fn find_by_seller(
        &self,
        seller_id: Uuid,
        options: &ProductListOptions,
    ) -> (Vec<ProductDetail>, usize);
}

struct LocalProductRepository {
    products: HashMap<Uuid, ProductDetail>,
}

impl LocalProductRepository {
    fn new(products: impl IntoIterator<Item = ProductDetail>) -> Self {
        Self {
            products: products
                .into_iter()
                .map(|product| (product.product_id, product))
                .collect(),
        }
    }
}

impl ProductRepository for LocalProductRepository {
    fn find_by_id(&self, product_id: Uuid) -> Option<ProductDetail> {
        self.products.get(&product_id).cloned()
    }

    fn find_by_seller(
        &self,
        seller_id: Uuid,
        options: &ProductListOptions,
    ) -> (Vec<ProductDetail>, usize) {
        let mut products = self
            .products
            .values()
            .filter(|product| product.seller_id == seller_id)
            .cloned()
            .collect::<Vec<_>>();
        products.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        if options.descending {
            products.reverse();
        }

        let total = products.len();
        let start = options.page.saturating_mul(options.size);
        let page = products
            .into_iter()
            .skip(start)
            .take(options.size)
            .collect();
        (page, total)
    }
}

#[derive(Clone)]
struct AppState {
    products: Arc<dyn ProductRepository>,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

fn app(products: Arc<dyn ProductRepository>) -> Router {
    Router::new()
        .route("/api/products", get(get_product_list))
        .route("/api/products/{product_id}", get(get_product_detail))
        .with_state(AppState { products })
}

async fn get_product_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> (StatusCode, Json<ApiResponse<ProductPage>>) {
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return response(status, None, message),
    };
    let options = match product_list_options(&query) {
        Ok(options) => options,
        Err((status, message)) => return response(status, None, message),
    };
    let (products, total_elements) = state.products.find_by_seller(seller_id, &options);
    let content = products
        .into_iter()
        .map(ProductListItem::from)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let total_pages = total_elements.div_ceil(options.size);

    response(
        StatusCode::OK,
        Some(ProductPage {
            content,
            total_pages,
            total_elements,
            first: options.page == 0,
            last: options.page.saturating_add(1) >= total_pages,
            size: options.size,
            number_of_elements,
        }),
        PRODUCT_LIST_FOUND_MESSAGE,
    )
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

fn product_list_options(
    query: &HashMap<String, String>,
) -> Result<ProductListOptions, (StatusCode, &'static str)> {
    let page = query
        .get("page")
        .map_or(Ok(0), |value| value.parse::<usize>())
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    let size = query
        .get("size")
        .map_or(Ok(20), |value| value.parse::<usize>())
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    if size == 0 {
        return Err((StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE));
    }

    let descending = match query.get("sort").map(String::as_str) {
        None => true,
        Some("createdAt") => false,
        Some(sort) => match sort.split_once(',') {
            Some(("createdAt", direction)) if direction.eq_ignore_ascii_case("desc") => true,
            Some(("createdAt", direction)) if direction.eq_ignore_ascii_case("asc") => false,
            _ => return Err((StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE)),
        },
    };

    Ok(ProductListOptions {
        page,
        size,
        descending,
    })
}

async fn get_product_detail(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
) -> (StatusCode, Json<ApiResponse<ProductDetail>>) {
    let Ok(product_id) = Uuid::parse_str(&product_id) else {
        return response(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };

    match state.products.find_by_id(product_id) {
        Some(product) => response(StatusCode::OK, Some(product), PRODUCT_FOUND_MESSAGE),
        None => response(StatusCode::NOT_FOUND, None, PRODUCT_NOT_FOUND_MESSAGE),
    }
}

fn response<T>(
    status: StatusCode,
    data: Option<T>,
    message: &'static str,
) -> (StatusCode, Json<ApiResponse<T>>) {
    (
        status,
        Json(ApiResponse {
            status: status.as_u16(),
            data,
            message,
        }),
    )
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app(Arc::new(LocalProductRepository::new([])))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    fn product() -> ProductDetail {
        ProductDetail {
            product_id: Uuid::new_v4(),
            name: "테스트 상품".to_owned(),
            price: 10_000,
            category: "FOOD".to_owned(),
            description: "상품 설명".to_owned(),
            stock: 3,
            original_link: Some("https://example.com/products/1".to_owned()),
            image_url: Some("https://example.com/products/1.png".to_owned()),
            seller_id: Uuid::new_v4(),
            created_at: "2026-01-01T00:00:00+09:00".to_owned(),
            updated_at: Some("2026-01-02T00:00:00+09:00".to_owned()),
        }
    }

    async fn get(
        products: impl IntoIterator<Item = ProductDetail>,
        path: &str,
    ) -> (StatusCode, serde_json::Value) {
        send(
            products,
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await
    }

    async fn get_as(
        products: impl IntoIterator<Item = ProductDetail>,
        path: &str,
        member_id: &str,
    ) -> (StatusCode, serde_json::Value) {
        send(
            products,
            Request::builder()
                .uri(path)
                .header("x-member-id", member_id)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    async fn send(
        products: impl IntoIterator<Item = ProductDetail>,
        request: Request<Body>,
    ) -> (StatusCode, serde_json::Value) {
        let response = app(Arc::new(LocalProductRepository::new(products)))
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn lists_only_the_sellers_products_with_legacy_sort_and_paging() {
        let seller_id = Uuid::new_v4();
        let mut older = product();
        older.seller_id = seller_id;
        older.name = "오래된 상품".to_owned();
        older.created_at = "2026-01-01T00:00:00+09:00".to_owned();
        let mut newer = product();
        newer.seller_id = seller_id;
        newer.name = "최신 상품".to_owned();
        newer.created_at = "2026-01-02T00:00:00+09:00".to_owned();
        let mut another_sellers = product();
        another_sellers.created_at = "2026-01-03T00:00:00+09:00".to_owned();
        let products = vec![older.clone(), newer.clone(), another_sellers];

        let (status, body) = get_as(
            products.clone(),
            "/api/products?page=1&size=1&sort=createdAt,desc",
            &seller_id.to_string(),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({
                "status": 200,
                "data": {
                    "content": [{
                        "productId": older.product_id,
                        "name": "오래된 상품",
                        "price": 10_000,
                        "imageUrl": "https://example.com/products/1.png",
                        "category": "FOOD",
                        "createdAt": "2026-01-01T00:00:00+09:00",
                        "updatedAt": "2026-01-02T00:00:00+09:00"
                    }],
                    "totalPages": 2,
                    "totalElements": 2,
                    "first": false,
                    "last": true,
                    "size": 1,
                    "numberOfElements": 1
                },
                "message": PRODUCT_LIST_FOUND_MESSAGE
            })
        );

        let (_, ascending) = get_as(
            products,
            "/api/products?size=1&sort=createdAt,ASC",
            &seller_id.to_string(),
        )
        .await;
        assert_eq!(
            ascending["data"]["content"][0]["productId"],
            older.product_id.to_string()
        );
    }

    #[tokio::test]
    async fn returns_an_empty_product_page() {
        let (status, body) = get_as([], "/api/products", &Uuid::new_v4().to_string()).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({
                "status": 200,
                "data": {
                    "content": [],
                    "totalPages": 0,
                    "totalElements": 0,
                    "first": true,
                    "last": true,
                    "size": 20,
                    "numberOfElements": 0
                },
                "message": PRODUCT_LIST_FOUND_MESSAGE
            })
        );
    }

    #[tokio::test]
    async fn product_list_validates_member_header_and_paging() {
        let (missing_status, missing_body) = get([], "/api/products").await;
        assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
        assert_eq!(missing_body["message"], NO_LOGIN_INFO_MESSAGE);

        let (invalid_status, invalid_body) = get_as([], "/api/products", "not-a-uuid").await;
        assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
        assert_eq!(invalid_body["message"], INVALID_PARAMETER_MESSAGE);

        let (paging_status, paging_body) =
            get_as([], "/api/products?size=0", &Uuid::new_v4().to_string()).await;
        assert_eq!(paging_status, StatusCode::BAD_REQUEST);
        assert_eq!(paging_body["message"], INVALID_PARAMETER_MESSAGE);
    }

    #[tokio::test]
    async fn returns_product_detail() {
        let product = product();
        let (status, body) = get(
            [product.clone()],
            &format!("/api/products/{}", product.product_id),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({
                "status": 200,
                "data": {
                    "productId": product.product_id,
                    "name": "테스트 상품",
                    "price": 10_000,
                    "category": "FOOD",
                    "description": "상품 설명",
                    "stock": 3,
                    "originalLink": "https://example.com/products/1",
                    "imageUrl": "https://example.com/products/1.png",
                    "sellerId": product.seller_id,
                    "createdAt": "2026-01-01T00:00:00+09:00",
                    "updatedAt": "2026-01-02T00:00:00+09:00"
                },
                "message": PRODUCT_FOUND_MESSAGE
            })
        );
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_product() {
        let (status, body) = get([], &format!("/api/products/{}", Uuid::new_v4())).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body,
            serde_json::json!({
                "status": 404,
                "data": null,
                "message": PRODUCT_NOT_FOUND_MESSAGE
            })
        );
    }

    #[tokio::test]
    async fn returns_bad_request_for_invalid_product_id() {
        let (status, body) = get([], "/api/products/not-a-uuid").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            serde_json::json!({
                "status": 400,
                "data": null,
                "message": INVALID_PARAMETER_MESSAGE
            })
        );
    }
}
