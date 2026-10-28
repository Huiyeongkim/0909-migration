use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Json, Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;

const LOCAL_IMAGE_BASE_URL: &str = "http://127.0.0.1:8080/local-images/";
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;
const MAX_UPLOAD_GRANTS: usize = 10_000;
const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const INVALID_IMAGE_TYPE_MESSAGE: &str = "허용되지 않는 파일 형식입니다. (jpeg, png, webp만 가능)";

#[derive(Clone)]
pub struct ImageRepository {
    backend: ImageBackend,
    upload_grants: Arc<Mutex<HashMap<String, UploadGrant>>>,
}

#[derive(Clone)]
enum ImageBackend {
    Postgres(PgPool),
    #[cfg(test)]
    Test(Arc<Mutex<HashMap<String, StoredImage>>>),
}

#[derive(Clone)]
struct StoredImage {
    bytes: Vec<u8>,
    content_type: String,
}

struct UploadGrant {
    token: Uuid,
    content_type: String,
    expires_at: std::time::Instant,
}

#[derive(Clone, Copy)]
enum ImageKind {
    Product,
    GroupPurchase,
}

impl ImageKind {
    fn directory(self) -> &'static str {
        match self {
            Self::Product => "products",
            Self::GroupPurchase => "group-purchases",
        }
    }
}

