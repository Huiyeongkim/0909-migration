use axum::{
    extract::{Json, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

#[cfg(test)]
use std::{collections::HashMap, sync::Arc};
#[cfg(test)]
use tokio::sync::Mutex;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";

#[derive(Clone)]
pub struct SellerBalanceRepository {
    backend: RepositoryBackend,
}

#[derive(Clone)]
enum RepositoryBackend {
    Postgres(PgPool),
    #[cfg(test)]
    Test(Arc<Mutex<TestStore>>),
}

#[cfg(test)]
#[derive(Default)]
struct TestStore {
    balances: HashMap<Uuid, SellerBalanceRecord>,
    histories: Vec<SellerBalanceHistoryInfo>,
}

#[derive(Clone)]
struct SellerBalanceRecord {
    balance_id: Uuid,
    member_id: Uuid,
    balance: i64,
}

#[derive(Debug)]
enum RepositoryError {
    Duplicate,
    NotFound,
    Database(sqlx::Error),
    InvalidTimestamp,
}

impl From<sqlx::Error> for RepositoryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SellerBalanceRequest {
    seller_id: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SellerBalanceThumbnailInfo {
    seller_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SellerBalanceInfo {
    seller_balance_id: Uuid,
    member_id: Uuid,
    balance: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SellerBalanceHistoryInfo {
    seller_balance_history_id: Uuid,
    member_id: Uuid,
    seller_payout_id: Option<Uuid>,
    order_settlement_id: Option<Uuid>,
    amount: i64,
    status: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SellerBalanceHistoryPage {
    content: Vec<SellerBalanceHistoryInfo>,
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

#[derive(Deserialize)]
struct HistoryQuery {
    page: Option<usize>,
    size: Option<usize>,
    sort: Option<String>,
}

#[derive(Clone, Copy)]
enum SortField {
    CreatedAt,
    Amount,
    Status,
    HistoryId,
}

impl SortField {
    fn sql(self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::Amount => "amount",
            Self::Status => "status",
            Self::HistoryId => "history_id",
        }
    }
}

#[derive(Clone, Copy)]
struct HistorySort {
    field: SortField,
    descending: bool,
}

impl SellerBalanceRepository {
    pub fn postgres(pool: PgPool) -> Self {
        Self {
            backend: RepositoryBackend::Postgres(pool),
        }
    }

    async fn create(&self, seller_id: Uuid) -> Result<SellerBalanceThumbnailInfo, RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let result = sqlx::query(
                    "INSERT INTO settlement_schema.seller_balance \
                     (balance_id, member_id, settlement_balance, created_at) \
                     VALUES ($1, $2, 0, NOW())",
                )
                .bind(Uuid::new_v4())
                .bind(seller_id)
                .execute(pool)
                .await;
                match result {
                    Ok(_) => {}
                    Err(sqlx::Error::Database(error))
                        if error.code().as_deref() == Some("23505") =>
                    {
                        return Err(RepositoryError::Duplicate);
                    }
                    Err(error) => return Err(RepositoryError::Database(error)),
                }
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                let mut store = store.lock().await;
                if store.balances.contains_key(&seller_id) {
                    return Err(RepositoryError::Duplicate);
                }
                store.balances.insert(
                    seller_id,
                    SellerBalanceRecord {
                        balance_id: Uuid::new_v4(),
                        member_id: seller_id,
                        balance: 0,
                    },
                );
            }
        }
        Ok(SellerBalanceThumbnailInfo { seller_id })
    }

    async fn get(&self, member_id: Uuid) -> Result<SellerBalanceInfo, RepositoryError> {
        let balance = match &self.backend {
            RepositoryBackend::Postgres(pool) => sqlx::query(
                "SELECT balance_id, member_id, settlement_balance \
                 FROM settlement_schema.seller_balance WHERE member_id = $1",
            )
            .bind(member_id)
            .fetch_optional(pool)
            .await?
            .map(|row| {
                Ok::<_, sqlx::Error>(SellerBalanceRecord {
                    balance_id: row.try_get("balance_id")?,
                    member_id: row.try_get("member_id")?,
                    balance: row.try_get("settlement_balance")?,
                })
            })
            .transpose()?,
            #[cfg(test)]
            RepositoryBackend::Test(store) => store.lock().await.balances.get(&member_id).cloned(),
        }
        .ok_or(RepositoryError::NotFound)?;

        Ok(SellerBalanceInfo {
            seller_balance_id: balance.balance_id,
            member_id: balance.member_id,
            balance: balance.balance,
        })
    }

    async fn history(
        &self,
        member_id: Uuid,
        page: usize,
        size: usize,
        sort: Option<HistorySort>,
    ) -> Result<(Vec<SellerBalanceHistoryInfo>, usize), RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let total: i64 = sqlx::query(
                    "SELECT COUNT(*) FROM settlement_schema.seller_balance_history \
                     WHERE member_id = $1",
                )
                .bind(member_id)
                .fetch_one(pool)
                .await?
                .try_get(0)?;
                let order = sort
                    .map(|sort| {
                        format!(
                            " ORDER BY {} {}",
                            sort.field.sql(),
                            if sort.descending { "DESC" } else { "ASC" }
                        )
                    })
                    .unwrap_or_default();
                let sql = format!(
                    "SELECT history_id, member_id, seller_payout_id, order_settlement_id, \
                     amount, status, created_at \
                     FROM settlement_schema.seller_balance_history \
                     WHERE member_id = $1{order} LIMIT $2 OFFSET $3"
                );
                let rows = sqlx::query(&sql)
                    .bind(member_id)
                    .bind(size as i64)
                    .bind(page.saturating_mul(size) as i64)
                    .fetch_all(pool)
                    .await?;
                let histories = rows
                    .iter()
                    .map(history_from_row)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((histories, total as usize))
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                let store = store.lock().await;
                let mut histories = store
                    .histories
                    .iter()
                    .filter(|history| history.member_id == member_id)
                    .cloned()
                    .collect::<Vec<_>>();
                if let Some(sort) = sort {
                    histories.sort_by(|left, right| {
                        let ordering = match sort.field {
                            SortField::CreatedAt => left.created_at.cmp(&right.created_at),
                            SortField::Amount => left.amount.cmp(&right.amount),
                            SortField::Status => left.status.cmp(&right.status),
                            SortField::HistoryId => left
                                .seller_balance_history_id
                                .cmp(&right.seller_balance_history_id),
                        };
                        if sort.descending {
                            ordering.reverse()
                        } else {
                            ordering
                        }
                    });
                }
                let total = histories.len();
                Ok((
                    histories
                        .into_iter()
                        .skip(page.saturating_mul(size))
                        .take(size)
                        .collect(),
                    total,
                ))
            }
        }
    }

    #[cfg(test)]
    async fn seed_history(&self, history: SellerBalanceHistoryInfo) {
        let RepositoryBackend::Test(store) = &self.backend else {
            panic!("test repository required");
        };
        store.lock().await.histories.push(history);
    }
}

