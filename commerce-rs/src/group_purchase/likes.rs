use super::{
    api_response, error, member_id, GroupPurchaseRepository, GroupPurchaseState,
    INVALID_PARAMETER_MESSAGE, PURCHASE_NOT_FOUND_MESSAGE,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use uuid::Uuid;

const ALREADY_LIKED_MESSAGE: &str = "이미 찜한 상태입니다.";
const LIKE_NOT_FOUND_MESSAGE: &str = "찜한 상태가 아닙니다.";
const LIKED_MESSAGE: &str = "공동구매가 찜 되었습니다.";
const UNLIKED_MESSAGE: &str = "공동구매 찜이 해제되었습니다.";

#[derive(Debug, PartialEq, Eq)]
enum LikeResult {
    Liked,
    PurchaseNotFound,
    AlreadyLiked,
}

#[derive(Debug, PartialEq, Eq)]
enum UnlikeResult {
    Unliked,
    NotLiked,
}

impl GroupPurchaseRepository {
    fn like(&self, member_id: Uuid, purchase_id: Uuid) -> LikeResult {
        let mut purchases = self.purchases.write().unwrap();
        let Some(purchase) = purchases
            .iter_mut()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
        else {
            return LikeResult::PurchaseNotFound;
        };
        let mut likes = self.likes.write().unwrap();
        if !likes.insert((member_id, purchase_id)) {
            return LikeResult::AlreadyLiked;
        }
        purchase.like_count += 1;
        LikeResult::Liked
    }

    fn unlike(&self, member_id: Uuid, purchase_id: Uuid) -> UnlikeResult {
        let mut purchases = self.purchases.write().unwrap();
        let mut likes = self.likes.write().unwrap();
        if !likes.remove(&(member_id, purchase_id)) {
            return UnlikeResult::NotLiked;
        }
        if let Some(purchase) = purchases
            .iter_mut()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
        {
            purchase.like_count -= 1;
        }
        UnlikeResult::Unliked
    }

    #[cfg(test)]
    fn like_count(&self, purchase_id: Uuid) -> Option<i32> {
        self.purchases
            .read()
            .unwrap()
            .iter()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .map(|purchase| purchase.like_count)
    }
}

pub(super) async fn like(
    State(state): State<GroupPurchaseState>,
    Path(purchase_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (member_id, purchase_id) = match request_ids(&headers, &purchase_id) {
        Ok(ids) => ids,
        Err((status, message)) => return error(status, message),
    };
    match state.purchases.like(member_id, purchase_id) {
        LikeResult::Liked => api_response::<()>(StatusCode::OK, None, LIKED_MESSAGE),
        LikeResult::PurchaseNotFound => error(StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE),
        LikeResult::AlreadyLiked => error(StatusCode::BAD_REQUEST, ALREADY_LIKED_MESSAGE),
    }
}

pub(super) async fn unlike(
    State(state): State<GroupPurchaseState>,
    Path(purchase_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (member_id, purchase_id) = match request_ids(&headers, &purchase_id) {
        Ok(ids) => ids,
        Err((status, message)) => return error(status, message),
    };
    match state.purchases.unlike(member_id, purchase_id) {
        UnlikeResult::Unliked => api_response::<()>(StatusCode::OK, None, UNLIKED_MESSAGE),
        UnlikeResult::NotLiked => error(StatusCode::BAD_REQUEST, LIKE_NOT_FOUND_MESSAGE),
    }
}

fn request_ids(
    headers: &HeaderMap,
    purchase_id: &str,
) -> Result<(Uuid, Uuid), (StatusCode, &'static str)> {
    let purchase_id = Uuid::parse_str(purchase_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    Ok((member_id(headers)?, purchase_id))
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

    fn purchase(purchase_id: Uuid) -> super::super::GroupPurchase {
        super::super::GroupPurchase {
            group_purchase_id: purchase_id,
            min_quantity: 2,
            max_quantity: 20,
            title: "사과 공동구매".to_owned(),
            description: None,
            discounted_price: 8_000,
            current_quantity: 0,
            like_count: 0,
            start_date: "2099-01-01T00:00:00Z".to_owned(),
            end_date: "2099-01-31T00:00:00Z".to_owned(),
            seller_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            image_url: None,
            status: "SCHEDULED".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    async fn send(
        repository: GroupPurchaseRepository,
        method: &str,
        purchase_id: Uuid,
        member_id: Option<Uuid>,
    ) -> (StatusCode, Value) {
        let app = super::super::router(repository, LocalProductRepository::new([]));
        let mut request = Request::builder()
            .method(method)
            .uri(format!("/api/purchases/{purchase_id}/likes"));
        if let Some(member_id) = member_id {
            request = request.header("x-member-id", member_id.to_string());
        }
        let response = app
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn like_and_unlike_preserve_membership_and_count() {
        let purchase_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let repository = GroupPurchaseRepository::new([purchase(purchase_id)]);

        let (status, body) = send(repository.clone(), "POST", purchase_id, Some(member_id)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"], LIKED_MESSAGE);
        assert_eq!(repository.like_count(purchase_id), Some(1));

        let (status, body) = send(repository.clone(), "POST", purchase_id, Some(member_id)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], ALREADY_LIKED_MESSAGE);
        assert_eq!(repository.like_count(purchase_id), Some(1));

        let (status, body) = send(repository.clone(), "DELETE", purchase_id, Some(member_id)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"], UNLIKED_MESSAGE);
        assert_eq!(repository.like_count(purchase_id), Some(0));

        let (status, body) = send(repository.clone(), "DELETE", purchase_id, Some(member_id)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], LIKE_NOT_FOUND_MESSAGE);
        assert_eq!(repository.like_count(purchase_id), Some(0));
    }

    #[tokio::test]
    async fn like_requires_a_member_and_existing_purchase() {
        let missing_purchase_id = Uuid::new_v4();
        let repository = GroupPurchaseRepository::empty();

        let (status, body) = send(
            repository.clone(),
            "POST",
            missing_purchase_id,
            Some(Uuid::new_v4()),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["message"], PURCHASE_NOT_FOUND_MESSAGE);

        let (status, body) = send(repository, "POST", missing_purchase_id, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["message"], super::super::NO_LOGIN_INFO_MESSAGE);
    }

    #[test]
    fn duplicate_concurrent_like_increments_once() {
        let purchase_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let repository = GroupPurchaseRepository::new([purchase(purchase_id)]);
        let results = std::thread::scope(|scope| {
            (0..16)
                .map(|_| {
                    let repository = repository.clone();
                    scope.spawn(move || repository.like(member_id, purchase_id))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect::<Vec<_>>()
        });

        assert_eq!(
            results
                .iter()
                .filter(|result| **result == LikeResult::Liked)
                .count(),
            1
        );
        assert_eq!(repository.like_count(purchase_id), Some(1));
    }
}
