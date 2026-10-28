use crate::{
    cart::CartRepository, group_purchase::GroupPurchaseRepository, local_auth::AuthRepository,
    ProductRepository,
};
use axum::Router;
use std::sync::{Arc, Mutex};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

mod cancel;
mod confirm;
mod create;
mod lifecycle;
mod participants;
mod read;

#[derive(Clone, Default)]
pub(crate) struct OrderRepository {
    orders: Arc<Mutex<Vec<Order>>>,
}

#[derive(Clone)]
pub(super) struct OrderState {
    orders: OrderRepository,
    carts: CartRepository,
    group_purchases: GroupPurchaseRepository,
    auth: AuthRepository,
    products: Arc<dyn ProductRepository>,
}

#[derive(Clone)]
pub(super) struct Order {
    pub(super) order_id: Uuid,
    pub(super) order_number: String,
    pub(super) quantity: i32,
    pub(super) price: i64,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the local-payment migration unit")
    )]
    pub(super) paid_price: i64,
    pub(super) status: &'static str,
    pub(super) member_id: Uuid,
    pub(super) address: String,
    pub(super) address_detail: String,
    pub(super) postal_code: String,
    pub(super) receiver_name: String,
    pub(super) seller_id: Uuid,
    pub(super) group_purchase_id: Uuid,
    pub(super) idempotency_key: String,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by the order-lifecycle migration unit")
    )]
    pub(super) expired_at: String,
    pub(super) created_at: String,
    pub(super) updated_at: Option<String>,
    pub(super) deleted_at: Option<String>,
}

pub(super) struct NewOrder {
    pub(super) quantity: i32,
    pub(super) unit_price: i64,
    pub(super) member_id: Uuid,
    pub(super) address: String,
    pub(super) address_detail: String,
    pub(super) postal_code: String,
    pub(super) receiver_name: String,
    pub(super) seller_id: Uuid,
    pub(super) group_purchase_id: Uuid,
    pub(super) idempotency_key: String,
}

impl OrderRepository {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(super) fn transact<T>(&self, operation: impl FnOnce(&mut Vec<Order>) -> T) -> T {
        operation(&mut self.orders.lock().unwrap())
    }
}

impl Order {
    pub(super) fn create(new_order: NewOrder) -> Self {
        let created_at = OffsetDateTime::now_utc();
        Self {
            order_id: Uuid::new_v4(),
            order_number: order_number(created_at),
            quantity: new_order.quantity,
            price: new_order.unit_price,
            paid_price: new_order
                .unit_price
                .wrapping_mul(i64::from(new_order.quantity)),
            status: "PENDING",
            member_id: new_order.member_id,
            address: new_order.address,
            address_detail: new_order.address_detail,
            postal_code: new_order.postal_code,
            receiver_name: new_order.receiver_name,
            seller_id: new_order.seller_id,
            group_purchase_id: new_order.group_purchase_id,
            idempotency_key: new_order.idempotency_key,
            expired_at: (created_at + Duration::minutes(10))
                .format(&Rfc3339)
                .expect("RFC3339 formatting must succeed"),
            created_at: created_at
                .format(&Rfc3339)
                .expect("RFC3339 formatting must succeed"),
            updated_at: None,
            deleted_at: None,
        }
    }
}

fn order_number(now: OffsetDateTime) -> String {
    let date = format!(
        "{:02}{:02}{:02}",
        now.year().rem_euclid(100),
        now.month() as u8,
        now.day()
    );
    let random = order_number_suffix();
    format!("ORD-{date}-{random}")
}

fn order_number_suffix() -> String {
    const CHARACTERS: &[u8; 36] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let bytes = Uuid::new_v4().into_bytes();
    let mut random = u64::from_be_bytes([
        0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5],
    ]);
    let mut suffix = [b'A'; 8];
    for character in suffix.iter_mut().rev() {
        *character = CHARACTERS[(random % CHARACTERS.len() as u64) as usize];
        random /= CHARACTERS.len() as u64;
    }
    String::from_utf8(suffix.to_vec()).expect("order number alphabet must be valid UTF-8")
}

pub fn router(
    orders: OrderRepository,
    carts: CartRepository,
    group_purchases: GroupPurchaseRepository,
    auth: AuthRepository,
    products: Arc<dyn ProductRepository>,
) -> Router {
    let state = OrderState {
        orders,
        carts,
        group_purchases,
        auth,
        products,
    };
    Router::new()
        .merge(create::router())
        .merge(read::router())
        .merge(confirm::router())
        .merge(cancel::router())
        .merge(lifecycle::router())
        .merge(participants::router())
        .with_state(state)
}