#[cfg(test)]
impl Default for SellerBalanceRepository {
    fn default() -> Self {
        Self {
            backend: RepositoryBackend::Test(Arc::new(Mutex::new(TestStore::default()))),
        }
    }
}

pub fn router(repository: SellerBalanceRepository) -> Router {
    Router::new()
        .route("/internal/balances", post(create_balance))
        .route("/api/balances", get(get_balance))
        .route("/api/balances/history", get(get_history))
        .with_state(repository)
}

async fn create_balance(
    State(repository): State<SellerBalanceRepository>,
    payload: Result<Json<SellerBalanceRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(payload)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some(seller_id) = payload.seller_id else {
        return error(StatusCode::BAD_REQUEST, "SellerId 값이 없습니다.");
    };
    match repository.create(seller_id).await {
        Ok(info) => api_response(StatusCode::OK, Some(info), "seller balance 생성되었습니다."),
        Err(RepositoryError::Duplicate) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
        }
        Err(error) => repository_error(error),
    }
}

async fn get_balance(
    State(repository): State<SellerBalanceRepository>,
    headers: HeaderMap,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    match repository.get(member_id).await {
        Ok(info) => api_response(StatusCode::OK, Some(info), "조회되었습니다."),
        Err(RepositoryError::NotFound) => {
            error(StatusCode::NOT_FOUND, "판매자를 찾을 수 없습니다.")
        }
        Err(error) => repository_error(error),
    }
}

async fn get_history(
    State(repository): State<SellerBalanceRepository>,
    headers: HeaderMap,
    query: Result<Query<HistoryQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Query(query)) = query else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let page = query.page.unwrap_or(0);
    let size = query.size.unwrap_or(20);
    if size == 0
        || i64::try_from(size).is_err()
        || page
            .checked_mul(size)
            .and_then(|offset| i64::try_from(offset).ok())
            .is_none()
    {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    }
    let sort = match query.sort.as_deref().map(parse_sort).transpose() {
        Ok(sort) => sort,
        Err(()) => return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
    };
    match repository.history(member_id, page, size, sort).await {
        Ok((content, total_elements)) => {
            let number_of_elements = content.len();
            let total_pages = total_elements.div_ceil(size);
            api_response(
                StatusCode::OK,
                Some(SellerBalanceHistoryPage {
                    content,
                    total_pages,
                    total_elements,
                    first: page == 0,
                    last: page.saturating_add(1) >= total_pages,
                    size,
                    number_of_elements,
                }),
                "조회되었습니다.",
            )
        }
        Err(error) => repository_error(error),
    }
}

