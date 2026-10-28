use argon2::{
    password_hash::{
        Error as PasswordHashError, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use axum::{
    extract::{rejection::JsonRejection, Json, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[cfg(test)]
use std::{collections::HashMap, sync::Arc};
#[cfg(test)]
use tokio::sync::Mutex;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const FAILED_LOGIN_MESSAGE: &str = "로그인 인증에 실패했습니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const SESSION_HOURS: i64 = 24;

#[derive(Clone)]
pub struct AuthRepository {
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
    accounts: HashMap<String, Account>,
    sessions: HashMap<Vec<u8>, TestSession>,
}

#[cfg(test)]
struct TestSession {
    member_id: Uuid,
    expires_at: OffsetDateTime,
    revoked: bool,
}

#[derive(Clone)]
struct Account {
    member_id: Uuid,
    username: String,
    password_hash: String,
}

pub(crate) enum RepositoryError {
    DuplicateUsername,
    Database(sqlx::Error),
}

pub(crate) enum ResolveError {
    NoSession,
    Repository(Box<RepositoryError>),
}

impl From<sqlx::Error> for RepositoryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemberInfo {
    member_id: Uuid,
    username: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginInfo {
    access_token: String,
    member_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInfo {
    member_id: Uuid,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

impl AuthRepository {
    pub fn postgres(pool: PgPool) -> Self {
        Self {
            backend: RepositoryBackend::Postgres(pool),
        }
    }

    pub(crate) async fn member_exists(&self, member_id: Uuid) -> Result<bool, ()> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM auth_schema.local_account WHERE member_id = $1)",
            )
            .bind(member_id)
            .fetch_one(pool)
            .await
            .map_err(|error| {
                eprintln!("local auth database error: {error}");
            }),
            #[cfg(test)]
            RepositoryBackend::Test(store) => Ok(store
                .lock()
                .await
                .accounts
                .values()
                .any(|account| account.member_id == member_id)),
        }
    }

    async fn create_account(
        &self,
        username: String,
        password_hash: String,
    ) -> Result<Account, RepositoryError> {
        let account = Account {
            member_id: Uuid::new_v4(),
            username,
            password_hash,
        };
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let result = sqlx::query(
                    "INSERT INTO auth_schema.local_account \
                     (member_id, username, password_hash, created_at) \
                     VALUES ($1, $2, $3, NOW())",
                )
                .bind(account.member_id)
                .bind(&account.username)
                .bind(&account.password_hash)
                .execute(pool)
                .await;
                match result {
                    Ok(_) => {}
                    Err(sqlx::Error::Database(error))
                        if error.code().as_deref() == Some("23505") =>
                    {
                        return Err(RepositoryError::DuplicateUsername);
                    }
                    Err(error) => return Err(RepositoryError::Database(error)),
                }
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                let mut store = store.lock().await;
                if store.accounts.contains_key(&account.username) {
                    return Err(RepositoryError::DuplicateUsername);
                }
                store
                    .accounts
                    .insert(account.username.clone(), account.clone());
            }
        }
        Ok(account)
    }

    async fn find_account(&self, username: &str) -> Result<Option<Account>, RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT member_id, username, password_hash \
                     FROM auth_schema.local_account WHERE username = $1",
                )
                .bind(username)
                .fetch_optional(pool)
                .await?;
                row.map(|row| {
                    Ok(Account {
                        member_id: row.try_get("member_id")?,
                        username: row.try_get("username")?,
                        password_hash: row.try_get("password_hash")?,
                    })
                })
                .transpose()
                .map_err(RepositoryError::Database)
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                Ok(store.lock().await.accounts.get(username).cloned())
            }
        }
    }

    async fn create_session(
        &self,
        member_id: Uuid,
        token_hash: Vec<u8>,
        expires_at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO auth_schema.local_session \
                     (token_hash, member_id, created_at, expires_at) \
                     VALUES ($1, $2, NOW(), $3)",
                )
                .bind(token_hash)
                .bind(member_id)
                .bind(expires_at)
                .execute(pool)
                .await?;
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                store.lock().await.sessions.insert(
                    token_hash,
                    TestSession {
                        member_id,
                        expires_at,
                        revoked: false,
                    },
                );
            }
        }
        Ok(())
    }

    async fn resolve_session(&self, token_hash: &[u8]) -> Result<Option<Uuid>, RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT member_id FROM auth_schema.local_session \
                     WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > NOW()",
                )
                .bind(token_hash)
                .fetch_optional(pool)
                .await?;
                row.map(|row| row.try_get("member_id"))
                    .transpose()
                    .map_err(RepositoryError::Database)
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => Ok(store
                .lock()
                .await
                .sessions
                .get(token_hash)
                .filter(|session| {
                    !session.revoked && session.expires_at > OffsetDateTime::now_utc()
                })
                .map(|session| session.member_id)),
        }
    }

    async fn revoke_session(&self, token_hash: &[u8]) -> Result<bool, RepositoryError> {
        match &self.backend {
            RepositoryBackend::Postgres(pool) => {
                let result = sqlx::query(
                    "UPDATE auth_schema.local_session SET revoked_at = NOW() \
                     WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > NOW()",
                )
                .bind(token_hash)
                .execute(pool)
                .await?;
                Ok(result.rows_affected() == 1)
            }
            #[cfg(test)]
            RepositoryBackend::Test(store) => {
                let mut store = store.lock().await;
                let Some(session) = store.sessions.get_mut(token_hash) else {
                    return Ok(false);
                };
                if session.revoked || session.expires_at <= OffsetDateTime::now_utc() {
                    return Ok(false);
                }
                session.revoked = true;
                Ok(true)
            }
        }
    }

    #[cfg(test)]
    async fn stored_password_hash(&self, username: &str) -> Option<String> {
        let RepositoryBackend::Test(store) = &self.backend else {
            panic!("test repository required");
        };
        store
            .lock()
            .await
            .accounts
            .get(username)
            .map(|account| account.password_hash.clone())
    }

    #[cfg(test)]
    pub(crate) async fn insert_test_member(&self, member_id: Uuid) {
        let RepositoryBackend::Test(store) = &self.backend else {
            panic!("test repository required");
        };
        store.lock().await.accounts.insert(
            member_id.to_string(),
            Account {
                member_id,
                username: member_id.to_string(),
                password_hash: "test-only".to_owned(),
            },
        );
    }

    #[cfg(test)]
    pub(crate) async fn insert_test_session(&self, member_id: Uuid, token: Uuid) {
        let RepositoryBackend::Test(store) = &self.backend else {
            panic!("test repository required");
        };
        store.lock().await.sessions.insert(
            token_hash(&token.to_string()),
            TestSession {
                member_id,
                expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
                revoked: false,
            },
        );
    }
}

