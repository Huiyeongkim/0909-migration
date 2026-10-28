use super::{CanceledOrder, OrderSettlement, OrderState, OrderStore, PointLedgerEntry};
use crate::group_purchase::participate::{
    OrderCancellation, OrderCancellationError, OrderCancellationSnapshot,
    OrderCancellationTransactionError,
};
use axum::{
    extract::{rejection::JsonRejection, rejection::QueryRejection, Json, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

const INVALID_PARAMETER_MESSAGE: &str = "적절하지 않은 요청 값이 존재합니다.";
const NO_LOGIN_INFO_MESSAGE: &str = "로그인 정보가 없습니다.";
const ORDER_NOT_FOUND_MESSAGE: &str = "주문을 찾을 수 없습니다.";
const CANCELED_ORDER_NOT_FOUND_MESSAGE: &str = "취소된 주문을 찾을 수 없습니다.";
const INVALID_ORDER_STATUS_MESSAGE: &str = "결제 완료 상태의 주문만 취소할 수 있습니다.";
const CANCELLATION_NOT_ALLOWED_MESSAGE: &str = "주문 취소가 불가능한 상태입니다.";
const INVALID_CANCEL_STATUS_MESSAGE: &str = "취소 요청 상태가 아닙니다.";
const ORDER_ACCESS_DENIED_MESSAGE: &str = "본인의 주문내역만 조회할 수 있습니다.";
const SELLER_ACCESS_DENIED_MESSAGE: &str = "접근이 거부되었습니다. 판매자 권한이 필요합니다.";
const PURCHASE_NOT_FOUND_MESSAGE: &str = "공동구매를 찾을 수 없습니다.";
const DECREASE_QUANTITY_FAILED_MESSAGE: &str = "공동구매 수량 감소에 실패했습니다.";
const DUPLICATE_REQUEST_MESSAGE: &str = "이미 일부 처리된 요청입니다.";
const INTERNAL_ERROR_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";
const CANCEL_CREATED_MESSAGE: &str = "주문 취소 되었습니다.";
const CANCEL_LIST_MESSAGE: &str = "주문 취소 목록을 조회했습니다.";
const PENDING_LIST_MESSAGE: &str = "주문 취소 대기 내역을 페이징하여 조회했습니다.";
const APPROVED_MESSAGE: &str = "판매자가 주문 취소 승인했습니다.";
const REJECTED_MESSAGE: &str = "판매자가 주문 취소 거부했습니다.";
const RETRY_MESSAGE: &str = "주문 취소 재처리를 실행했습니다.";

const VOID_POLICY: &str = "CANCEL_POLICY_VOID_V1";
const REVERSAL_POLICY: &str = "CANCEL_POLICY_REVERSAL_V1";
const REFUND_POLICY: &str = "CANCEL_POLICY_REFUND_V1";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CancelReason {
    GroupPurchaseFailed,
    ChangeOfMind,
    ProductDefect,
    DeliveryDelay,
    OutOfStock,
}

impl CancelReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::GroupPurchaseFailed => "GROUP_PURCHASE_FAILED",
            Self::ChangeOfMind => "CHANGE_OF_MIND",
            Self::ProductDefect => "PRODUCT_DEFECT",
            Self::DeliveryDelay => "DELIVERY_DELAY",
            Self::OutOfStock => "OUT_OF_STOCK",
        }
    }

    fn is_buyer_fault(self) -> bool {
        self == Self::ChangeOfMind
    }

    fn is_seller_fault(self) -> bool {
        matches!(
            self,
            Self::ProductDefect | Self::DeliveryDelay | Self::OutOfStock
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelRequest {
    reason: CancelReason,
    detail_reason: String,
    idempotency_key: String,
}

#[derive(Deserialize)]
struct ListQuery {
    page: Option<usize>,
    size: Option<usize>,
    sort: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderCancelInfo {
    order_id: Uuid,
    status: &'static str,
    original_paid_amount: i64,
    cancel_fee_amount: i64,
    shipping_fee_amount: i64,
    refund_amount: i64,
    reason: &'static str,
    detail_reason: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelPage {
    content: Vec<OrderCancelInfo>,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefundLog {
    event: &'static str,
    transaction_id: Uuid,
    order_id: Uuid,
    amount: i64,
}

#[derive(Debug, PartialEq, Eq)]
enum CancelError {
    OrderNotFound,
    CancellationNotFound,
    InvalidOrderStatus,
    CancellationNotAllowed,
    InvalidCancelStatus,
    BuyerForbidden,
    SellerForbidden,
    PurchaseNotFound,
    Quantity,
    DuplicateRequest,
    Internal,
}

#[derive(Clone, Copy)]
struct RefundPolicy {
    id: &'static str,
    refund: i64,
    fee: i64,
    shipping: i64,
}

pub(super) fn router() -> Router<OrderState> {
    Router::new()
        .route("/api/orders/cancel/retry", post(retry_cancellations))
        .route("/api/orders/cancel/auto", post(auto_approve_cancellations))
        .route("/api/orders/cancel/{order_id}", post(request_cancellation))
        .route("/api/orders/cancel", get(list_buyer_cancellations))
        .route(
            "/api/orders/cancel/pending",
            get(list_pending_cancellations),
        )
        .route(
            "/api/orders/cancel/{order_id}/approve",
            patch(approve_cancellation),
        )
        .route(
            "/api/orders/cancel/{order_id}/reject",
            patch(reject_cancellation),
        )
}

async fn request_cancellation(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    request: Result<Json<CancelRequest>, JsonRejection>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return api_response::<()>(status, None, message),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return invalid_parameter_response();
    };
    let Ok(Json(request)) = request else {
        return invalid_parameter_response();
    };
    if request.detail_reason.trim().is_empty()
        || request.idempotency_key.trim().is_empty()
        || request.detail_reason.chars().count() > 1_000
        || request.idempotency_key.chars().count() > 255
    {
        return invalid_parameter_response();
    }
    match state.orders.request_cancellation(
        &state.group_purchases,
        member_id,
        order_id,
        request,
        OffsetDateTime::now_utc(),
    ) {
        Ok((_, refund)) => {
            log_refunds(refund);
            api_response::<()>(StatusCode::OK, None, CANCEL_CREATED_MESSAGE)
        }
        Err(error) => error_response(error),
    }
}

async fn list_buyer_cancellations(
    State(state): State<OrderState>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    list_cancellations(state, headers, query, false)
}

async fn list_pending_cancellations(
    State(state): State<OrderState>,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    list_cancellations(state, headers, query, true)
}

fn list_cancellations(
    state: OrderState,
    headers: HeaderMap,
    query: Result<Query<ListQuery>, QueryRejection>,
    seller_pending: bool,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return api_response::<()>(status, None, message),
    };
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => return invalid_parameter_response(),
    };
    let page = query.page.unwrap_or(0);
    let size = query.size.unwrap_or(20);
    if size == 0 {
        return invalid_parameter_response();
    }
    let descending = match query.sort.as_deref() {
        None | Some("createdAt,asc") => false,
        Some("createdAt,desc") => true,
        Some(_) => return invalid_parameter_response(),
    };
    let (cancellations, total_elements) = state.orders.transact_store(|store| {
        let mut cancellations = store
            .cancellations
            .iter()
            .filter(|cancellation| {
                store.orders.iter().any(|order| {
                    order.order_id == cancellation.order_id && order.deleted_at.is_none()
                }) && if seller_pending {
                    cancellation.seller_id == member_id && cancellation.status == "PENDING"
                } else {
                    cancellation.member_id == member_id
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        cancellations.sort_by(|left, right| {
            let ordering = left.created_at.cmp(&right.created_at);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        let total = cancellations.len();
        let content = cancellations
            .into_iter()
            .skip(page.saturating_mul(size))
            .take(size)
            .collect::<Vec<_>>();
        (content, total)
    });
    let content = cancellations
        .iter()
        .map(OrderCancelInfo::from)
        .collect::<Vec<_>>();
    let total_pages = total_elements.div_ceil(size);
    let number_of_elements = content.len();
    api_response(
        StatusCode::OK,
        Some(CancelPage {
            content,
            total_pages,
            total_elements,
            first: page == 0,
            last: total_pages == 0 || page.saturating_add(1) >= total_pages,
            size,
            number_of_elements,
        }),
        if seller_pending {
            PENDING_LIST_MESSAGE
        } else {
            CANCEL_LIST_MESSAGE
        },
    )
}

async fn approve_cancellation(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return api_response::<()>(status, None, message),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return invalid_parameter_response();
    };
    match state
        .orders
        .approve_cancellation(&state.group_purchases, member_id, order_id)
    {
        Ok((cancellation, refund)) => {
            log_refunds(Some(refund));
            api_response(
                StatusCode::OK,
                Some(OrderCancelInfo::from(&cancellation)),
                APPROVED_MESSAGE,
            )
        }
        Err(error) => error_response(error),
    }
}

async fn reject_cancellation(
    State(state): State<OrderState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Response {
    let member_id = match member_id(&headers) {
        Ok(member_id) => member_id,
        Err((status, message)) => return api_response::<()>(status, None, message),
    };
    let Ok(order_id) = Uuid::parse_str(&order_id) else {
        return invalid_parameter_response();
    };
    match state.orders.reject_cancellation(member_id, order_id) {
        Ok(cancellation) => api_response(
            StatusCode::OK,
            Some(OrderCancelInfo::from(&cancellation)),
            REJECTED_MESSAGE,
        ),
        Err(error) => error_response(error),
    }
}

async fn retry_cancellations(State(state): State<OrderState>) -> Response {
    match state.orders.retry_cancellations(OffsetDateTime::now_utc()) {
        Ok(refunds) => {
            log_refunds(refunds);
            api_response::<()>(StatusCode::OK, None, RETRY_MESSAGE)
        }
        Err(error) => error_response(error),
    }
}

async fn auto_approve_cancellations(State(state): State<OrderState>) -> Response {
    match state
        .orders
        .auto_approve_cancellations(&state.group_purchases, OffsetDateTime::now_utc())
    {
        Ok(refunds) => {
            log_refunds(refunds);
            api_response::<()>(StatusCode::OK, None, RETRY_MESSAGE)
        }
        Err(error) => error_response(error),
    }
}

impl super::OrderRepository {
    fn request_cancellation(
        &self,
        purchases: &crate::group_purchase::GroupPurchaseRepository,
        member_id: Uuid,
        order_id: Uuid,
        request: CancelRequest,
        now: OffsetDateTime,
    ) -> Result<(CanceledOrder, Option<RefundLog>), CancelError> {
        self.transact_store(|store| {
            let original = store.clone();
            let result =
                request_cancellation_in_store(store, purchases, member_id, order_id, request, now);
            if result.is_err() {
                *store = original;
            }
            result
        })
    }

    fn approve_cancellation(
        &self,
        purchases: &crate::group_purchase::GroupPurchaseRepository,
        seller_id: Uuid,
        order_id: Uuid,
    ) -> Result<(CanceledOrder, RefundLog), CancelError> {
        self.transact_store(|store| {
            let original = store.clone();
            let result = approve_cancellation_in_store(store, purchases, seller_id, order_id);
            if result.is_err() {
                *store = original;
            }
            result
        })
    }

    fn reject_cancellation(
        &self,
        seller_id: Uuid,
        order_id: Uuid,
    ) -> Result<CanceledOrder, CancelError> {
        self.transact_store(|store| {
            let cancellation_index = store
                .cancellations
                .iter()
                .position(|item| item.order_id == order_id)
                .ok_or(CancelError::CancellationNotFound)?;
            let order_index = store
                .orders
                .iter()
                .position(|order| order.order_id == order_id && order.deleted_at.is_none())
                .ok_or(CancelError::OrderNotFound)?;
            if store.orders[order_index].seller_id != seller_id {
                return Err(CancelError::SellerForbidden);
            }
            if store.cancellations[cancellation_index].status != "PENDING" {
                return Err(CancelError::InvalidCancelStatus);
            }
            let updated_at = now_string();
            store.cancellations[cancellation_index].status = "REJECTED";
            store.cancellations[cancellation_index].updated_at = Some(updated_at.clone());
            store.orders[order_index].status = "PAYMENT_COMPLETED";
            store.orders[order_index].updated_at = Some(updated_at);
            Ok(store.cancellations[cancellation_index].clone())
        })
    }

    fn retry_cancellations(&self, now: OffsetDateTime) -> Result<Vec<RefundLog>, CancelError> {
        self.transact_store(|store| {
            let original = store.clone();
            let cutoff = now - Duration::minutes(15);
            let targets = store
                .cancellations
                .iter()
                .filter(|item| {
                    matches!(item.status, "REQUESTED" | "APPROVED")
                        && parse_time(&item.canceled_at).is_some_and(|time| time < cutoff)
                        && store.orders.iter().any(|order| {
                            order.order_id == item.order_id && order.deleted_at.is_none()
                        })
                })
                .map(|item| item.canceled_order_id)
                .collect::<Vec<_>>();
            let mut logs = Vec::new();
            for id in targets {
                let index = store
                    .cancellations
                    .iter()
                    .position(|item| item.canceled_order_id == id)
                    .expect("selected cancellation must remain present");
                match complete_refund(store, index, now) {
                    Ok(log) => logs.push(log),
                    Err(error) => {
                        *store = original;
                        return Err(error);
                    }
                }
            }
            Ok(logs)
        })
    }

    fn auto_approve_cancellations(
        &self,
        purchases: &crate::group_purchase::GroupPurchaseRepository,
        now: OffsetDateTime,
    ) -> Result<Vec<RefundLog>, CancelError> {
        self.transact_store(|store| {
            let original = store.clone();
            let cutoff = now - Duration::days(2);
            let targets = store
                .cancellations
                .iter()
                .filter(|item| {
                    item.status == "PENDING"
                        && parse_time(&item.canceled_at).is_some_and(|time| time < cutoff)
                })
                .filter_map(|item| {
                    store
                        .orders
                        .iter()
                        .find(|order| order.order_id == item.order_id && order.deleted_at.is_none())
                        .map(|order| AutoTarget {
                            cancellation_id: item.canceled_order_id,
                            group_purchase_id: order.group_purchase_id,
                            quantity: order.quantity,
                        })
                })
                .collect::<Vec<_>>();
            let requests = targets
                .iter()
                .map(|target| OrderCancellation {
                    group_purchase_id: target.group_purchase_id,
                    quantity: target.quantity,
                })
                .collect::<Vec<_>>();
            let result = purchases.with_order_cancellations(&requests, |_| {
                let mut logs = Vec::new();
                for target in &targets {
                    let index = store
                        .cancellations
                        .iter()
                        .position(|item| item.canceled_order_id == target.cancellation_id)
                        .expect("selected cancellation must remain present");
                    logs.push(complete_refund(store, index, now)?);
                }
                Ok::<_, CancelError>(logs)
            });
            match result {
                Ok(logs) => Ok(logs),
                Err(error) => {
                    *store = original;
                    Err(map_group_error(error))
                }
            }
        })
    }
}

#[derive(Clone, Copy)]
struct AutoTarget {
    cancellation_id: Uuid,
    group_purchase_id: Uuid,
    quantity: i32,
}

fn request_cancellation_in_store(
    store: &mut OrderStore,
    purchases: &crate::group_purchase::GroupPurchaseRepository,
    member_id: Uuid,
    order_id: Uuid,
    request: CancelRequest,
    now: OffsetDateTime,
) -> Result<(CanceledOrder, Option<RefundLog>), CancelError> {
    let order_index = store
        .orders
        .iter()
        .position(|order| order.order_id == order_id && order.deleted_at.is_none())
        .ok_or(CancelError::OrderNotFound)?;
    if store.orders[order_index].member_id != member_id {
        return Err(CancelError::BuyerForbidden);
    }
    if let Some(existing) = store
        .cancellations
        .iter()
        .find(|item| item.order_id == order_id)
    {
        return Ok((existing.clone(), None));
    }
    if store
        .cancellations
        .iter()
        .any(|item| item.member_id == member_id && item.idempotency_key == request.idempotency_key)
    {
        return Err(CancelError::DuplicateRequest);
    }
    if store.orders[order_index].status != "PAYMENT_COMPLETED" {
        return Err(CancelError::InvalidOrderStatus);
    }
    if !request.reason.is_buyer_fault() && !request.reason.is_seller_fault() {
        return Err(CancelError::CancellationNotAllowed);
    }
    let group_purchase_id = store.orders[order_index].group_purchase_id;
    let quantity = store.orders[order_index].quantity;
    if request.reason.is_buyer_fault() {
        purchases
            .with_order_cancellation(group_purchase_id, quantity, |snapshot| {
                create_cancellation(store, order_index, request, snapshot, now, true)
            })
            .map_err(map_group_error)
    } else {
        let snapshot = purchases
            .order_cancellation_snapshot(group_purchase_id)
            .map_err(map_cancellation_error)?;
        create_cancellation(store, order_index, request, &snapshot, now, false)
    }
}

fn create_cancellation(
    store: &mut OrderStore,
    order_index: usize,
    request: CancelRequest,
    purchase: &OrderCancellationSnapshot,
    now: OffsetDateTime,
    refund_immediately: bool,
) -> Result<(CanceledOrder, Option<RefundLog>), CancelError> {
    let order = &store.orders[order_index];
    debug_assert_eq!(order.group_purchase_id, purchase.group_purchase_id);
    let policy = resolve_policy(
        request.reason,
        order.paid_price,
        purchase.succeeded_at.as_deref(),
        now,
    )?;
    let timestamp = format_time(now);
    let cancellation = CanceledOrder {
        canceled_order_id: Uuid::new_v4(),
        order_id: order.order_id,
        member_id: order.member_id,
        seller_id: order.seller_id,
        original_paid_amount: order.paid_price,
        cancel_fee_amount: policy.fee,
        shipping_fee_amount: policy.shipping,
        refund_amount: policy.refund,
        policy_id: policy.id,
        policy_snapshot: json!({"policyId": policy.id, "originalPaidAmount": order.paid_price,
            "cancelFeeAmount": policy.fee, "shippingFeeAmount": policy.shipping,
            "refundAmount": policy.refund})
        .to_string(),
        status: if refund_immediately {
            "REQUESTED"
        } else {
            "PENDING"
        },
        reason: request.reason.as_str(),
        detail_reason: request.detail_reason,
        idempotency_key: request.idempotency_key,
        canceled_at: timestamp.clone(),
        returned_at: None,
        created_at: timestamp.clone(),
        updated_at: None,
    };
    store.orders[order_index].status = "CANCELLED";
    store.orders[order_index].updated_at = Some(timestamp);
    store.cancellations.push(cancellation);
    let cancellation_index = store.cancellations.len() - 1;
    let refund = if refund_immediately {
        Some(complete_refund(store, cancellation_index, now)?)
    } else {
        None
    };
    Ok((store.cancellations[cancellation_index].clone(), refund))
}

fn approve_cancellation_in_store(
    store: &mut OrderStore,
    purchases: &crate::group_purchase::GroupPurchaseRepository,
    seller_id: Uuid,
    order_id: Uuid,
) -> Result<(CanceledOrder, RefundLog), CancelError> {
    let cancellation_index = store
        .cancellations
        .iter()
        .position(|item| item.order_id == order_id)
        .ok_or(CancelError::CancellationNotFound)?;
    let order_index = store
        .orders
        .iter()
        .position(|order| order.order_id == order_id && order.deleted_at.is_none())
        .ok_or(CancelError::OrderNotFound)?;
    if store.orders[order_index].seller_id != seller_id {
        return Err(CancelError::SellerForbidden);
    }
    if store.cancellations[cancellation_index].status != "PENDING" {
        return Err(CancelError::InvalidCancelStatus);
    }
    let group_purchase_id = store.orders[order_index].group_purchase_id;
    let quantity = store.orders[order_index].quantity;
    purchases
        .with_order_cancellation(group_purchase_id, quantity, |_| {
            let refund = complete_refund(store, cancellation_index, OffsetDateTime::now_utc())?;
            Ok::<_, CancelError>((store.cancellations[cancellation_index].clone(), refund))
        })
        .map_err(map_group_error)
}

fn complete_refund(
    store: &mut OrderStore,
    index: usize,
    now: OffsetDateTime,
) -> Result<RefundLog, CancelError> {
    let cancellation = store.cancellations[index].clone();
    if store
        .point_ledger
        .iter()
        .any(|entry| entry.order_id == cancellation.order_id && entry.kind == "REFUND")
    {
        return Err(CancelError::InvalidCancelStatus);
    }
    let balance = store
        .point_balances
        .entry(cancellation.member_id)
        .or_default();
    *balance = balance
        .checked_add(cancellation.refund_amount)
        .ok_or(CancelError::Internal)?;
    let timestamp = format_time(now);
    let transaction_id = Uuid::new_v4();
    store.point_ledger.push(PointLedgerEntry {
        transaction_id,
        member_id: cancellation.member_id,
        order_id: cancellation.order_id,
        amount: cancellation.refund_amount,
        balance_after: *balance,
        kind: "REFUND",
        created_at: timestamp.clone(),
    });
    if cancellation.reason == "CHANGE_OF_MIND"
        && !store
            .settlements
            .iter()
            .any(|item| item.order_id == cancellation.order_id && item.status == "BUYER_CANCEL")
    {
        let group_purchase_id = store
            .orders
            .iter()
            .find(|order| order.order_id == cancellation.order_id)
            .ok_or(CancelError::OrderNotFound)?
            .group_purchase_id;
        store.settlements.push(OrderSettlement {
            order_settlement_id: Uuid::new_v4(),
            seller_id: cancellation.seller_id,
            group_purchase_id,
            order_id: cancellation.order_id,
            status: "BUYER_CANCEL",
            order_amount: 0,
            platform_fee_rate: 0.0,
            platform_fee: 0,
            settlement_amount: cancellation.cancel_fee_amount,
            created_at: timestamp.clone(),
            settled_at: None,
        });
    }
    store.cancellations[index].status = "COMPLETED";
    store.cancellations[index].returned_at = Some(timestamp.clone());
    store.cancellations[index].updated_at = Some(timestamp);
    Ok(RefundLog {
        event: "local_point_refund",
        transaction_id,
        order_id: cancellation.order_id,
        amount: cancellation.refund_amount,
    })
}

fn resolve_policy(
    reason: CancelReason,
    paid_amount: i64,
    succeeded_at: Option<&str>,
    canceled_at: OffsetDateTime,
) -> Result<RefundPolicy, CancelError> {
    if reason.is_seller_fault() || (reason.is_buyer_fault() && succeeded_at.is_none()) {
        return Ok(RefundPolicy {
            id: VOID_POLICY,
            refund: paid_amount.max(0),
            fee: 0,
            shipping: 0,
        });
    }
    if !reason.is_buyer_fault() {
        return Err(CancelError::CancellationNotAllowed);
    }
    let succeeded_at = succeeded_at
        .and_then(parse_time)
        .ok_or(CancelError::CancellationNotAllowed)?;
    let elapsed = canceled_at - succeeded_at;
    let fee = (paid_amount as f64 * 0.2) as i64;
    let available = paid_amount.saturating_sub(fee).max(0);
    if elapsed < Duration::hours(48) {
        Ok(RefundPolicy {
            id: REVERSAL_POLICY,
            refund: available,
            fee,
            shipping: 0,
        })
    } else if elapsed < Duration::days(14) {
        let shipping = available.min(6_000);
        Ok(RefundPolicy {
            id: REFUND_POLICY,
            refund: available - shipping,
            fee,
            shipping,
        })
    } else {
        Err(CancelError::CancellationNotAllowed)
    }
}

impl From<&CanceledOrder> for OrderCancelInfo {
    fn from(item: &CanceledOrder) -> Self {
        Self {
            order_id: item.order_id,
            status: item.status,
            original_paid_amount: item.original_paid_amount,
            cancel_fee_amount: item.cancel_fee_amount,
            shipping_fee_amount: item.shipping_fee_amount,
            refund_amount: item.refund_amount,
            reason: item.reason,
            detail_reason: item.detail_reason.clone(),
            created_at: item.created_at.clone(),
        }
    }
}

fn map_group_error(error: OrderCancellationTransactionError<CancelError>) -> CancelError {
    match error {
        OrderCancellationTransactionError::Cancellation(error) => map_cancellation_error(error),
        OrderCancellationTransactionError::Commit(error) => error,
    }
}

fn map_cancellation_error(error: OrderCancellationError) -> CancelError {
    match error {
        OrderCancellationError::NotFound => CancelError::PurchaseNotFound,
        OrderCancellationError::Quantity => CancelError::Quantity,
    }
}

fn member_id(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let Some(value) = headers.get("x-member-id") else {
        return Err((StatusCode::UNAUTHORIZED, NO_LOGIN_INFO_MESSAGE));
    };
    let value = value
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))?;
    Uuid::parse_str(value).map_err(|_| (StatusCode::BAD_REQUEST, INVALID_PARAMETER_MESSAGE))
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

fn invalid_parameter_response() -> Response {
    api_response::<()>(StatusCode::BAD_REQUEST, None, INVALID_PARAMETER_MESSAGE)
}

fn error_response(error: CancelError) -> Response {
    let (status, message) = match error {
        CancelError::OrderNotFound => (StatusCode::NOT_FOUND, ORDER_NOT_FOUND_MESSAGE),
        CancelError::CancellationNotFound => {
            (StatusCode::NOT_FOUND, CANCELED_ORDER_NOT_FOUND_MESSAGE)
        }
        CancelError::InvalidOrderStatus => (StatusCode::BAD_REQUEST, INVALID_ORDER_STATUS_MESSAGE),
        CancelError::CancellationNotAllowed => {
            (StatusCode::BAD_REQUEST, CANCELLATION_NOT_ALLOWED_MESSAGE)
        }
        CancelError::InvalidCancelStatus => {
            (StatusCode::BAD_REQUEST, INVALID_CANCEL_STATUS_MESSAGE)
        }
        CancelError::BuyerForbidden => (StatusCode::FORBIDDEN, ORDER_ACCESS_DENIED_MESSAGE),
        CancelError::SellerForbidden => (StatusCode::FORBIDDEN, SELLER_ACCESS_DENIED_MESSAGE),
        CancelError::PurchaseNotFound => (StatusCode::NOT_FOUND, PURCHASE_NOT_FOUND_MESSAGE),
        CancelError::Quantity => (StatusCode::BAD_REQUEST, DECREASE_QUANTITY_FAILED_MESSAGE),
        CancelError::DuplicateRequest => (StatusCode::CONFLICT, DUPLICATE_REQUEST_MESSAGE),
        CancelError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_MESSAGE),
    };
    api_response::<()>(status, None, message)
}

fn log_refunds(refunds: impl IntoIterator<Item = RefundLog>) {
    for refund in refunds {
        eprintln!(
            "{}",
            serde_json::to_string(&refund).expect("refund log must serialize")
        );
    }
}

fn parse_time(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}
fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .expect("RFC3339 formatting must succeed")
}
fn now_string() -> String {
    format_time(OffsetDateTime::now_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::CartRepository,
        group_purchase::GroupPurchaseRepository,
        local_auth::AuthRepository,
        order::{NewOrder, Order, OrderRepository},
        LocalProductRepository,
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::Value;
    use std::{sync::Arc, thread};
    use tower::ServiceExt;

    struct Fixture {
        state: OrderState,
        buyer_id: Uuid,
        seller_id: Uuid,
        order_id: Uuid,
        purchase_id: Uuid,
    }

    fn fixture(status: &'static str, paid_price: i64, quantity: i32) -> Fixture {
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let mut order = Order::create(NewOrder {
            quantity,
            unit_price: paid_price / i64::from(quantity),
            member_id: buyer_id,
            address: "서울시 테스트 주소".to_owned(),
            address_detail: "101호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "구매자".to_owned(),
            seller_id,
            group_purchase_id: purchase_id,
            idempotency_key: Uuid::new_v4().to_string(),
        });
        order.paid_price = paid_price;
        order.status = status;
        let order_id = order.order_id;
        let orders = OrderRepository::empty();
        orders.transact(|stored| stored.push(order));
        let group_purchases = GroupPurchaseRepository::for_order_tests([(
            purchase_id,
            product_id,
            seller_id,
            8_000,
            quantity + 5,
            quantity + 20,
            "OPEN",
        )]);
        Fixture {
            state: OrderState {
                orders,
                carts: CartRepository::empty(),
                group_purchases,
                auth: AuthRepository::default(),
                products: Arc::new(LocalProductRepository::new([])),
            },
            buyer_id,
            seller_id,
            order_id,
            purchase_id,
        }
    }

    fn cancel_body(reason: &str, key: &str) -> Value {
        json!({
            "reason": reason,
            "detailReason": "테스트 취소 사유",
            "idempotencyKey": key,
        })
    }

    async fn send(
        state: OrderState,
        method: &str,
        uri: &str,
        member_id: Option<Uuid>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(member_id) = member_id {
            request = request.header("x-member-id", member_id.to_string());
        }
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        let response = router()
            .with_state(state)
            .oneshot(
                request
                    .body(Body::from(
                        body.map_or_else(String::new, |body| body.to_string()),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[test]
    fn approved_time_boundaries_and_low_value_refunds_are_nonnegative() {
        let success = OffsetDateTime::parse("2026-01-01T00:00:00Z", &Rfc3339).unwrap();
        let reversal = resolve_policy(
            CancelReason::ChangeOfMind,
            20_000,
            Some("2026-01-01T00:00:00Z"),
            success + Duration::hours(47),
        )
        .unwrap();
        assert_eq!(
            (
                reversal.id,
                reversal.fee,
                reversal.shipping,
                reversal.refund
            ),
            (REVERSAL_POLICY, 4_000, 0, 16_000)
        );

        let boundary = resolve_policy(
            CancelReason::ChangeOfMind,
            5_000,
            Some("2026-01-01T00:00:00Z"),
            success + Duration::hours(48),
        )
        .unwrap();
        assert_eq!(
            (
                boundary.id,
                boundary.fee,
                boundary.shipping,
                boundary.refund
            ),
            (REFUND_POLICY, 1_000, 4_000, 0)
        );
        assert!(resolve_policy(
            CancelReason::ChangeOfMind,
            20_000,
            Some("2026-01-01T00:00:00Z"),
            success + Duration::days(14),
        )
        .is_err());
    }

    #[tokio::test]
    async fn buyer_cancellation_refunds_points_decreases_quantity_and_is_idempotent() {
        let fixture = fixture("PAYMENT_COMPLETED", 20_000, 2);
        let before = fixture
            .state
            .group_purchases
            .order_test_quantity(fixture.purchase_id)
            .unwrap();
        let uri = format!("/api/orders/cancel/{}", fixture.order_id);
        let (status, response) = send(
            fixture.state.clone(),
            "POST",
            &uri,
            Some(fixture.buyer_id),
            Some(cancel_body("CHANGE_OF_MIND", "buyer-key")),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["message"], CANCEL_CREATED_MESSAGE);
        let (repeat_status, _) = send(
            fixture.state.clone(),
            "POST",
            &uri,
            Some(fixture.buyer_id),
            Some(cancel_body("CHANGE_OF_MIND", "changed-key")),
        )
        .await;
        assert_eq!(repeat_status, StatusCode::OK);
        let (list_status, list) = send(
            fixture.state.clone(),
            "GET",
            "/api/orders/cancel?page=0&size=10&sort=createdAt,desc",
            Some(fixture.buyer_id),
            None,
        )
        .await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(list["message"], CANCEL_LIST_MESSAGE);
        assert_eq!(list["data"]["totalElements"], 1);
        assert_eq!(list["data"]["content"][0]["status"], "COMPLETED");
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(before - 2)
        );
        fixture.state.orders.transact_store(|store| {
            assert_eq!(store.orders[0].status, "CANCELLED");
            assert_eq!(store.cancellations.len(), 1);
            assert_eq!(store.cancellations[0].status, "COMPLETED");
            assert_eq!(store.cancellations[0].policy_id, VOID_POLICY);
            assert!(store.cancellations[0].policy_snapshot.contains(VOID_POLICY));
            assert_eq!(store.point_balances[&fixture.buyer_id], 20_000);
            assert_eq!(store.point_ledger.len(), 1);
            assert_ne!(store.point_ledger[0].transaction_id, Uuid::nil());
            assert_eq!(store.point_ledger[0].member_id, fixture.buyer_id);
            assert_eq!(store.point_ledger[0].amount, 20_000);
            assert!(parse_time(&store.point_ledger[0].created_at).is_some());
            assert_eq!(store.settlements.len(), 1);
            assert_eq!(store.settlements[0].status, "BUYER_CANCEL");
        });
    }

    #[tokio::test]
    async fn seller_rejection_restores_paid_order_and_approval_refunds_once() {
        let rejected = fixture("PAYMENT_COMPLETED", 20_000, 1);
        let rejected_uri = format!("/api/orders/cancel/{}", rejected.order_id);
        send(
            rejected.state.clone(),
            "POST",
            &rejected_uri,
            Some(rejected.buyer_id),
            Some(cancel_body("PRODUCT_DEFECT", "seller-reject")),
        )
        .await;
        let (pending_status, pending) = send(
            rejected.state.clone(),
            "GET",
            "/api/orders/cancel/pending",
            Some(rejected.seller_id),
            None,
        )
        .await;
        assert_eq!(pending_status, StatusCode::OK);
        assert_eq!(pending["message"], PENDING_LIST_MESSAGE);
        assert_eq!(pending["data"]["totalElements"], 1);
        let (foreign_status, foreign) = send(
            rejected.state.clone(),
            "PATCH",
            &format!("{rejected_uri}/reject"),
            Some(Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(foreign_status, StatusCode::FORBIDDEN);
        assert_eq!(foreign["message"], SELLER_ACCESS_DENIED_MESSAGE);
        let (status, response) = send(
            rejected.state.clone(),
            "PATCH",
            &format!("{rejected_uri}/reject"),
            Some(rejected.seller_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["data"]["status"], "REJECTED");
        rejected.state.orders.transact_store(|store| {
            assert_eq!(store.orders[0].status, "PAYMENT_COMPLETED");
            assert!(store.point_ledger.is_empty());
        });

        let approved = fixture("PAYMENT_COMPLETED", 20_000, 1);
        let approved_uri = format!("/api/orders/cancel/{}", approved.order_id);
        send(
            approved.state.clone(),
            "POST",
            &approved_uri,
            Some(approved.buyer_id),
            Some(cancel_body("OUT_OF_STOCK", "seller-approve")),
        )
        .await;
        let (status, response) = send(
            approved.state.clone(),
            "PATCH",
            &format!("{approved_uri}/approve"),
            Some(approved.seller_id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["data"]["status"], "COMPLETED");
        approved.state.orders.transact_store(|store| {
            assert_eq!(store.point_ledger.len(), 1);
            assert!(store.settlements.is_empty());
        });
    }

    #[tokio::test]
    async fn ownership_soft_delete_invalid_reason_and_list_contracts_are_enforced() {
        let fixture = fixture("PAYMENT_COMPLETED", 20_000, 1);
        let uri = format!("/api/orders/cancel/{}", fixture.order_id);
        let (foreign_status, _) = send(
            fixture.state.clone(),
            "POST",
            &uri,
            Some(Uuid::new_v4()),
            Some(cancel_body("CHANGE_OF_MIND", "shared-key")),
        )
        .await;
        assert_eq!(foreign_status, StatusCode::FORBIDDEN);

        let (reason_status, reason_response) = send(
            fixture.state.clone(),
            "POST",
            &uri,
            Some(fixture.buyer_id),
            Some(cancel_body("GROUP_PURCHASE_FAILED", "shared-key")),
        )
        .await;
        assert_eq!(reason_status, StatusCode::BAD_REQUEST);
        assert_eq!(reason_response["message"], CANCELLATION_NOT_ALLOWED_MESSAGE);

        fixture
            .state
            .orders
            .transact(|orders| orders[0].deleted_at = Some("2026-01-01T00:00:00Z".to_owned()));
        let (deleted_status, _) = send(
            fixture.state.clone(),
            "POST",
            &uri,
            Some(fixture.buyer_id),
            Some(cancel_body("CHANGE_OF_MIND", "shared-key")),
        )
        .await;
        assert_eq!(deleted_status, StatusCode::NOT_FOUND);
        fixture.state.orders.transact_store(|store| {
            assert!(store.cancellations.is_empty());
            assert!(store.point_ledger.is_empty());
            let timestamp = now_string();
            store.cancellations.push(CanceledOrder {
                canceled_order_id: Uuid::new_v4(),
                order_id: fixture.order_id,
                member_id: fixture.buyer_id,
                seller_id: fixture.seller_id,
                original_paid_amount: 20_000,
                cancel_fee_amount: 0,
                shipping_fee_amount: 0,
                refund_amount: 20_000,
                policy_id: VOID_POLICY,
                policy_snapshot: "{}".to_owned(),
                status: "PENDING",
                reason: "PRODUCT_DEFECT",
                detail_reason: "삭제 주문".to_owned(),
                idempotency_key: Uuid::new_v4().to_string(),
                canceled_at: timestamp.clone(),
                returned_at: None,
                created_at: timestamp,
                updated_at: None,
            });
        });

        for (member_id, uri) in [
            (fixture.buyer_id, "/api/orders/cancel"),
            (fixture.seller_id, "/api/orders/cancel/pending"),
        ] {
            let (status, response) =
                send(fixture.state.clone(), "GET", uri, Some(member_id), None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(response["data"]["totalElements"], 0);
        }
    }

    #[test]
    fn concurrent_requests_create_one_cancellation_and_refund() {
        let fixture = fixture("PAYMENT_COMPLETED", 20_000, 2);
        let state = Arc::new(fixture.state);
        let buyer_id = fixture.buyer_id;
        let order_id = fixture.order_id;
        let attempts = (0..8)
            .map(|_| {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    state.orders.request_cancellation(
                        &state.group_purchases,
                        buyer_id,
                        order_id,
                        CancelRequest {
                            reason: CancelReason::ChangeOfMind,
                            detail_reason: "동시 요청".to_owned(),
                            idempotency_key: "same-key".to_owned(),
                        },
                        OffsetDateTime::now_utc(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for attempt in attempts {
            assert!(attempt.join().unwrap().is_ok());
        }
        state.orders.transact_store(|store| {
            assert_eq!(store.cancellations.len(), 1);
            assert_eq!(store.point_ledger.len(), 1);
        });
    }

    #[test]
    fn idempotency_keys_are_scoped_to_the_buyer() {
        let fixture = fixture("PAYMENT_COMPLETED", 20_000, 1);
        let other_buyer = Uuid::new_v4();
        let mut other_order = Order::create(NewOrder {
            quantity: 1,
            unit_price: 20_000,
            member_id: other_buyer,
            address: "서울시 테스트 주소".to_owned(),
            address_detail: "202호".to_owned(),
            postal_code: "12345".to_owned(),
            receiver_name: "다른 구매자".to_owned(),
            seller_id: fixture.seller_id,
            group_purchase_id: fixture.purchase_id,
            idempotency_key: Uuid::new_v4().to_string(),
        });
        other_order.status = "PAYMENT_COMPLETED";
        let other_order_id = other_order.order_id;
        fixture
            .state
            .orders
            .transact(|orders| orders.push(other_order));

        for (member_id, order_id) in [
            (fixture.buyer_id, fixture.order_id),
            (other_buyer, other_order_id),
        ] {
            assert!(fixture
                .state
                .orders
                .request_cancellation(
                    &fixture.state.group_purchases,
                    member_id,
                    order_id,
                    CancelRequest {
                        reason: CancelReason::ChangeOfMind,
                        detail_reason: "회원 범위".to_owned(),
                        idempotency_key: "same-member-scoped-key".to_owned(),
                    },
                    OffsetDateTime::now_utc(),
                )
                .is_ok());
        }
        fixture
            .state
            .orders
            .transact_store(|store| assert_eq!(store.cancellations.len(), 2));
    }

    #[test]
    fn refund_failure_rolls_back_order_cancellation_and_quantity() {
        let fixture = fixture("PAYMENT_COMPLETED", 20_000, 2);
        let quantity = fixture
            .state
            .group_purchases
            .order_test_quantity(fixture.purchase_id)
            .unwrap();
        fixture.state.orders.transact_store(|store| {
            store.point_balances.insert(fixture.buyer_id, i64::MAX);
        });
        let result = fixture.state.orders.request_cancellation(
            &fixture.state.group_purchases,
            fixture.buyer_id,
            fixture.order_id,
            CancelRequest {
                reason: CancelReason::ChangeOfMind,
                detail_reason: "실패".to_owned(),
                idempotency_key: "rollback".to_owned(),
            },
            OffsetDateTime::now_utc(),
        );
        assert!(matches!(result, Err(CancelError::Internal)));
        assert_eq!(
            fixture
                .state
                .group_purchases
                .order_test_quantity(fixture.purchase_id),
            Some(quantity)
        );
        fixture.state.orders.transact_store(|store| {
            assert_eq!(store.orders[0].status, "PAYMENT_COMPLETED");
            assert!(store.cancellations.is_empty());
            assert!(store.point_ledger.is_empty());
        });
    }

    #[test]
    fn retry_and_auto_complete_stale_refunds_once() {
        let retry = fixture("PAYMENT_COMPLETED", 10_000, 1);
        insert_stale_cancellation(&retry, "REQUESTED", "CHANGE_OF_MIND", Duration::minutes(20));
        assert_eq!(
            retry
                .state
                .orders
                .retry_cancellations(OffsetDateTime::now_utc())
                .unwrap()
                .len(),
            1
        );
        assert!(retry
            .state
            .orders
            .retry_cancellations(OffsetDateTime::now_utc())
            .unwrap()
            .is_empty());

        let auto = fixture("PAYMENT_COMPLETED", 10_000, 1);
        insert_stale_cancellation(&auto, "PENDING", "PRODUCT_DEFECT", Duration::days(3));
        assert_eq!(
            auto.state
                .orders
                .auto_approve_cancellations(&auto.state.group_purchases, OffsetDateTime::now_utc())
                .unwrap()
                .len(),
            1
        );
        auto.state.orders.transact_store(|store| {
            assert_eq!(store.cancellations[0].status, "COMPLETED");
            assert_eq!(store.point_ledger.len(), 1);
        });
    }

    fn insert_stale_cancellation(
        fixture: &Fixture,
        status: &'static str,
        reason: &'static str,
        age: Duration,
    ) {
        fixture.state.orders.transact_store(|store| {
            let time = OffsetDateTime::now_utc() - age;
            store.orders[0].status = "CANCELLED";
            store.cancellations.push(CanceledOrder {
                canceled_order_id: Uuid::new_v4(),
                order_id: fixture.order_id,
                member_id: fixture.buyer_id,
                seller_id: fixture.seller_id,
                original_paid_amount: 10_000,
                cancel_fee_amount: 0,
                shipping_fee_amount: 0,
                refund_amount: 10_000,
                policy_id: VOID_POLICY,
                policy_snapshot: "{}".to_owned(),
                status,
                reason,
                detail_reason: "재처리".to_owned(),
                idempotency_key: Uuid::new_v4().to_string(),
                canceled_at: format_time(time),
                returned_at: None,
                created_at: format_time(time),
                updated_at: None,
            });
        });
    }
}