fn parse_sort(value: &str) -> Result<HistorySort, ()> {
    let mut parts = value.split(',');
    let field = match parts.next() {
        Some("createdAt") => SortField::CreatedAt,
        Some("amount") => SortField::Amount,
        Some("status") => SortField::Status,
        Some("sellerBalanceHistoryId") => SortField::HistoryId,
        _ => return Err(()),
    };
    let descending = match parts.next().unwrap_or("asc").to_ascii_lowercase().as_str() {
        "asc" => false,
        "desc" => true,
        _ => return Err(()),
    };
    if parts.next().is_some() {
        return Err(());
    }
    Ok(HistorySort { field, descending })
}

fn history_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SellerBalanceHistoryInfo, RepositoryError> {
    let created_at: OffsetDateTime = row.try_get("created_at")?;
    Ok(SellerBalanceHistoryInfo {
        seller_balance_history_id: row.try_get("history_id")?,
        member_id: row.try_get("member_id")?,
        seller_payout_id: row.try_get("seller_payout_id")?,
        order_settlement_id: row.try_get("order_settlement_id")?,
        amount: row.try_get("amount")?,
        status: row.try_get("status")?,
        created_at: created_at
            .format(&Rfc3339)
            .map_err(|_| RepositoryError::InvalidTimestamp)?,
    })
}

fn member_id(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let Some(value) = headers.get("x-member-id") else {
        return Err((StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE));
    };
    value
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or((StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))
}

fn repository_error(repository_error: RepositoryError) -> Response {
    if let RepositoryError::Database(error) = repository_error {
        eprintln!("seller balance repository error: {error}");
    }
    error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
}

fn error(status: StatusCode, message: &'static str) -> Response {
    api_response::<serde_json::Value>(status, None, message)
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
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    async fn send(app: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn create_is_unique_and_current_balance_is_member_scoped() {
        let member_id = Uuid::new_v4();
        let missing_member = Uuid::new_v4();
        let app = router(SellerBalanceRepository::default());
        let body = format!(r#"{{"sellerId":"{member_id}"}}"#);

        let (status, response) = send(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri("/internal/balances")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sellerId":null}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["message"], "SellerId 값이 없습니다.");

        let (status, response) = send(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri("/internal/balances")
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["data"]["sellerId"], member_id.to_string());

        let (status, response) = send(
            app.clone(),
            Request::builder()
                .uri("/api/balances")
                .header("x-member-id", member_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["data"]["memberId"], member_id.to_string());
        assert_eq!(response["data"]["balance"], 0);

        let (status, response) = send(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri("/internal/balances")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response["message"], INTERNAL_ERROR_MESSAGE);

        let (status, response) = send(
            app,
            Request::builder()
                .uri("/api/balances")
                .header("x-member-id", missing_member.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(response["message"], "판매자를 찾을 수 없습니다.");
    }

    #[tokio::test]
    async fn history_filters_member_then_sorts_and_pages() {
        let member_id = Uuid::new_v4();
        let other_member = Uuid::new_v4();
        let repository = SellerBalanceRepository::default();
        for (owner, amount, created_at) in [
            (member_id, 100, "2026-01-01T00:00:00Z"),
            (member_id, 300, "2026-01-03T00:00:00Z"),
            (member_id, 200, "2026-01-02T00:00:00Z"),
            (other_member, 999, "2026-01-04T00:00:00Z"),
        ] {
            repository
                .seed_history(SellerBalanceHistoryInfo {
                    seller_balance_history_id: Uuid::new_v4(),
                    member_id: owner,
                    seller_payout_id: None,
                    order_settlement_id: Some(Uuid::new_v4()),
                    amount,
                    status: "CREDIT".to_owned(),
                    created_at: created_at.to_owned(),
                })
                .await;
        }
        let app = router(repository);

        let (status, response) = send(
            app,
            Request::builder()
                .uri("/api/balances/history?page=1&size=1&sort=amount,desc")
                .header("x-member-id", member_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["data"]["totalElements"], 3);
        assert_eq!(response["data"]["totalPages"], 3);
        assert_eq!(response["data"]["numberOfElements"], 1);
        assert_eq!(
            response["data"]["content"][0]["memberId"],
            member_id.to_string()
        );
        assert_eq!(response["data"]["content"][0]["amount"], 200);
    }
}