#[cfg(test)]
impl Default for AuthRepository {
    fn default() -> Self {
        Self {
            backend: RepositoryBackend::Test(Arc::new(Mutex::new(TestStore::default()))),
        }
    }
}

pub fn router(repository: AuthRepository) -> Router {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .with_state(repository)
}

async fn register(
    State(repository): State<AuthRepository>,
    payload: Result<Json<Credentials>, JsonRejection>,
) -> Response {
    let Ok(Json(credentials)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some((username, password)) = valid_credentials(credentials) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let password_hash = match hash_password(password).await {
        Ok(password_hash) => password_hash,
        Err(()) => return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE),
    };
    match repository.create_account(username, password_hash).await {
        Ok(account) => api_response(
            StatusCode::CREATED,
            Some(MemberInfo {
                member_id: account.member_id,
                username: account.username,
            }),
            "회원가입이 완료되었습니다.",
        ),
        Err(RepositoryError::DuplicateUsername) => {
            error(StatusCode::CONFLICT, "이미 사용중인 아이디입니다.")
        }
        Err(error) => repository_error(error),
    }
}

async fn login(
    State(repository): State<AuthRepository>,
    payload: Result<Json<Credentials>, JsonRejection>,
) -> Response {
    let Ok(Json(credentials)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some((username, password)) = valid_credentials(credentials) else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let account = match repository.find_account(&username).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            let _ = hash_password(password).await;
            return error(StatusCode::UNAUTHORIZED, FAILED_LOGIN_MESSAGE);
        }
        Err(error) => return repository_error(error),
    };
    match password_matches(password, account.password_hash).await {
        Ok(true) => {}
        Ok(false) => return error(StatusCode::UNAUTHORIZED, FAILED_LOGIN_MESSAGE),
        Err(()) => return error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE),
    }

    let token = Uuid::new_v4().to_string();
    if let Err(error) = repository
        .create_session(
            account.member_id,
            token_hash(&token),
            OffsetDateTime::now_utc() + Duration::hours(SESSION_HOURS),
        )
        .await
    {
        return repository_error(error);
    }
    api_response(
        StatusCode::OK,
        Some(LoginInfo {
            access_token: token,
            member_id: account.member_id,
        }),
        "로그인을 성공하였습니다.",
    )
}

async fn session(State(repository): State<AuthRepository>, headers: HeaderMap) -> Response {
    match resolve_member_id(&repository, &headers).await {
        Ok(member_id) => api_response(
            StatusCode::OK,
            Some(SessionInfo { member_id }),
            "인증되었습니다.",
        ),
        Err(ResolveError::NoSession) => error(StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE),
        Err(ResolveError::Repository(failure)) => repository_error(*failure),
    }
}

async fn logout(State(repository): State<AuthRepository>, headers: HeaderMap) -> Response {
    let token = match bearer_token(&headers) {
        Some(token) => token,
        None => return error(StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE),
    };
    match repository.revoke_session(&token_hash(&token)).await {
        Ok(true) => api_response::<()>(StatusCode::OK, None, "로그아웃이 완료되었습니다."),
        Ok(false) => error(StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE),
        Err(error) => repository_error(error),
    }
}

