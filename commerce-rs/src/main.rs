use axum::{
    extract::{rejection::JsonRejection, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use image::ImageRepository;
use local_auth::AuthRepository;
use seller_balance::SellerBalanceRepository;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

mod group_purchase;
mod image;
mod local_auth;
mod seller_balance;

const PRODUCT_FOUND_MESSAGE: &str = "상품 조회가 완료되었습니다.";
const PRODUCT_LIST_FOUND_MESSAGE: &str = "내가 등록한 상품 목록 조회가 완료되었습니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";
const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const PRODUCT_CREATED_MESSAGE: &str = "상품이 등록되었습니다.";
const PRODUCT_UPDATED_MESSAGE: &str = "상품 수정이 완료되었습니다.";
const PRODUCT_DELETED_MESSAGE: &str = "상품이 삭제되었습니다.";
const FORBIDDEN_NOT_PRODUCT_OWNER_MESSAGE: &str = "본인이 등록한 상품이 아닙니다.";
const ACTIVE_GROUP_PURCHASE_MESSAGE: &str = "진행 중이거나 예정된 공동구매가 존재합니다.";

const PRODUCT_CATEGORIES: [&str; 9] = [
    "HOME",
    "FOOD",
    "HEALTH",
    "BEAUTY",
    "FASHION",
    "ELECTRONICS",
    "KIDS",
    "HOBBY",
    "PET",
];

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductVectorItem {
    product_id: Uuid,
    name: String,
    description: String,
    category: String,
}

impl From<ProductDetail> for ProductVectorItem {
    fn from(product: ProductDetail) -> Self {
        Self {
            product_id: product.product_id,
            name: product.name,
            description: product.description,
            category: product.category,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductVectorPage {
    content: Vec<ProductVectorItem>,
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

struct InternalProductOptions {
    page: usize,
    size: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductCreateRequest {
    name: Option<String>,
    price: Option<i64>,
    category: Option<String>,
    description: Option<String>,
    #[serde(default)]
    stock: i32,
    original_url: Option<String>,
    image_url: Option<String>,
    idempotency_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductUpdateRequest {
    name: String,
    price: i64,
    category: String,
    description: String,
    stock: i32,
    original_link: Option<String>,
    image_url: Option<String>,
}

struct NewProduct {
    name: String,
    price: i64,
    category: String,
    description: String,
    stock: i32,
    original_link: Option<String>,
    image_url: Option<String>,
    idempotency_key: String,
    seller_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductRegisterInfo {
    product_id: Uuid,
    name: String,
    price: i64,
    category: String,
    description: String,
    stock: i32,
    original_url: Option<String>,
    image_url: Option<String>,
    seller_id: Uuid,
    created_at: String,
}

impl From<ProductDetail> for ProductRegisterInfo {
    fn from(product: ProductDetail) -> Self {
        Self {
            product_id: product.product_id,
            name: product.name,
            price: product.price,
            category: product.category,
            description: product.description,
            stock: product.stock,
            original_url: product.original_link,
            image_url: product.image_url,
            seller_id: product.seller_id,
            created_at: product.created_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductUpdateInfo {
    product_id: Uuid,
    name: String,
    price: i64,
    category: String,
    description: String,
    stock: i32,
    original_link: Option<String>,
    image_url: Option<String>,
    seller_id: Uuid,
}

impl From<ProductDetail> for ProductUpdateInfo {
    fn from(product: ProductDetail) -> Self {
        Self {
            product_id: product.product_id,
            name: product.name,
            price: product.price,
            category: product.category,
            description: product.description,
            stock: product.stock,
            original_link: product.original_link,
            image_url: product.image_url,
            seller_id: product.seller_id,
        }
    }
}

enum DeleteProductResult {
    HardDeleted,
    SoftDeleted,
    NotFound,
    Forbidden,
    ActiveGroupPurchase,
}

trait ProductRepository: Send + Sync {
    fn find_by_id(&self, product_id: Uuid) -> Option<ProductDetail>;
    fn find_by_seller(
        &self,
        seller_id: Uuid,
        options: &ProductListOptions,
    ) -> (Vec<ProductDetail>, usize);
    fn find_all_for_vector(&self, options: &InternalProductOptions) -> (Vec<ProductDetail>, usize);
    fn create(&self, product: NewProduct) -> ProductDetail;
    fn update(&self, product_id: Uuid, update: ProductUpdateRequest) -> Option<ProductDetail>;
    fn delete(&self, product_id: Uuid, seller_id: Uuid) -> DeleteProductResult;
}

struct LocalProductRepository {
    store: RwLock<LocalProductStore>,
}

struct LocalProductStore {
    products: HashMap<Uuid, StoredProduct>,
    idempotency_keys: HashMap<String, Uuid>,
    used_in_group_purchase: HashSet<Uuid>,
    active_group_purchase: HashSet<Uuid>,
}

struct StoredProduct {
    detail: ProductDetail,
    deleted_at: Option<String>,
}

impl LocalProductRepository {
    fn new(products: impl IntoIterator<Item = ProductDetail>) -> Self {
        Self {
            store: RwLock::new(LocalProductStore {
                products: products
                    .into_iter()
                    .map(|product| {
                        (
                            product.product_id,
                            StoredProduct {
                                detail: product,
                                deleted_at: None,
                            },
                        )
                    })
                    .collect(),
                idempotency_keys: HashMap::new(),
                used_in_group_purchase: HashSet::new(),
                active_group_purchase: HashSet::new(),
            }),
        }
    }

    #[cfg(test)]
    fn mark_group_purchase_usage(&self, product_id: Uuid, active: bool) {
        let mut store = self.store.write().unwrap();
        store.used_in_group_purchase.insert(product_id);
        if active {
            store.active_group_purchase.insert(product_id);
        }
    }

    #[cfg(test)]
    fn is_soft_deleted(&self, product_id: Uuid) -> bool {
        self.store
            .read()
            .unwrap()
            .products
            .get(&product_id)
            .is_some_and(|product| product.deleted_at.is_some())
    }
}

impl ProductRepository for LocalProductRepository {
    fn find_by_id(&self, product_id: Uuid) -> Option<ProductDetail> {
        self.store
            .read()
            .unwrap()
            .products
            .get(&product_id)
            .map(|product| product.detail.clone())
    }

    fn find_by_seller(
        &self,
        seller_id: Uuid,
        options: &ProductListOptions,
    ) -> (Vec<ProductDetail>, usize) {
        let store = self.store.read().unwrap();
        let mut products = store
            .products
            .values()
            .filter(|product| product.detail.seller_id == seller_id)
            .map(|product| product.detail.clone())
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

    fn find_all_for_vector(&self, options: &InternalProductOptions) -> (Vec<ProductDetail>, usize) {
        let store = self.store.read().unwrap();
        let mut products = store
            .products
            .values()
            .map(|product| product.detail.clone())
            .collect::<Vec<_>>();
        products.sort_by_key(|product| product.product_id);

        let total = products.len();
        let start = options.page.saturating_mul(options.size);
        let page = products
            .into_iter()
            .skip(start)
            .take(options.size)
            .collect();
        (page, total)
    }

    fn create(&self, product: NewProduct) -> ProductDetail {
        let mut store = self.store.write().unwrap();
        if let Some(product_id) = store.idempotency_keys.get(&product.idempotency_key) {
            return store.products[product_id].detail.clone();
        }

        let product_id = Uuid::new_v4();
        let detail = ProductDetail {
            product_id,
            name: product.name,
            price: product.price,
            category: product.category,
            description: product.description,
            stock: product.stock,
            original_link: product.original_link,
            image_url: product.image_url,
            seller_id: product.seller_id,
            created_at: now(),
            updated_at: None,
        };
        store
            .idempotency_keys
            .insert(product.idempotency_key, product_id);
        store.products.insert(
            product_id,
            StoredProduct {
                detail: detail.clone(),
                deleted_at: None,
            },
        );
        detail
    }

    fn update(&self, product_id: Uuid, update: ProductUpdateRequest) -> Option<ProductDetail> {
        let mut store = self.store.write().unwrap();
        let product = store.products.get_mut(&product_id)?;
        product.detail.name = update.name;
        product.detail.price = update.price;
        product.detail.category = update.category;
        product.detail.description = update.description;
        product.detail.stock = update.stock;
        product.detail.original_link = update.original_link;
        product.detail.image_url = update.image_url;
        product.detail.updated_at = Some(now());
        Some(product.detail.clone())
    }

    fn delete(&self, product_id: Uuid, seller_id: Uuid) -> DeleteProductResult {
        let mut store = self.store.write().unwrap();
        let Some(product) = store.products.get(&product_id) else {
            return DeleteProductResult::NotFound;
        };
        if product.detail.seller_id != seller_id {
            return DeleteProductResult::Forbidden;
        }
        if store.active_group_purchase.contains(&product_id) {
            return DeleteProductResult::ActiveGroupPurchase;
        }
        if store.used_in_group_purchase.contains(&product_id) {
            store.products.get_mut(&product_id).unwrap().deleted_at = Some(now());
            DeleteProductResult::SoftDeleted
        } else {
            store.products.remove(&product_id);
            store
                .idempotency_keys
                .retain(|_, stored_product_id| *stored_product_id != product_id);
            DeleteProductResult::HardDeleted
        }
    }
}

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
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

fn app(
    products: Arc<dyn ProductRepository>,
    images: ImageRepository,
    seller_balances: SellerBalanceRepository,
    auth: AuthRepository,
) -> Router {
    let group_purchases = group_purchase::GroupPurchaseRepository::empty();
    Router::new()
        .route("/internal/products/vector", get(get_products_for_vector))
        .route("/api/products", get(get_product_list).post(create_product))
        .route(
            "/api/products/{product_id}",
            get(get_product_detail)
                .patch(update_product)
                .delete(delete_product),
        )
        .with_state(AppState {
            products: products.clone(),
        })
        .merge(group_purchase::router(group_purchases, products))
        .merge(image::router(images))
        .merge(seller_balance::router(seller_balances))
        .merge(local_auth::router(auth))
}

async fn get_products_for_vector(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let options = match internal_product_options(&query) {
        Ok(options) => options,
        Err((status, message)) => {
            return response::<serde_json::Value>(status, None, message).into_response();
        }
    };
    let (products, total_elements) = state.products.find_all_for_vector(&options);
    let content = products
        .into_iter()
        .map(ProductVectorItem::from)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let total_pages = total_elements.div_ceil(options.size);

    Json(ProductVectorPage {
        content,
        total_pages,
        total_elements,
        first: options.page == 0,
        last: options.page.saturating_add(1) >= total_pages,
        size: options.size,
        number_of_elements,
    })
    .into_response()
}

async fn create_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ProductCreateRequest>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse<ProductRegisterInfo>>) {
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return response(status, None, message),
    };
    let Json(request) = match payload {
        Ok(request) => request,
        Err(_) => return response(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE),
    };
    let product = match new_product(request, seller_id) {
        Ok(product) => product,
        Err((status, message)) => return response(status, None, message),
    };

    response(
        StatusCode::CREATED,
        Some(ProductRegisterInfo::from(state.products.create(product))),
        PRODUCT_CREATED_MESSAGE,
    )
}

async fn update_product(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    payload: Result<Json<ProductUpdateRequest>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse<ProductUpdateInfo>>) {
    let Ok(product_id) = Uuid::parse_str(&product_id) else {
        return response(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };
    let Json(update) = match payload {
        Ok(update) if valid_category(&update.category) => update,
        _ => return response(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE),
    };

    match state.products.update(product_id, update) {
        Some(product) => response(
            StatusCode::OK,
            Some(ProductUpdateInfo::from(product)),
            PRODUCT_UPDATED_MESSAGE,
        ),
        None => response(StatusCode::NOT_FOUND, None, PRODUCT_NOT_FOUND_MESSAGE),
    }
}

async fn delete_product(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    headers: HeaderMap,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let seller_id = match member_id(&headers) {
        Ok(seller_id) => seller_id,
        Err((status, message)) => return response(status, None, message),
    };
    let Ok(product_id) = Uuid::parse_str(&product_id) else {
        return response(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };

    match state.products.delete(product_id, seller_id) {
        DeleteProductResult::HardDeleted | DeleteProductResult::SoftDeleted => {
            response(StatusCode::OK, None, PRODUCT_DELETED_MESSAGE)
        }
        DeleteProductResult::NotFound => {
            response(StatusCode::NOT_FOUND, None, PRODUCT_NOT_FOUND_MESSAGE)
        }
        DeleteProductResult::Forbidden => response(
            StatusCode::FORBIDDEN,
            None,
            FORBIDDEN_NOT_PRODUCT_OWNER_MESSAGE,
        ),
        DeleteProductResult::ActiveGroupPurchase => {
            response(StatusCode::CONFLICT, None, ACTIVE_GROUP_PURCHASE_MESSAGE)
        }
    }
}

fn new_product(
    request: ProductCreateRequest,
    seller_id: Uuid,
) -> Result<NewProduct, (StatusCode, &'static str)> {
    let valid = request
        .name
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
        && request.price.is_some_and(|value| value > 0)
        && request
            .category
            .as_ref()
            .is_some_and(|value| valid_category(value))
        && request
            .description
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
        && request.stock > 0
        && request
            .idempotency_key
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty());
    if !valid {
        return Err((StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE));
    }

    Ok(NewProduct {
        name: request.name.unwrap(),
        price: request.price.unwrap(),
        category: request.category.unwrap(),
        description: request.description.unwrap(),
        stock: request.stock,
        original_link: request.original_url,
        image_url: request.image_url,
        idempotency_key: request.idempotency_key.unwrap(),
        seller_id,
    })
}

fn valid_category(category: &str) -> bool {
    PRODUCT_CATEGORIES.contains(&category)
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

fn internal_product_options(
    query: &HashMap<String, String>,
) -> Result<InternalProductOptions, (StatusCode, &'static str)> {
    let page = query
        .get("page")
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Required request parameter 'page' for method parameter type int is not present",
        ))?
        .parse::<i32>()
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    let size = query
        .get("size")
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Required request parameter 'size' for method parameter type int is not present",
        ))?
        .parse::<i32>()
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    if page < 0 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Page index must not be less than zero",
        ));
    }
    if size < 1 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Page size must not be less than one",
        ));
    }

    Ok(InternalProductOptions {
        page: page as usize,
        size: size as usize,
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
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("DATABASE_URL must be set"))?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(std::io::Error::other)?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .map_err(std::io::Error::other)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(
        listener,
        app(
            Arc::new(LocalProductRepository::new([])),
            ImageRepository::postgres(pool.clone()),
            SellerBalanceRepository::postgres(pool.clone()),
            AuthRepository::postgres(pool),
        ),
    )
    .await
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
        send_to(Arc::new(LocalProductRepository::new(products)), request).await
    }

    async fn send_to(
        products: Arc<LocalProductRepository>,
        request: Request<Body>,
    ) -> (StatusCode, serde_json::Value) {
        let response = app(
            products,
            ImageRepository::default(),
            SellerBalanceRepository::default(),
            AuthRepository::default(),
        )
        .oneshot(request)
        .await
        .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    async fn write(
        products: Arc<LocalProductRepository>,
        method: &str,
        path: &str,
        member_id: Option<Uuid>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder().method(method).uri(path);
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
        send_to(products, request.body(body).unwrap()).await
    }

    fn create_body(idempotency_key: &str) -> serde_json::Value {
        serde_json::json!({
            "name": "새 상품",
            "price": 20_000,
            "category": "FOOD",
            "description": "새 상품 설명",
            "stock": 10,
            "originalUrl": "https://example.com/original",
            "imageUrl": null,
            "idempotencyKey": idempotency_key
        })
    }

    #[tokio::test]
    async fn creates_a_product_and_replays_the_idempotency_key() {
        let products = Arc::new(LocalProductRepository::new([]));
        let seller_id = Uuid::new_v4();

        let (status, created) = write(
            products.clone(),
            "POST",
            "/api/products",
            Some(seller_id),
            Some(create_body("create-key")),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["status"], 201);
        assert_eq!(created["data"]["stock"], 10);
        assert_eq!(
            created["data"]["originalUrl"],
            "https://example.com/original"
        );
        assert_eq!(created["data"]["sellerId"], seller_id.to_string());
        assert_eq!(created["message"], PRODUCT_CREATED_MESSAGE);

        let mut replay_body = create_body("create-key");
        replay_body["name"] = serde_json::json!("무시되는 상품명");
        let (replay_status, replay) = write(
            products,
            "POST",
            "/api/products",
            Some(seller_id),
            Some(replay_body),
        )
        .await;
        assert_eq!(replay_status, StatusCode::CREATED);
        assert_eq!(replay["data"]["productId"], created["data"]["productId"]);
        assert_eq!(replay["data"]["name"], "새 상품");
    }

    #[tokio::test]
    async fn create_product_validates_header_and_body() {
        let products = Arc::new(LocalProductRepository::new([]));
        let (missing_header, missing_body) = write(
            products.clone(),
            "POST",
            "/api/products",
            None,
            Some(create_body("missing-header")),
        )
        .await;
        assert_eq!(missing_header, StatusCode::UNAUTHORIZED);
        assert_eq!(missing_body["message"], NO_LOGIN_INFO_MESSAGE);

        let mut invalid = create_body("invalid-body");
        invalid["stock"] = serde_json::json!(0);
        let (invalid_status, invalid_body) = write(
            products,
            "POST",
            "/api/products",
            Some(Uuid::new_v4()),
            Some(invalid),
        )
        .await;
        assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
        assert_eq!(invalid_body["message"], INVALID_PARAMETER_MESSAGE);
    }

    #[tokio::test]
    async fn updates_stock_and_returns_not_found_for_a_missing_product() {
        let product = product();
        let product_id = product.product_id;
        let products = Arc::new(LocalProductRepository::new([product]));
        let update = serde_json::json!({
            "name": "수정 상품",
            "price": 30_000,
            "category": "BEAUTY",
            "description": "수정 설명",
            "stock": 25,
            "originalLink": null,
            "imageUrl": "https://example.com/updated.png"
        });

        let (status, updated) = write(
            products.clone(),
            "PATCH",
            &format!("/api/products/{product_id}"),
            None,
            Some(update.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["data"]["stock"], 25);
        assert_eq!(updated["data"]["name"], "수정 상품");
        assert_eq!(products.find_by_id(product_id).unwrap().stock, 25);

        let (missing_status, missing) = write(
            products,
            "PATCH",
            &format!("/api/products/{}", Uuid::new_v4()),
            None,
            Some(update),
        )
        .await;
        assert_eq!(missing_status, StatusCode::NOT_FOUND);
        assert_eq!(missing["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn delete_enforces_ownership_and_missing_product_errors() {
        let product = product();
        let product_id = product.product_id;
        let products = Arc::new(LocalProductRepository::new([product]));

        let (forbidden_status, forbidden) = write(
            products.clone(),
            "DELETE",
            &format!("/api/products/{product_id}"),
            Some(Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(forbidden_status, StatusCode::FORBIDDEN);
        assert_eq!(forbidden["message"], FORBIDDEN_NOT_PRODUCT_OWNER_MESSAGE);

        let (missing_status, missing) = write(
            products,
            "DELETE",
            &format!("/api/products/{}", Uuid::new_v4()),
            Some(Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(missing_status, StatusCode::NOT_FOUND);
        assert_eq!(missing["message"], PRODUCT_NOT_FOUND_MESSAGE);
    }

    #[tokio::test]
    async fn delete_hard_deletes_unused_and_soft_deletes_used_products() {
        let hard_deleted = product();
        let soft_deleted = product();
        let active = product();
        let seller_id = hard_deleted.seller_id;
        let hard_deleted_id = hard_deleted.product_id;
        let soft_deleted_id = soft_deleted.product_id;
        let active_id = active.product_id;
        let mut soft_deleted = soft_deleted;
        soft_deleted.seller_id = seller_id;
        let mut active = active;
        active.seller_id = seller_id;
        let products = Arc::new(LocalProductRepository::new([
            hard_deleted,
            soft_deleted,
            active,
        ]));
        products.mark_group_purchase_usage(soft_deleted_id, false);
        products.mark_group_purchase_usage(active_id, true);

        let (hard_status, _) = write(
            products.clone(),
            "DELETE",
            &format!("/api/products/{hard_deleted_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(hard_status, StatusCode::OK);
        assert!(products.find_by_id(hard_deleted_id).is_none());

        let (soft_status, soft_body) = write(
            products.clone(),
            "DELETE",
            &format!("/api/products/{soft_deleted_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(soft_status, StatusCode::OK);
        assert_eq!(soft_body["message"], PRODUCT_DELETED_MESSAGE);
        assert!(products.find_by_id(soft_deleted_id).is_some());
        assert!(products.is_soft_deleted(soft_deleted_id));

        let (active_status, active_body) = write(
            products.clone(),
            "DELETE",
            &format!("/api/products/{active_id}"),
            Some(seller_id),
            None,
        )
        .await;
        assert_eq!(active_status, StatusCode::CONFLICT);
        assert_eq!(active_body["message"], ACTIVE_GROUP_PURCHASE_MESSAGE);
        assert!(products.find_by_id(active_id).is_some());
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
    async fn returns_vector_products_in_product_id_order_with_paging() {
        let mut first = product();
        first.product_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        first.name = "첫 상품".to_owned();
        let mut second = product();
        second.product_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        second.name = "두 번째 상품".to_owned();
        let mut third = product();
        third.product_id = Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap();
        third.name = "세 번째 상품".to_owned();

        let (status, body) = get(
            [third, first, second.clone()],
            "/internal/products/vector?page=1&size=1",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({
                "content": [{
                    "productId": second.product_id,
                    "name": "두 번째 상품",
                    "description": "상품 설명",
                    "category": "FOOD"
                }],
                "totalPages": 3,
                "totalElements": 3,
                "first": false,
                "last": false,
                "size": 1,
                "numberOfElements": 1
            })
        );
    }

    #[tokio::test]
    async fn returns_an_empty_vector_page() {
        let (status, body) = get([], "/internal/products/vector?page=0&size=10").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({
                "content": [],
                "totalPages": 0,
                "totalElements": 0,
                "first": true,
                "last": true,
                "size": 10,
                "numberOfElements": 0
            })
        );
    }

    #[tokio::test]
    async fn vector_feed_preserves_legacy_invalid_paging_errors() {
        let (invalid_status, invalid) =
            get([], "/internal/products/vector?page=invalid&size=10").await;
        assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
        assert_eq!(invalid["message"], INVALID_PARAMETER_MESSAGE);

        let (missing_status, missing) = get([], "/internal/products/vector?size=10").await;
        assert_eq!(missing_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            missing["message"],
            "Required request parameter 'page' for method parameter type int is not present"
        );

        let (negative_status, negative) =
            get([], "/internal/products/vector?page=-1&size=10").await;
        assert_eq!(negative_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(negative["message"], "Page index must not be less than zero");

        let (zero_status, zero) = get([], "/internal/products/vector?page=0&size=0").await;
        assert_eq!(zero_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(zero["message"], "Page size must not be less than one");
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
