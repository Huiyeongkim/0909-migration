use super::{OrderRepository, OrderState, OrderStore, PointLedgerEntry};
use crate::{
    group_purchase::participate::{
        OrderCancellation, OrderCancellationError, OrderCancellationTransactionError,
    },
    local_auth::ResolveError,
};
use axum::{
    extract::{rejection::JsonRejection, Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const ORDER_NOT_FOUND_MESSAGE: &str = "주문을 찾을 수 없습니다.";
const ORDER_ACCESS_DENIED_MESSAGE: &str = "본인의 주문만 결제할 수 있습니다.";
const INVALID_ORDER_STATUS_MESSAGE: &str = "결제 대기 상태의 주문만 결제할 수 있습니다.";
const EXPIRED_ORDER_MESSAGE: &str = "결제 가능 시간이 만료되었습니다.";
const INSUFFICIENT_POINTS_MESSAGE: &str = "포인트 잔액이 부족합니다.";
const DUPLICATE_TRANSACTION_MESSAGE: &str = "이미 다른 요청에 사용된 거래 ID입니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const CHARGED_MESSAGE: &str = "포인트가 충전되었습니다.";
const PAID_MESSAGE: &str = "포인트 결제가 완료되었습니다.";
const EXPIRED_MESSAGE: &str = "만료된 주문을 처리했습니다.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChargeRequest {
    amount: i64,
    transaction_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentRequest {
    transaction_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChargeInfo {
    transaction_id: Uuid,
    amount: i64,
    balance: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentInfo {
    order_id: Uuid,
    transaction_id: Uuid,
    amount: i64,
    balance: i64,
    status: &'static str,
    payment_method: &'static str,
    paid_at: String,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: u16,
    data: Option<T>,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalPaymentLog {
    event: &'static str,
    transaction_id: Uuid,
    order_id: Uuid,
    amount: i64,
}

#[derive(Debug, PartialEq, Eq)]
enum LifecycleError {
    InvalidAmount,
    BalanceOverflow,
    OrderNotFound,
    Forbidden,
    InvalidStatus,
    Expired,
    InsufficientPoints,
    DuplicateTransaction,
    PurchaseNotFound,
    Quantity,
    Internal,
}

enum AuthenticationError {
    NoSession,
    Internal,
}

enum ExpirationCommitError {
    NoLongerEligible,
}

pub(super) fn router() -> Router<OrderState> {
    Router::new()
        .route("/api/points/charge", post(charge_points))
        .route("/api/orders/{order_id}/point-payment", post(pay_order))
        .route("/api/orders/expire", post(expire_orders))
}

async fn charge_points(
    State(state): State<OrderState>,
    headers: HeaderMap,
    request: Result<Json<ChargeRequest>, JsonRejection>,
) -> Response {
    let member_id = match authenticated_member(&state, &headers).await {
        Ok(member_id) => member_id,
        Err(error) => return authentication_error_response(error),
    };
    let Ok(Json(request)) = request else {
        return api_response::<()>(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };
    match state
        .orders
        .charge(member_id, request.transaction_id, request.amount)
    {
        Ok(info) => api_response(StatusCode::OK, Some(info), CHARGED_MESSAGE),
        Err(error) => lifecycle_error_response(error),
    }
}

async fn pay_order(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    request: Result<Json<PaymentRequest>, JsonRejection>,
) -> Response {
    let member_id = match authenticated_member(&state, &headers).await {
        Ok(member_id) => member_id,
        Err(error) => return authentication_error_response(error),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return api_response::<()>(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };
    let Ok(Json(request)) = request else {
        return api_response::<()>(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE);
    };
    match state.orders.pay(
        member_id,
        order_id,
        request.transaction_id,
        OffsetDateTime::now_utc(),
    ) {
        Ok(info) => {
            log_payment(&info);
            api_response(StatusCode::OK, Some(info), PAID_MESSAGE)
        }
        Err(error) => lifecycle_error_response(error),
    }
}

async fn expire_orders(State(state): State<OrderState>) -> Response {
    match state
        .orders
        .expire_pending(&state.group_purchases, OffsetDateTime::now_utc())
    {
        Ok(()) => api_response::<()>(StatusCode::OK, None, EXPIRED_MESSAGE),
        Err(error) => lifecycle_error_response(error),
    }
}

async fn authenticated_member(
    state: &OrderState,
    headers: &HeaderMap,
) -> Result<Uuid, AuthenticationError> {
    match crate::local_auth::resolve_member_id(&state.auth, headers).await {
        Ok(member_id) => Ok(member_id),
        Err(ResolveError::NoSession) => Err(AuthenticationError::NoSession),
        Err(ResolveError::Repository(_)) => Err(AuthenticationError::Internal),
    }
}

fn authentication_error_response(error: AuthenticationError) -> Response {
    match error {
        AuthenticationError::NoSession => {
            api_response::<()>(StatusCode::UNAUTHORIZED, None, NO_LOGIN_INFO_MESSAGE)
        }
        AuthenticationError::Internal => api_response::<()>(
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            INTERNAL_ERROR_MESSAGE,
        ),
    }
}

impl OrderRepository {
    fn charge(
        &self,
        member_id: Uuid,
        transaction_id: Uuid,
        amount: i64,
    ) -> Result<ChargeInfo, LifecycleError> {
        if amount <= 0 {
            return Err(LifecycleError::InvalidAmount);
        }
        self.transact_store(|store| {
            if let Some(entry) = store.point_ledger.iter().find(|entry| {
                entry.member_id == member_id && entry.transaction_id == transaction_id
            }) {
                return if entry.kind == "CHARGE" && entry.amount == amount {
                    Ok(ChargeInfo {
                        transaction_id,
                        amount: entry.amount,
                        balance: entry.balance_after,
                    })
                } else {
                    Err(LifecycleError::DuplicateTransaction)
                };
            }
            let balance = store.point_balances.entry(member_id).or_default();
            let next_balance = balance
                .checked_add(amount)
                .ok_or(LifecycleError::BalanceOverflow)?;
            *balance = next_balance;
            store.point_ledger.push(PointLedgerEntry {
                transaction_id,
                member_id,
                order_id: Uuid::nil(),
                amount,
                balance_after: next_balance,
                kind: "CHARGE",
                created_at: now_string(OffsetDateTime::now_utc()),
            });
            Ok(ChargeInfo {
                transaction_id,
                amount,
                balance: next_balance,
            })
        })
    }

    fn pay(
        &self,
        member_id: Uuid,
        order_id: Uuid,
        transaction_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<PaymentInfo, LifecycleError> {
        self.transact_store(|store| {
            if let Some(entry) = store.point_ledger.iter().find(|entry| {
                entry.member_id == member_id && entry.transaction_id == transaction_id
            }) {
                if entry.kind != "USE" || entry.order_id != order_id {
                    return Err(LifecycleError::DuplicateTransaction);
                }
                let order = store
                    .orders
                    .iter()
                    .find(|order| order.order_id == order_id && order.deleted_at.is_none())
                    .ok_or(LifecycleError::OrderNotFound)?;
                return Ok(PaymentInfo {
                    order_id,
                    transaction_id,
                    amount: entry.amount,
                    balance: entry.balance_after,
                    status: "PAYMENT_COMPLETED",
                    payment_method: "POINT",
                    paid_at: order.paid_at.clone().ok_or(LifecycleError::Internal)?,
                });
            }

            let order_index = store
                .orders
                .iter()
                .position(|order| order.order_id == order_id && order.deleted_at.is_none())
                .ok_or(LifecycleError::OrderNotFound)?;
            let order = &store.orders[order_index];
            if order.member_id != member_id {
                return Err(LifecycleError::Forbidden);
            }
            if order.status != "PENDING" {
                return Err(LifecycleError::InvalidStatus);
            }
            if OffsetDateTime::parse(&order.expired_at, &Rfc3339)
                .map_or(true, |expired_at| expired_at <= now)
            {
                return Err(LifecycleError::Expired);
            }
            let amount = order.paid_price;
            let balance = store.point_balances.entry(member_id).or_default();
            let next_balance = balance
                .checked_sub(amount)
                .filter(|balance| *balance >= 0)
                .ok_or(LifecycleError::InsufficientPoints)?;
            let paid_at = now_string(now);
            *balance = next_balance;
            let order = &mut store.orders[order_index];
            order.status = "PAYMENT_COMPLETED";
            order.payment_method = Some("POINT");
            order.paid_at = Some(paid_at.clone());
            order.updated_at = Some(paid_at.clone());
            store.point_ledger.push(PointLedgerEntry {
                transaction_id,
                member_id,
                order_id,
                amount,
                balance_after: next_balance,
                kind: "USE",
                created_at: paid_at.clone(),
            });
            Ok(PaymentInfo {
                order_id,
                transaction_id,
                amount,
                balance: next_balance,
                status: "PAYMENT_COMPLETED",
                payment_method: "POINT",
                paid_at,
            })
        })
    }

    fn expire_pending(
        &self,
        purchases: &crate::group_purchase::GroupPurchaseRepository,
        now: OffsetDateTime,
    ) -> Result<(), LifecycleError> {
        self.expire_pending_with_hook(purchases, now, || {})
    }

    fn expire_pending_with_hook(
        &self,
        purchases: &crate::group_purchase::GroupPurchaseRepository,
        now: OffsetDateTime,
        before_purchase_lock: impl Fn(),
    ) -> Result<(), LifecycleError> {
        let targets = self.transact_store(|store| {
            store
                .orders
                .iter()
                .filter(|order| {
                    order.deleted_at.is_none()
                        && order.status == "PENDING"
                        && OffsetDateTime::parse(&order.expired_at, &Rfc3339)
                            .is_ok_and(|expired_at| expired_at < now)
                })
                .map(|order| (order.order_id, order.group_purchase_id, order.quantity))
                .collect::<Vec<_>>()
        });

        for (order_id, group_purchase_id, quantity) in targets {
            before_purchase_lock();
            let request = OrderCancellation {
                group_purchase_id,
                quantity,
            };
            match purchases.with_order_cancellations(&[request], |_| {
                self.transact_store(|store| expire_in_store(store, order_id, now))
            }) {
                Ok(()) => {}
                Err(OrderCancellationTransactionError::Commit(
                    ExpirationCommitError::NoLongerEligible,
                )) => {}
                Err(OrderCancellationTransactionError::Cancellation(
                    OrderCancellationError::NotFound,
                )) => return Err(LifecycleError::PurchaseNotFound),
                Err(OrderCancellationTransactionError::Cancellation(
                    OrderCancellationError::Quantity,
                )) => return Err(LifecycleError::Quantity),
            }
        }
        Ok(())
    }
}

fn expire_in_store(
    store: &mut OrderStore,
    order_id: Uuid,
    now: OffsetDateTime,
) -> Result<(), ExpirationCommitError> {
    let Some(order) = store
        .orders
        .iter_mut()
        .find(|order| order.order_id == order_id && order.deleted_at.is_none())
    else {
        return Err(ExpirationCommitError::NoLongerEligible);
    };
    if order.status != "PENDING"
        || !OffsetDateTime::parse(&order.expired_at, &Rfc3339)
            .is_ok_and(|expired_at| expired_at < now)
    {
        return Err(ExpirationCommitError::NoLongerEligible);
    }
    order.status = "EXPIRED";
    order.updated_at = Some(now_string(now));
    Ok(())
}

fn lifecycle_error_response(error: LifecycleError) -> Response {
    let (status, message) = match error {
        LifecycleError::InvalidAmount => (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE),
        LifecycleError::BalanceOverflow | LifecycleError::Internal => {
            (StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
        }
        LifecycleError::OrderNotFound => (StatusCode::NOT_FOUND, ORDER_NOT_FOUND_MESSAGE),
        LifecycleError::Forbidden => (StatusCode::FORBIDDEN, ORDER_ACCESS_DENIED_MESSAGE),
        LifecycleError::InvalidStatus => (StatusCode::BAD_REQUEST, INVALID_ORDER_STATUS_MESSAGE),
        LifecycleError::Expired => (StatusCode::BAD_REQUEST, EXPIRED_ORDER_MESSAGE),
        LifecycleError::InsufficientPoints => {
            (StatusCode::BAD_REQUEST, INSUFFICIENT_POINTS_MESSAGE)
        }
        LifecycleError::DuplicateTransaction => {
            (StatusCode::CONFLICT, DUPLICATE_TRANSACTION_MESSAGE)
        }
        LifecycleError::PurchaseNotFound | LifecycleError::Quantity => {
            (StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE)
        }
    };
    api_response::<()>(status, None, message)
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

fn log_payment(info: &PaymentInfo) {
    let log = LocalPaymentLog {
        event: "POINT_PAYMENT_COMPLETED",
        transaction_id: info.transaction_id,
        order_id: info.order_id,
        amount: info.amount,
    };
    if let Ok(log) = serde_json::to_string(&log) {
        eprintln!("{log}");
    }
}

fn now_string(now: OffsetDateTime) -> String {
    now.format(&Rfc3339)
        .expect("RFC3339 formatting must succeed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository,
        group_purchase::GroupPurchaseRepository,
        local_auth::AuthRepository,
        order::{NewOrder, Order},
        LocalProductRepository,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request},
    };
    use serde_json::{json, Value};
    use std::{
        sync::{Arc, Barrier},
        thread,
    };
    use time::Duration;
    use tower::ServiceExt;

    fn order(
        member_id: Uuid,
        purchase_id: Uuid,
        status: &'static str,
        now: OffsetDateTime,
    ) -> Order {
        let mut order = Order::create(NewOrder {
            quantity: 2,
            unit_price: 8_000,
            member_id,
            address: "서울시".to_owned(),
            address_detail: "101호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "구매자".to_owned(),
            seller_id: Uuid::new_v4(),
            group_purchase_id: purchase_id,
            idempotency_key: Uuid::new_v4().to_string(),
        });
        order.status = status;
        order.expired_at = now_string(now + Duration::minutes(10));
        order
    }

    fn state(
        orders: impl IntoIterator<Item = Order>,
        purchases: GroupPurchaseRepository,
        auth: AuthRepository,
    ) -> OrderState {
        let repository = OrderRepository::empty();
        repository.transact(|stored| stored.extend(orders));
        OrderState {
            orders: repository,
            carts: CartRepository::empty(),
            group_purchases: purchases,
            auth,
            products: Arc::new(LocalProductRepository::new([])),
        }
    }

    async fn send(state: OrderState, request: Request<Body>) -> (StatusCode, Value) {
        let response = router().with_state(state).oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn authorized_json(path: &str, token: Uuid, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[test]
    fn charge_is_member_scoped_idempotent_and_rejects_overflow() {
        let repository = OrderRepository::empty();
        let member = Uuid::new_v4();
        let other = Uuid::new_v4();
        let transaction = Uuid::new_v4();

        let first = repository.charge(member, transaction, 10_000).unwrap();
        let replay = repository.charge(member, transaction, 10_000).unwrap();
        let other_result = repository.charge(other, transaction, 3_000).unwrap();

        assert_eq!(first, replay);
        assert_eq!(first.balance, 10_000);
        assert_eq!(other_result.balance, 3_000);
        repository.transact_store(|store| {
            assert_eq!(store.point_ledger.len(), 2);
            store.point_balances.insert(member, i64::MAX);
        });
        assert_eq!(
            repository.charge(member, Uuid::new_v4(), 1),
            Err(LifecycleError::BalanceOverflow)
        );
        repository.transact_store(|store| {
            assert_eq!(store.point_balances[&member], i64::MAX);
            assert_eq!(store.point_ledger.len(), 2);
        });
    }

    #[test]
    fn payment_debits_snapshot_price_and_replays_without_second_debit() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let order = order(member, purchase, "PENDING", now);
        let order_id = order.order_id;
        let repository = OrderRepository::empty();
        repository.transact_store(|store| {
            store.orders.push(order);
            store.point_balances.insert(member, 20_000);
        });
        let transaction = Uuid::new_v4();

        let first = repository.pay(member, order_id, transaction, now).unwrap();
        let replay = repository
            .pay(member, order_id, transaction, now + Duration::seconds(1))
            .unwrap();

        assert_eq!(first, replay);
        assert_eq!(first.amount, 16_000);
        assert_eq!(first.balance, 4_000);
        repository.transact_store(|store| {
            assert_eq!(store.point_balances[&member], 4_000);
            assert_eq!(store.point_ledger.len(), 1);
            assert_eq!(store.orders[0].status, "PAYMENT_COMPLETED");
            assert_eq!(store.orders[0].payment_method, Some("POINT"));
        });
    }

    #[test]
    fn payment_rejects_owner_balance_expiry_and_cross_order_reuse_failures() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let other = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let pending = order(member, purchase, "PENDING", now);
        let pending_id = pending.order_id;
        let mut expired = order(member, purchase, "PENDING", now);
        expired.expired_at = now_string(now - Duration::seconds(1));
        let expired_id = expired.order_id;
        let repository = OrderRepository::empty();
        repository.transact_store(|store| {
            store.orders.extend([pending, expired]);
            store.point_balances.insert(member, 10_000);
        });

        assert_eq!(
            repository.pay(other, pending_id, Uuid::new_v4(), now),
            Err(LifecycleError::Forbidden)
        );
        assert_eq!(
            repository.pay(member, pending_id, Uuid::new_v4(), now),
            Err(LifecycleError::InsufficientPoints)
        );
        assert_eq!(
            repository.pay(member, expired_id, Uuid::new_v4(), now),
            Err(LifecycleError::Expired)
        );
        repository.charge(member, Uuid::new_v4(), 20_000).unwrap();
        let transaction = Uuid::new_v4();
        repository
            .pay(member, pending_id, transaction, now)
            .unwrap();
        assert_eq!(
            repository.pay(member, expired_id, transaction, now),
            Err(LifecycleError::DuplicateTransaction)
        );
        repository.transact_store(|store| {
            assert_eq!(store.orders[1].status, "PENDING");
            assert_eq!(store.point_ledger.len(), 2);
        });
    }

    #[test]
    fn expiration_changes_only_expired_pending_orders_and_restores_quantity() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let product = Uuid::new_v4();
        let seller = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::for_order_tests([(
            purchase, product, seller, 8_000, 5, 10, "OPEN",
        )]);
        let mut expired = order(member, purchase, "PENDING", now);
        expired.expired_at = now_string(now - Duration::seconds(1));
        let expired_id = expired.order_id;
        let active = order(member, purchase, "PENDING", now);
        let active_id = active.order_id;
        let paid = order(member, purchase, "PAYMENT_COMPLETED", now);
        let paid_id = paid.order_id;
        let repository = OrderRepository::empty();
        repository.transact(|orders| orders.extend([expired, active, paid]));

        repository.expire_pending(&purchases, now).unwrap();

        repository.transact(|orders| {
            assert_eq!(
                orders
                    .iter()
                    .find(|item| item.order_id == expired_id)
                    .unwrap()
                    .status,
                "EXPIRED"
            );
            assert_eq!(
                orders
                    .iter()
                    .find(|item| item.order_id == active_id)
                    .unwrap()
                    .status,
                "PENDING"
            );
            assert_eq!(
                orders
                    .iter()
                    .find(|item| item.order_id == paid_id)
                    .unwrap()
                    .status,
                "PAYMENT_COMPLETED"
            );
        });
        assert_eq!(purchases.order_current_quantity(purchase), Some(3));
    }

    #[test]
    fn expiration_rolls_back_order_when_quantity_restore_fails() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let mut expired = order(member, purchase, "PENDING", now);
        expired.expired_at = now_string(now - Duration::seconds(1));
        let repository = OrderRepository::empty();
        repository.transact(|orders| orders.push(expired));
        let purchases = GroupPurchaseRepository::empty();

        assert_eq!(
            repository.expire_pending(&purchases, now),
            Err(LifecycleError::PurchaseNotFound)
        );
        repository.transact(|orders| assert_eq!(orders[0].status, "PENDING"));
    }

    #[test]
    fn expiration_releases_order_lock_before_group_purchase_transition() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let product = Uuid::new_v4();
        let seller = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::for_order_tests([(
            purchase, product, seller, 8_000, 2, 10, "OPEN",
        )]);
        let mut expired = order(member, purchase, "PENDING", now);
        expired.expired_at = now_string(now - Duration::seconds(1));
        let repository = OrderRepository::empty();
        repository.transact(|orders| orders.push(expired));
        let reached_purchase_step = Arc::new(Barrier::new(2));
        let continue_transition = Arc::new(Barrier::new(2));

        let worker_repository = repository.clone();
        let worker_purchases = purchases.clone();
        let worker_reached = reached_purchase_step.clone();
        let worker_continue = continue_transition.clone();
        let worker = thread::spawn(move || {
            worker_repository.expire_pending_with_hook(&worker_purchases, now, || {
                worker_reached.wait();
                worker_continue.wait();
            })
        });

        reached_purchase_step.wait();
        assert!(repository.try_transact_store(|_| ()).is_some());
        continue_transition.wait();
        assert_eq!(worker.join().unwrap(), Ok(()));
        assert_eq!(purchases.order_current_quantity(purchase), Some(0));
        repository.transact(|orders| assert_eq!(orders[0].status, "EXPIRED"));
    }

    #[test]
    fn expiration_revalidates_a_payment_that_started_before_expiry() {
        let expiration_run_at = OffsetDateTime::now_utc();
        let payment_started_at = expiration_run_at - Duration::seconds(2);
        let member = Uuid::new_v4();
        let product = Uuid::new_v4();
        let seller = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::for_order_tests([(
            purchase, product, seller, 8_000, 2, 10, "OPEN",
        )]);
        let mut expiring = order(member, purchase, "PENDING", expiration_run_at);
        expiring.expired_at = now_string(expiration_run_at - Duration::seconds(1));
        let order_id = expiring.order_id;
        let repository = OrderRepository::empty();
        repository.transact_store(|store| {
            store.orders.push(expiring);
            store.point_balances.insert(member, 20_000);
        });
        let expiration_snapshot_ready = Arc::new(Barrier::new(2));
        let payment_completed = Arc::new(Barrier::new(2));

        let worker_repository = repository.clone();
        let worker_purchases = purchases.clone();
        let worker_ready = expiration_snapshot_ready.clone();
        let worker_payment_completed = payment_completed.clone();
        let worker = thread::spawn(move || {
            worker_repository.expire_pending_with_hook(&worker_purchases, expiration_run_at, || {
                worker_ready.wait();
                worker_payment_completed.wait();
            })
        });

        expiration_snapshot_ready.wait();
        repository
            .pay(member, order_id, Uuid::new_v4(), payment_started_at)
            .unwrap();
        payment_completed.wait();

        assert_eq!(worker.join().unwrap(), Ok(()));
        assert_eq!(purchases.order_current_quantity(purchase), Some(2));
        repository.transact(|orders| assert_eq!(orders[0].status, "PAYMENT_COMPLETED"));
    }

    #[tokio::test]
    async fn charge_and_payment_routes_require_bearer_session() {
        let now = OffsetDateTime::now_utc();
        let member = Uuid::new_v4();
        let token = Uuid::new_v4();
        let purchase = Uuid::new_v4();
        let order = order(member, purchase, "PENDING", now);
        let order_id = order.order_id;
        let auth = AuthRepository::default();
        auth.insert_test_member(member).await;
        auth.insert_test_session(member, token).await;
        let state = state([order], GroupPurchaseRepository::empty(), auth);

        let (status, body) = send(
            state.clone(),
            authorized_json(
                "/api/points/charge",
                token,
                json!({"amount": 20_000, "transactionId": Uuid::new_v4()}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["balance"], 20_000);

        let (status, body) = send(
            state.clone(),
            authorized_json(
                &format!("/api/orders/{order_id}/point-payment"),
                token,
                json!({"transactionId": Uuid::new_v4()}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["paymentMethod"], "POINT");

        let (status, body) = send(
            state,
            Request::builder()
                .method("POST")
                .uri("/api/points/charge")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"amount": 1, "transactionId": Uuid::new_v4()}).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["message"], NO_LOGIN_INFO_MESSAGE);
    }
}
