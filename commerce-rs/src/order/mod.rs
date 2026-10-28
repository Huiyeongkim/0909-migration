use axum::Router;

mod cancel;
mod confirm;
mod create;
mod lifecycle;
mod participants;
mod read;

pub fn router() -> Router {
    Router::new()
}
