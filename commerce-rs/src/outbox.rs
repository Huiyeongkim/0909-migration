use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone)]
struct OutboxState {
    pool: sqlx::PgPool,
}

#[derive(Serialize)]
struct ProcessResult {
    published: usize,
    failed: usize,
}

pub(crate) fn router(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/internal/outbox/process", post(process))
        .with_state(OutboxState { pool })
}

pub(crate) async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO order_schema.event_outbox(event_id,aggregate_type,aggregate_id,event_type,payload) VALUES($1,$2,$3,$4,$5) ON CONFLICT(aggregate_type,aggregate_id,event_type) DO NOTHING")
        .bind(Uuid::new_v4())
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(event_type)
        .bind(payload)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn process(State(state): State<OutboxState>) -> impl IntoResponse {
    match publish_pending(&state.pool, 100, publish_log).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => {
            eprintln!("outbox database error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn publish_pending(
    pool: &sqlx::PgPool,
    limit: i64,
    publisher: fn(&Value) -> Result<(), String>,
) -> Result<ProcessResult, sqlx::Error> {
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT event_id FROM order_schema.event_outbox WHERE published_at IS NULL ORDER BY occurred_at LIMIT $1")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let mut result = ProcessResult {
        published: 0,
        failed: 0,
    };
    for id in ids {
        let mut tx = pool.begin().await?;
        let row = sqlx::query("SELECT event_id,aggregate_type,aggregate_id,event_type,payload FROM order_schema.event_outbox WHERE event_id=$1 AND published_at IS NULL FOR UPDATE SKIP LOCKED")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else { continue };
        let log = serde_json::json!({
            "eventId": row.get::<Uuid, _>("event_id"),
            "aggregateType": row.get::<String, _>("aggregate_type"),
            "aggregateId": row.get::<Uuid, _>("aggregate_id"),
            "eventType": row.get::<String, _>("event_type"),
            "payload": row.get::<Value, _>("payload"),
        });
        match publisher(&log) {
            Ok(()) => {
                sqlx::query("UPDATE order_schema.event_outbox SET published_at=NOW(),attempts=attempts+1,last_error=NULL WHERE event_id=$1")
                    .bind(id).execute(&mut *tx).await?;
                result.published += 1;
            }
            Err(error) => {
                sqlx::query("UPDATE order_schema.event_outbox SET attempts=attempts+1,last_error=$2 WHERE event_id=$1")
                    .bind(id).bind(error).execute(&mut *tx).await?;
                result.failed += 1;
            }
        }
        tx.commit().await?;
    }
    Ok(result)
}

fn publish_log(event: &Value) -> Result<(), String> {
    eprintln!("{event}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn postgres_outbox_rolls_back_deduplicates_and_retries_delivery() {
        let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&database_url)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let rolled_back_member = Uuid::new_v4();
        let rolled_back_event = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO order_schema.point_account(member_id,balance) VALUES($1,100)")
            .bind(rolled_back_member)
            .execute(&mut *tx)
            .await
            .unwrap();
        enqueue(
            &mut tx,
            "POINT_TRANSACTION",
            rolled_back_event,
            "POINT_CHARGED",
            serde_json::json!({"amount": 100}),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        let state_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM order_schema.point_account WHERE member_id=$1)",
        )
        .bind(rolled_back_member)
        .fetch_one(&pool)
        .await
        .unwrap();
        let event_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM order_schema.event_outbox WHERE aggregate_id=$1)",
        )
        .bind(rolled_back_event)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!state_exists && !event_exists);

        let aggregate_id = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();
        for _ in 0..2 {
            enqueue(
                &mut tx,
                "ORDER",
                aggregate_id,
                "ORDER_CREATED",
                serde_json::json!({"orderId": aggregate_id}),
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM order_schema.event_outbox WHERE aggregate_id=$1 AND event_type='ORDER_CREATED'")
            .bind(aggregate_id).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);

        let failed = publish_pending(&pool, 100, |_| Err("publisher unavailable".to_owned()))
            .await
            .unwrap();
        assert!(failed.failed >= 1);
        let (published_at, attempts, last_error): (Option<time::OffsetDateTime>, i32, Option<String>) =
            sqlx::query_as("SELECT published_at,attempts,last_error FROM order_schema.event_outbox WHERE aggregate_id=$1 AND event_type='ORDER_CREATED'")
                .bind(aggregate_id).fetch_one(&pool).await.unwrap();
        assert!(published_at.is_none());
        assert_eq!(attempts, 1);
        assert_eq!(last_error.as_deref(), Some("publisher unavailable"));

        let published = publish_pending(&pool, 100, |_| Ok(())).await.unwrap();
        assert!(published.published >= 1);
        let (published_at, attempts): (Option<time::OffsetDateTime>, i32) = sqlx::query_as("SELECT published_at,attempts FROM order_schema.event_outbox WHERE aggregate_id=$1 AND event_type='ORDER_CREATED'")
            .bind(aggregate_id).fetch_one(&pool).await.unwrap();
        assert!(published_at.is_some());
        assert_eq!(attempts, 2);
    }
}