#[derive(Debug)]
enum RepositoryError {
    InvalidKey,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RepositoryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresignedUrlRequest {
    file_name: Option<String>,
    content_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresignedUrlResponse {
    presigned_url: String,
    image_url: String,
    expiration_seconds: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageUploadResponse {
    image_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteImageQuery {
    image_url: String,
}

#[derive(Deserialize)]
struct UploadQuery {
    upload: Uuid,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

impl ImageRepository {
    pub fn postgres(pool: PgPool) -> Self {
        Self {
            backend: ImageBackend::Postgres(pool),
            upload_grants: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn image_url(kind: ImageKind, member_id: Uuid, file_name: Option<&str>) -> (String, String) {
        let key = format!(
            "{}/{}/{}_{}",
            kind.directory(),
            member_id,
            Uuid::new_v4(),
            file_name.unwrap_or("null")
        );
        let image_url = format!("{LOCAL_IMAGE_BASE_URL}{key}");
        (key, image_url)
    }

    async fn create_presigned_urls(
        &self,
        kind: ImageKind,
        member_id: Uuid,
        file_name: Option<&str>,
        content_type: &str,
    ) -> Result<(String, String), ()> {
        let (key, image_url) = Self::image_url(kind, member_id, file_name);
        let token = Uuid::new_v4();
        let mut grants = self.upload_grants.lock().await;
        let now = std::time::Instant::now();
        grants.retain(|_, grant| grant.expires_at > now);
        if grants.len() >= MAX_UPLOAD_GRANTS {
            return Err(());
        }
        grants.insert(
            key,
            UploadGrant {
                token,
                content_type: content_type.to_owned(),
                expires_at: now + Duration::from_secs(600),
            },
        );
        Ok((format!("{image_url}?upload={token}"), image_url))
    }

    async fn consume_upload_grant(
        &self,
        key: &str,
        token: Uuid,
        content_type: &str,
    ) -> Result<(), ()> {
        let mut grants = self.upload_grants.lock().await;
        let grant = grants.get(key).ok_or(())?;
        if grant.token != token
            || grant.content_type != content_type
            || grant.expires_at <= std::time::Instant::now()
        {
            return Err(());
        }
        grants.remove(key);
        Ok(())
    }

    async fn put(
        &self,
        image_url: &str,
        bytes: Vec<u8>,
        content_type: String,
    ) -> Result<(), RepositoryError> {
        let (key, owner_id, image_kind) = parse_image_url(image_url)?;
        match &self.backend {
            ImageBackend::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO product_schema.image_object \
                     (object_key, owner_id, image_kind, content_type, image_data, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, NOW(), NOW()) \
                     ON CONFLICT (object_key) DO UPDATE SET \
                     content_type = EXCLUDED.content_type, image_data = EXCLUDED.image_data, updated_at = NOW() \
                     WHERE product_schema.image_object.owner_id = EXCLUDED.owner_id",
                )
                .bind(key)
                .bind(owner_id)
                .bind(image_kind)
                .bind(content_type)
                .bind(bytes)
                .execute(pool)
                .await?;
            }
            #[cfg(test)]
            ImageBackend::Test(images) => {
                images.lock().await.insert(
                    key.to_owned(),
                    StoredImage {
                        bytes,
                        content_type,
                    },
                );
            }
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<StoredImage>, RepositoryError> {
        match &self.backend {
            ImageBackend::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT content_type, image_data \
                     FROM product_schema.image_object WHERE object_key = $1",
                )
                .bind(key)
                .fetch_optional(pool)
                .await?;
                row.map(|row| {
                    Ok(StoredImage {
                        content_type: row.try_get("content_type")?,
                        bytes: row.try_get("image_data")?,
                    })
                })
                .transpose()
            }
            #[cfg(test)]
            ImageBackend::Test(images) => Ok(images.lock().await.get(key).cloned()),
        }
    }

    async fn delete_owned(&self, image_url: &str, member_id: Uuid) -> Result<(), RepositoryError> {
        let (key, owner_id, _) = parse_image_url(image_url)?;
        if owner_id != member_id {
            return Err(RepositoryError::InvalidKey);
        }
        match &self.backend {
            ImageBackend::Postgres(pool) => {
                sqlx::query(
                    "DELETE FROM product_schema.image_object WHERE object_key = $1 AND owner_id = $2",
                )
                .bind(key)
                .bind(member_id)
                .execute(pool)
                .await?;
            }
            #[cfg(test)]
            ImageBackend::Test(images) => {
                images.lock().await.remove(key);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
impl Default for ImageRepository {
    fn default() -> Self {
        Self {
            backend: ImageBackend::Test(Arc::new(Mutex::new(HashMap::new()))),
            upload_grants: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub fn router(images: ImageRepository) -> Router {
    Router::new()
        .route(
            "/api/products/images/presigned-url",
            post(product_presigned_url),
        )
        .route(
            "/api/purchases/images/presigned-url",
            post(group_purchase_presigned_url),
        )
        .route(
            "/api/products/images",
            post(product_upload).delete(delete_image),
        )
        .route(
            "/api/purchases/images",
            post(group_purchase_upload).delete(delete_image),
        )
        .route(
            "/local-images/{*key}",
            get(get_local_image).put(put_local_image),
        )
        .layer(DefaultBodyLimit::max(MAX_IMAGE_SIZE + 1024 * 1024))
        .with_state(images)
}

async fn product_presigned_url(
    State(images): State<ImageRepository>,
    headers: HeaderMap,
    payload: Result<Json<PresignedUrlRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    presigned_url(images, headers, payload, ImageKind::Product).await
}

async fn group_purchase_presigned_url(
    State(images): State<ImageRepository>,
    headers: HeaderMap,
    payload: Result<Json<PresignedUrlRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    presigned_url(images, headers, payload, ImageKind::GroupPurchase).await
}

async fn presigned_url(
    images: ImageRepository,
    headers: HeaderMap,
    payload: Result<Json<PresignedUrlRequest>, axum::extract::rejection::JsonRejection>,
    kind: ImageKind,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Json(payload)) = payload else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    let Some(content_type) = payload.content_type.as_deref() else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            INVALID_IMAGE_TYPE_MESSAGE,
        );
    };
    if !valid_content_type(content_type) {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            INVALID_IMAGE_TYPE_MESSAGE,
        );
    }
    let (presigned_url, image_url) = match images
        .create_presigned_urls(kind, member_id, payload.file_name.as_deref(), content_type)
        .await
    {
        Ok(urls) => urls,
        Err(()) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "이미지 업로드 공간이 부족합니다.",
            )
        }
    };
    api_response(
        StatusCode::OK,
        Some(PresignedUrlResponse {
            presigned_url,
            image_url,
            expiration_seconds: 600,
        }),
        "Presigned URL 발급 성공",
    )
}

async fn product_upload(
    State(images): State<ImageRepository>,
    headers: HeaderMap,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Response {
    direct_upload(images, headers, multipart, ImageKind::Product).await
}

async fn group_purchase_upload(
    State(images): State<ImageRepository>,
    headers: HeaderMap,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Response {
    direct_upload(images, headers, multipart, ImageKind::GroupPurchase).await
}

async fn direct_upload(
    images: ImageRepository,
    headers: HeaderMap,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
    kind: ImageKind,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(mut multipart) = multipart else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Required part 'file' is not present.",
        );
    };
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("file") {
            continue;
        }
        let file_name = field.file_name().map(str::to_owned);
        let content_type = field.content_type().unwrap_or_default().to_owned();
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(_) => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "파일 업로드 중 오류가 발생했습니다.",
                )
            }
        };
        if let Err(message) = validate_image(&content_type, &bytes) {
            return error(StatusCode::INTERNAL_SERVER_ERROR, message);
        }
        let (_, image_url) = ImageRepository::image_url(kind, member_id, file_name.as_deref());
        return match images.put(&image_url, bytes, content_type).await {
            Ok(()) => api_response(
                StatusCode::OK,
                Some(ImageUploadResponse { image_url }),
                "이미지 업로드 성공",
            ),
            Err(error) => repository_error(error),
        };
    }
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Required part 'file' is not present.",
    )
}

async fn put_local_image(
    State(images): State<ImageRepository>,
    Path(key): Path<String>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if let Err(message) = validate_image(content_type, &body) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, message);
    }
    if images
        .consume_upload_grant(&key, query.upload, content_type)
        .await
        .is_err()
    {
        return error(
            StatusCode::FORBIDDEN,
            "유효하지 않거나 만료된 업로드 URL입니다.",
        );
    }
    let image_url = format!("{LOCAL_IMAGE_BASE_URL}{key}");
    match images
        .put(&image_url, body.to_vec(), content_type.to_owned())
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => repository_error(error),
    }
}