pub(crate) async fn resolve_member_id(
    repository: &AuthRepository,
    headers: &HeaderMap,
) -> Result<Uuid, ResolveError> {
    let token = bearer_token(headers).ok_or(ResolveError::NoSession)?;
    repository
        .resolve_session(&token_hash(&token))
        .await
        .map_err(|error| ResolveError::Repository(Box::new(error)))?
        .ok_or(ResolveError::NoSession)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(|value| value.to_string())
}

fn valid_credentials(credentials: Credentials) -> Option<(String, String)> {
    let username = credentials.username.trim();
    if !(3..=64).contains(&username.len()) || !(8..=256).contains(&credentials.password.len()) {
        return None;
    }
    Some((username.to_owned(), credentials.password))
}

async fn hash_password(password: String) -> Result<String, ()> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_| ())?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| ())
    })
    .await
    .map_err(|_| ())?
}

async fn password_matches(password: String, encoded: String) -> Result<bool, ()> {
    tokio::task::spawn_blocking(move || {
        let hash = PasswordHash::new(&encoded).map_err(|_| ())?;
        match Argon2::default().verify_password(password.as_bytes(), &hash) {
            Ok(()) => Ok(true),
            Err(PasswordHashError::Password) => Ok(false),
            Err(_) => Err(()),
        }
    })
    .await
    .map_err(|_| ())?
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn repository_error(repository_error: RepositoryError) -> Response {
    match repository_error {
        RepositoryError::DuplicateUsername => {
            error(StatusCode::CONFLICT, "이미 사용중인 아이디입니다.")
        }
        RepositoryError::Database(_database_error) => {
            eprintln!("local auth database error");
            error(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
        }
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
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn send(repository: AuthRepository, request: Request<Body>) -> (StatusCode, Value) {
        let response = router(repository).oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn json_request(method: &str, path: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn register(repository: AuthRepository, username: &str, password: &str) -> Value {
        let (status, body) = send(
            repository,
            json_request(
                "POST",
                "/auth/register",
                json!({"username": username, "password": password}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        body
    }

    async fn login(
        repository: AuthRepository,
        username: &str,
        password: &str,
    ) -> (StatusCode, Value) {
        send(
            repository,
            json_request(
                "POST",
                "/auth/login",
                json!({"username": username, "password": password}),
            ),
        )
        .await
    }

    fn authorized(method: &str, path: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn registration_hashes_password_and_rejects_duplicate_username() {
        let repository = AuthRepository::default();
        let created = register(repository.clone(), "rookie", "password123!").await;
        let member_id = created["data"]["memberId"].as_str().unwrap();
        assert!(Uuid::parse_str(member_id).is_ok());

        let stored = repository.stored_password_hash("rookie").await.unwrap();
        assert_ne!(stored, "password123!");
        assert!(stored.starts_with("$argon2id$"));

        let (status, body) = send(
            repository,
            json_request(
                "POST",
                "/auth/register",
                json!({"username": "rookie", "password": "password123!"}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["message"], "이미 사용중인 아이디입니다.");
    }

    #[tokio::test]
    async fn login_uses_one_failure_for_unknown_username_and_wrong_password() {
        let repository = AuthRepository::default();
        register(repository.clone(), "rookie", "password123!").await;

        for (username, password) in [("rookie", "wrong-pass"), ("unknown", "wrong-pass")] {
            let (status, body) = login(repository.clone(), username, password).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(body["message"], FAILED_LOGIN_MESSAGE);
        }
    }

    #[tokio::test]
    async fn bearer_session_resolves_member_then_logout_revokes_it() {
        let repository = AuthRepository::default();
        let created = register(repository.clone(), "rookie", "password123!").await;
        let member_id = created["data"]["memberId"].as_str().unwrap().to_owned();
        let (status, logged_in) = login(repository.clone(), "rookie", "password123!").await;
        assert_eq!(status, StatusCode::OK);
        let token = logged_in["data"]["accessToken"].as_str().unwrap();
        assert_eq!(logged_in["data"]["memberId"], member_id);

        let (status, session) = send(
            repository.clone(),
            authorized("GET", "/auth/session", token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(session["data"]["memberId"], member_id);

        let (status, _) = send(
            repository.clone(),
            authorized("POST", "/auth/logout", token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = send(repository, authorized("GET", "/auth/session", token)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["message"], NO_LOGIN_INFO_MESSAGE);
    }

    #[tokio::test]
    async fn validates_credentials_and_bearer_shape() {
        let repository = AuthRepository::default();
        let (status, body) = send(
            repository.clone(),
            json_request(
                "POST",
                "/auth/register",
                json!({"username": "ab", "password": "short"}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["message"], INVALID_PARAMETER_MESSAGE);

        let (status, body) = send(
            repository,
            Request::builder()
                .uri("/auth/session")
                .header(header::AUTHORIZATION, "Bearer not-a-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["message"], NO_LOGIN_INFO_MESSAGE);
    }
}
