use super::GroupPurchaseState;
use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime, UtcOffset};
use uuid::Uuid;

#[derive(Deserialize)]
struct SearchRequest {
    ids: Option<Vec<Uuid>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRow {
    group_purchase_id: Uuid,
    min_quantity: i32,
    max_quantity: i32,
    title: String,
    description: Option<String>,
    image_url: Option<String>,
    discounted_price: i64,
    status: String,
    start_date: String,
    end_date: String,
    created_at: String,
    updated_at: Option<String>,
    current_quantity: i32,
    product_id: Uuid,
    category: String,
    price: i64,
    original_url: Option<String>,
    seller_id: Uuid,
}

pub(super) fn router() -> Router<GroupPurchaseState> {
    Router::new().route("/internal/purchases/search", post(find_by_ids))
}

async fn find_by_ids(
    State(state): State<GroupPurchaseState>,
    Json(request): Json<SearchRequest>,
) -> Json<Vec<SearchRow>> {
    Json(
        request
            .ids
            .unwrap_or_default()
            .into_iter()
            .filter_map(|purchase_id| {
                let purchase = state.purchases.find_by_id(purchase_id)?;
                let product = state.products.find_by_id(purchase.product_id)?;
                Some(SearchRow {
                    group_purchase_id: purchase.group_purchase_id,
                    min_quantity: purchase.min_quantity,
                    max_quantity: purchase.max_quantity,
                    title: purchase.title,
                    description: purchase.description,
                    image_url: purchase.image_url,
                    discounted_price: purchase.discounted_price,
                    status: purchase.status,
                    start_date: instant(&purchase.start_date),
                    end_date: instant(&purchase.end_date),
                    created_at: instant(&purchase.created_at),
                    updated_at: purchase.updated_at.as_deref().map(instant),
                    current_quantity: purchase.current_quantity,
                    product_id: product.product_id,
                    category: product.category,
                    price: product.price,
                    original_url: product.original_link,
                    seller_id: purchase.seller_id,
                })
            })
            .collect(),
    )
}

fn instant(value: &str) -> String {
    OffsetDateTime::parse(value, &Rfc3339)
        .expect("stored timestamps must be RFC 3339")
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .expect("RFC 3339 formatting must succeed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{group_purchase::GroupPurchaseRepository, LocalProductRepository, ProductDetail};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tower::ServiceExt;

    fn purchase(
        purchase_id: Uuid,
        product_id: Uuid,
        seller_id: Uuid,
    ) -> super::super::GroupPurchase {
        super::super::GroupPurchase {
            group_purchase_id: purchase_id,
            min_quantity: 2,
            max_quantity: 20,
            title: "사과 공동구매".to_owned(),
            description: Some("공동구매 설명".to_owned()),
            discounted_price: 8_000,
            current_quantity: 3,
            like_count: 0,
            start_date: "2099-01-01T09:00:00+09:00".to_owned(),
            end_date: "2099-01-31T00:00:00Z".to_owned(),
            seller_id,
            product_id,
            image_url: Some("https://example.com/purchase.png".to_owned()),
            status: "OPEN".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: Some("2026-01-02T00:00:00Z".to_owned()),
        }
    }

    fn product(product_id: Uuid) -> ProductDetail {
        ProductDetail {
            product_id,
            name: "사과".to_owned(),
            price: 10_000,
            category: "FOOD".to_owned(),
            description: "상품 설명".to_owned(),
            stock: 100,
            original_link: Some("https://example.com/original".to_owned()),
            image_url: None,
            seller_id: Uuid::new_v4(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    async fn search(
        purchases: GroupPurchaseRepository,
        products: LocalProductRepository,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = super::super::router(purchases, Arc::new(products))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/internal/purchases/search")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn returns_complete_wrapperless_rows_in_requested_order_with_duplicates() {
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let first_product_id = Uuid::new_v4();
        let second_product_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(first_id, first_product_id, seller_id),
            purchase(second_id, second_product_id, seller_id),
        ]);
        let products =
            LocalProductRepository::new([product(first_product_id), product(second_product_id)]);

        let (status, body) = search(
            purchases,
            products,
            json!({"ids": [second_id, first_id, second_id]}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 3);
        assert_eq!(body[0]["groupPurchaseId"], second_id.to_string());
        assert_eq!(body[1]["groupPurchaseId"], first_id.to_string());
        assert_eq!(body[2]["groupPurchaseId"], second_id.to_string());
        assert_eq!(body[1]["minQuantity"], 2);
        assert_eq!(body[1]["maxQuantity"], 20);
        assert_eq!(body[1]["title"], "사과 공동구매");
        assert_eq!(body[1]["description"], "공동구매 설명");
        assert_eq!(body[1]["imageUrl"], "https://example.com/purchase.png");
        assert_eq!(body[1]["discountedPrice"], 8_000);
        assert_eq!(body[1]["status"], "OPEN");
        assert_eq!(body[1]["startDate"], "2099-01-01T00:00:00Z");
        assert_eq!(body[1]["endDate"], "2099-01-31T00:00:00Z");
        assert_eq!(body[1]["createdAt"], "2026-01-01T00:00:00Z");
        assert_eq!(body[1]["updatedAt"], "2026-01-02T00:00:00Z");
        assert_eq!(body[1]["currentQuantity"], 3);
        assert_eq!(body[1]["productId"], first_product_id.to_string());
        assert_eq!(body[1]["category"], "FOOD");
        assert_eq!(body[1]["price"], 10_000);
        assert_eq!(body[1]["originalUrl"], "https://example.com/original");
        assert_eq!(body[1]["sellerId"], seller_id.to_string());
        assert!(body.get("status").is_none());
        assert!(body.get("data").is_none());
        assert!(body.get("message").is_none());
    }

    #[tokio::test]
    async fn null_and_empty_ids_return_empty_arrays() {
        for body in [json!({"ids": null}), json!({"ids": []})] {
            let (status, response) = search(
                GroupPurchaseRepository::empty(),
                LocalProductRepository::new([]),
                body,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(response, json!([]));
        }
    }

    #[tokio::test]
    async fn missing_purchases_and_products_are_omitted() {
        let joined_id = Uuid::new_v4();
        let missing_product_purchase_id = Uuid::new_v4();
        let joined_product_id = Uuid::new_v4();
        let missing_purchase_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(joined_id, joined_product_id, Uuid::new_v4()),
            purchase(missing_product_purchase_id, Uuid::new_v4(), Uuid::new_v4()),
        ]);
        let products = LocalProductRepository::new([product(joined_product_id)]);

        let (status, body) = search(
            purchases,
            products,
            json!({"ids": [missing_purchase_id, missing_product_purchase_id, joined_id]}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["groupPurchaseId"], joined_id.to_string());
    }
}