async fn get_local_image(
    State(images): State<ImageRepository>,
    Path(key): Path<String>,
) -> Response {
    match images.get(&key).await {
        Ok(Some(image)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, image.content_type)
            .body(Body::from(image.bytes))
            .unwrap(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => repository_error(error),
    }
}

async fn delete_image(
    State(images): State<ImageRepository>,
    headers: HeaderMap,
    query: Result<Query<DeleteImageQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return error(status, message),
    };
    let Ok(Query(query)) = query else {
        return error(StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE);
    };
    match images.delete_owned(&query.image_url, member_id).await {
        Ok(()) => api_response::<serde_json::Value>(StatusCode::OK, None, "이미지 삭제 성공"),
        Err(RepositoryError::InvalidKey) => error(
            StatusCode::FORBIDDEN,
            "본인이 업로드한 이미지만 삭제할 수 있습니다.",
        ),
        Err(error) => repository_error(error),
    }
}

fn parse_image_url(image_url: &str) -> Result<(&str, Uuid, &str), RepositoryError> {
    let key = image_url
        .strip_prefix(LOCAL_IMAGE_BASE_URL)
        .ok_or(RepositoryError::InvalidKey)?;
    let mut segments = key.splitn(3, '/');
    let kind = segments.next().ok_or(RepositoryError::InvalidKey)?;
    if !matches!(kind, "products" | "group-purchases") {
        return Err(RepositoryError::InvalidKey);
    }
    let owner_id = segments
        .next()
        .ok_or(RepositoryError::InvalidKey)?
        .parse()
        .map_err(|_| RepositoryError::InvalidKey)?;
    if segments.next().is_none_or(str::is_empty) {
        return Err(RepositoryError::InvalidKey);
    }
    Ok((key, owner_id, kind))
}

