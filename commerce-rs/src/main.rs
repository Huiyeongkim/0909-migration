use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

const PRODUCT_FOUND_MESSAGE: &str = "상품 조회가 완료되었습니다.";
const PRODUCT_NOT_FOUND_MESSAGE: &str = "상품을 찾을 수 없습니다.";
const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";

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

trait ProductRepository: Send + Sync {
    fn find_by_id(&self, product_id: Uuid) -> Option<ProductDetail>;
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
        .route("/api/products/{product_id}", get(get_product_detail))
        .with_state(AppState { products })
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
        let response = app(Arc::new(LocalProductRepository::new(products)))
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
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