fn member_id(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let Some(value) = headers.get("x-member-id") else {
        return Err((StatusCode::UNAUTHORIZED, "로그인 정보가 없습니다."));
    };
    value
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or((StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))
}

fn valid_content_type(content_type: &str) -> bool {
    matches!(content_type, "image/jpeg" | "image/png" | "image/webp")
}

fn validate_image(content_type: &str, bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.is_empty() {
        return Err("파일이 비어있습니다.");
    }
    if bytes.len() > MAX_IMAGE_SIZE {
        return Err("파일 크기는 10MB를 초과할 수 없습니다.");
    }
    if !valid_content_type(content_type) {
        return Err(INVALID_IMAGE_TYPE_MESSAGE);
    }
    let matches_signature = match content_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        _ => false,
    };
    matches_signature
        .then_some(())
        .ok_or("파일 내용과 형식이 일치하지 않습니다.")
}

fn repository_error(repository_error: RepositoryError) -> Response {
    if let RepositoryError::Database(error) = repository_error {
        eprintln!("image repository error: {error}");
    }
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "파일 업로드 중 오류가 발생했습니다.",
    )
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
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;

    async fn send(app: Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, body.to_vec())
    }

    #[tokio::test]
    async fn product_presigned_upload_can_be_read_and_deleted() {
        let member_id = Uuid::new_v4();
        let app = router(ImageRepository::default());
        let (status, body) = send(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/products/images/presigned-url")
                .header("x-member-id", member_id.to_string())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"fileName":"item.png","contentType":"image/png"}"#,
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let presigned_url = response["data"]["presignedUrl"].as_str().unwrap();
        let image_url = response["data"]["imageUrl"].as_str().unwrap();
        assert!(image_url.contains(&format!("products/{member_id}/")));

        let png = b"\x89PNG\r\n\x1a\nproduct";
        let upload_path = presigned_url.strip_prefix("http://127.0.0.1:8080").unwrap();
        let key_path = upload_path.split('?').next().unwrap();
        let (status, _) = send(
            app.clone(),
            Request::builder()
                .method("PUT")
                .uri(format!("{key_path}?upload={}", Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "image/png")
                .body(Body::from(png.as_slice()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = send(
            app.clone(),
            Request::builder()
                .method("PUT")
                .uri(upload_path)
                .header(header::CONTENT_TYPE, "image/png")
                .body(Body::from(png.as_slice()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let image_path = image_url.strip_prefix("http://127.0.0.1:8080").unwrap();
        let (status, body) = send(
            app.clone(),
            Request::builder()
                .uri(image_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, png);

        for _ in 0..2 {
            let (status, _) = send(
                app.clone(),
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/products/images?imageUrl={image_url}"))
                    .header("x-member-id", member_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }
        let (status, _) = send(
            app,
            Request::builder()
                .uri(image_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn group_purchase_multipart_upload_can_be_read_and_checks_delete_owner() {
        let member_id = Uuid::new_v4();
        let app = router(ImageRepository::default());
        let boundary = "image-boundary";
        let mut multipart = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"group.webp\"\r\nContent-Type: image/webp\r\n\r\n"
        )
        .into_bytes();
        let webp = b"RIFF\x04\x00\x00\x00WEBPdata";
        multipart.extend_from_slice(webp);
        multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let (status, body) = send(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/purchases/images")
                .header("x-member-id", member_id.to_string())
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let image_url = response["data"]["imageUrl"].as_str().unwrap();
        assert!(image_url.contains(&format!("group-purchases/{member_id}/")));

        let (status, body) = send(
            app.clone(),
            Request::builder()
                .uri(image_url.strip_prefix("http://127.0.0.1:8080").unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, webp);

        let (status, body) = send(
            app,
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/purchases/images?imageUrl={image_url}"))
                .header("x-member-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["message"],
            "본인이 업로드한 이미지만 삭제할 수 있습니다."
        );
    }
}
