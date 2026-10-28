use super::{
    cancel::{resolve_policy, CancelReason},
    CanceledOrder, NewOrder, Order, OrderRepository,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug)]
pub(super) enum CreateError {
    PurchaseNotFound,
    PurchaseNotOpen,
    PurchaseExpired,
    Capacity,
    CartNotFound,
    CartForbidden,
    CartEmpty,
    PartialReplay,
    Database(sqlx::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum LifecycleError {
    InvalidAmount,
    BalanceOverflow,
    OrderNotFound,
    Forbidden,
    InvalidStatus,
    Expired,
    InsufficientPoints,
    DuplicateTransaction,
    Quantity,
    Database,
}

pub(super) struct Charge {
    pub transaction_id: Uuid,
    pub amount: i64,
    pub balance: i64,
}

pub(super) struct Payment {
    pub order_id: Uuid,
    pub transaction_id: Uuid,
    pub amount: i64,
    pub balance: i64,
    pub paid_at: String,
}

pub(super) struct Delivery<'a> {
    pub address: &'a str,
    pub address_detail: &'a str,
    pub postal_code: &'a str,
    pub receiver_name: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ConfirmError {
    NotFound,
    Forbidden,
    InvalidStatus,
    Database,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CancelError {
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
    Database,
}

pub(super) struct Refund {
    pub transaction_id: Uuid,
    pub order_id: Uuid,
    pub amount: i64,
}

impl OrderRepository {
    pub(super) async fn create_direct_postgres(
        &self,
        member_id: Uuid,
        quantity: i32,
        group_purchase_id: Uuid,
        request_id: &str,
        delivery: Delivery<'_>,
    ) -> Result<Order, CreateError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(CreateError::Database)?;
        lock_request(&mut tx, member_id, request_id)
            .await
            .map_err(CreateError::Database)?;
        if let Some(order) = find_by_key(&mut tx, member_id, request_id)
            .await
            .map_err(CreateError::Database)?
        {
            tx.commit().await.map_err(CreateError::Database)?;
            return Ok(order);
        }
        let row = sqlx::query("SELECT discounted_price,seller_id,status,end_date,current_quantity,max_quantity FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
            .bind(group_purchase_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(CreateError::Database)?
            .ok_or(CreateError::PurchaseNotFound)?;
        validate_purchase(&row, quantity)?;
        let order = Order::create(NewOrder {
            quantity,
            unit_price: row.get("discounted_price"),
            member_id,
            address: delivery.address.to_owned(),
            address_detail: delivery.address_detail.to_owned(),
            postal_code: delivery.postal_code.to_owned(),
            receiver_name: delivery.receiver_name.to_owned(),
            seller_id: row.get("seller_id"),
            group_purchase_id,
            idempotency_key: request_id.to_owned(),
        });
        increase_quantity(&mut tx, group_purchase_id, quantity).await?;
        insert_order(&mut tx, &order)
            .await
            .map_err(CreateError::Database)?;
        tx.commit().await.map_err(CreateError::Database)?;
        Ok(order)
    }

    pub(super) async fn create_cart_postgres(
        &self,
        member_id: Uuid,
        cart_ids: &[Uuid],
        request_id: &str,
        delivery: Delivery<'_>,
    ) -> Result<Vec<Order>, CreateError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(CreateError::Database)?;
        lock_request(&mut tx, member_id, request_id)
            .await
            .map_err(CreateError::Database)?;
        let keys = cart_ids
            .iter()
            .map(|cart_id| format!("{request_id}-{cart_id}"))
            .collect::<Vec<_>>();
        let existing_rows = sqlx::query("SELECT order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at FROM order_schema.\"order\" WHERE member_id=$1 AND idempotency_key=ANY($2)")
            .bind(member_id)
            .bind(&keys)
            .fetch_all(&mut *tx)
            .await
            .map_err(CreateError::Database)?;
        if existing_rows.len() == keys.len() {
            let orders = existing_rows
                .iter()
                .map(order_from_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(CreateError::Database)?;
            tx.commit().await.map_err(CreateError::Database)?;
            return Ok(orders);
        }
        if !existing_rows.is_empty() {
            return Err(CreateError::PartialReplay);
        }

        let carts = sqlx::query("SELECT shopping_cart_id,member_id,group_purchase_id,quantity FROM order_schema.shopping_cart WHERE shopping_cart_id=ANY($1) ORDER BY shopping_cart_id FOR UPDATE")
            .bind(cart_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(CreateError::Database)?;
        if carts.len() != cart_ids.len() {
            return Err(CreateError::CartNotFound);
        }
        if carts
            .iter()
            .any(|row| row.get::<Uuid, _>("member_id") != member_id)
        {
            return Err(CreateError::CartForbidden);
        }
        if carts.iter().any(|row| row.get::<i32, _>("quantity") < 1) {
            return Err(CreateError::CartEmpty);
        }

        let mut purchase_ids = carts
            .iter()
            .map(|row| row.get::<Uuid, _>("group_purchase_id"))
            .collect::<Vec<_>>();
        purchase_ids.sort_unstable();
        purchase_ids.dedup();
        sqlx::query("SELECT group_purchase_id FROM product_schema.group_purchase WHERE group_purchase_id=ANY($1) ORDER BY group_purchase_id FOR UPDATE")
            .bind(&purchase_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(CreateError::Database)?;

        let mut orders = Vec::with_capacity(carts.len());
        for cart in carts {
            let purchase_id: Uuid = cart.get("group_purchase_id");
            let quantity: i32 = cart.get("quantity");
            let purchase = sqlx::query("SELECT discounted_price,seller_id,status,end_date,current_quantity,max_quantity FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
                .bind(purchase_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(CreateError::Database)?
                .ok_or(CreateError::PurchaseNotFound)?;
            validate_purchase(&purchase, quantity)?;
            let cart_id: Uuid = cart.get("shopping_cart_id");
            let order = Order::create(NewOrder {
                quantity,
                unit_price: purchase.get("discounted_price"),
                member_id,
                address: delivery.address.to_owned(),
                address_detail: delivery.address_detail.to_owned(),
                postal_code: delivery.postal_code.to_owned(),
                receiver_name: delivery.receiver_name.to_owned(),
                seller_id: purchase.get("seller_id"),
                group_purchase_id: purchase_id,
                idempotency_key: format!("{request_id}-{cart_id}"),
            });
            increase_quantity(&mut tx, purchase_id, quantity).await?;
            insert_order(&mut tx, &order)
                .await
                .map_err(CreateError::Database)?;
            orders.push(order);
        }
        sqlx::query("UPDATE order_schema.shopping_cart SET quantity=0,updated_at=NOW() WHERE shopping_cart_id=ANY($1)")
            .bind(cart_ids)
            .execute(&mut *tx)
            .await
            .map_err(CreateError::Database)?;
        tx.commit().await.map_err(CreateError::Database)?;
        Ok(orders)
    }

    pub(super) async fn charge_postgres(
        &self,
        member_id: Uuid,
        transaction_id: Uuid,
        amount: i64,
    ) -> Result<Charge, LifecycleError> {
        if amount <= 0 {
            return Err(LifecycleError::InvalidAmount);
        }
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| LifecycleError::Database)?;
        sqlx::query("INSERT INTO order_schema.point_account(member_id,balance) VALUES($1,0) ON CONFLICT DO NOTHING")
            .bind(member_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| LifecycleError::Database)?;
        let existing = sqlx::query("SELECT kind,amount,balance_after FROM order_schema.point_ledger WHERE member_id=$1 AND transaction_id=$2")
            .bind(member_id)
            .bind(transaction_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| LifecycleError::Database)?;
        if let Some(existing) = existing {
            if existing.get::<String, _>("kind") != "CHARGE"
                || existing.get::<i64, _>("amount") != amount
            {
                return Err(LifecycleError::DuplicateTransaction);
            }
            return Ok(Charge {
                transaction_id,
                amount,
                balance: existing.get("balance_after"),
            });
        }
        let balance: i64 = sqlx::query_scalar(
            "SELECT balance FROM order_schema.point_account WHERE member_id=$1 FOR UPDATE",
        )
        .bind(member_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| LifecycleError::Database)?;
        let next = balance
            .checked_add(amount)
            .ok_or(LifecycleError::BalanceOverflow)?;
        sqlx::query("UPDATE order_schema.point_account SET balance=$2 WHERE member_id=$1")
            .bind(member_id)
            .bind(next)
            .execute(&mut *tx)
            .await
            .map_err(|_| LifecycleError::Database)?;
        sqlx::query("INSERT INTO order_schema.point_ledger(ledger_id,member_id,transaction_id,kind,amount,balance_after) VALUES($1,$2,$3,'CHARGE',$4,$5)")
            .bind(Uuid::new_v4()).bind(member_id).bind(transaction_id).bind(amount).bind(next)
            .execute(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
        tx.commit().await.map_err(|_| LifecycleError::Database)?;
        Ok(Charge {
            transaction_id,
            amount,
            balance: next,
        })
    }

    pub(super) async fn pay_postgres(
        &self,
        member_id: Uuid,
        order_id: Uuid,
        transaction_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<Payment, LifecycleError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| LifecycleError::Database)?;
        let existing = sqlx::query("SELECT kind,order_id,amount,balance_after FROM order_schema.point_ledger WHERE member_id=$1 AND transaction_id=$2")
            .bind(member_id).bind(transaction_id).fetch_optional(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
        if let Some(entry) = existing {
            if entry.get::<String, _>("kind") != "USE"
                || entry.get::<Option<Uuid>, _>("order_id") != Some(order_id)
            {
                return Err(LifecycleError::DuplicateTransaction);
            }
            let paid_at: OffsetDateTime = sqlx::query_scalar("SELECT paid_at FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL")
                .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| LifecycleError::Database)?
                .flatten().ok_or(LifecycleError::OrderNotFound)?;
            return Ok(Payment {
                order_id,
                transaction_id,
                amount: entry.get("amount"),
                balance: entry.get("balance_after"),
                paid_at: format_time(paid_at),
            });
        }
        let order = sqlx::query("SELECT member_id,status,expired_at,paid_price FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| LifecycleError::Database)?
            .ok_or(LifecycleError::OrderNotFound)?;
        if order.get::<Uuid, _>("member_id") != member_id {
            return Err(LifecycleError::Forbidden);
        }
        if order.get::<String, _>("status") != "PENDING" {
            return Err(LifecycleError::InvalidStatus);
        }
        if order.get::<OffsetDateTime, _>("expired_at") <= now {
            return Err(LifecycleError::Expired);
        }
        let amount: i64 = order.get("paid_price");
        let balance: Option<i64> = sqlx::query_scalar(
            "SELECT balance FROM order_schema.point_account WHERE member_id=$1 FOR UPDATE",
        )
        .bind(member_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| LifecycleError::Database)?;
        let next = balance
            .unwrap_or(0)
            .checked_sub(amount)
            .filter(|value| *value >= 0)
            .ok_or(LifecycleError::InsufficientPoints)?;
        sqlx::query("UPDATE order_schema.point_account SET balance=$2 WHERE member_id=$1")
            .bind(member_id)
            .bind(next)
            .execute(&mut *tx)
            .await
            .map_err(|_| LifecycleError::Database)?;
        sqlx::query("UPDATE order_schema.\"order\" SET status='PAYMENT_COMPLETED',payment_method='POINT',paid_at=$2,updated_at=$2 WHERE order_id=$1")
            .bind(order_id).bind(now).execute(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
        sqlx::query("INSERT INTO order_schema.point_ledger(ledger_id,member_id,transaction_id,order_id,kind,amount,balance_after,created_at) VALUES($1,$2,$3,$4,'USE',$5,$6,$7)")
            .bind(Uuid::new_v4()).bind(member_id).bind(transaction_id).bind(order_id).bind(amount).bind(next).bind(now)
            .execute(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
        tx.commit().await.map_err(|_| LifecycleError::Database)?;
        Ok(Payment {
            order_id,
            transaction_id,
            amount,
            balance: next,
            paid_at: format_time(now),
        })
    }

    pub(super) async fn expire_postgres(&self, now: OffsetDateTime) -> Result<(), LifecycleError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT order_id FROM order_schema.\"order\" WHERE status='PENDING' AND deleted_at IS NULL AND expired_at < $1")
            .bind(now).fetch_all(pool).await.map_err(|_| LifecycleError::Database)?;
        for order_id in ids {
            let mut tx = pool.begin().await.map_err(|_| LifecycleError::Database)?;
            let row = sqlx::query("SELECT status,expired_at,group_purchase_id,quantity FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL FOR UPDATE")
                .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
            let Some(row) = row else { continue };
            if row.get::<String, _>("status") != "PENDING"
                || row.get::<OffsetDateTime, _>("expired_at") >= now
            {
                continue;
            }
            let updated = sqlx::query("UPDATE product_schema.group_purchase SET current_quantity=current_quantity-$2,updated_at=$3 WHERE group_purchase_id=$1 AND current_quantity >= $2")
                .bind(row.get::<Uuid, _>("group_purchase_id")).bind(row.get::<i32, _>("quantity")).bind(now)
                .execute(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
            if updated.rows_affected() != 1 {
                return Err(LifecycleError::Quantity);
            }
            sqlx::query("UPDATE order_schema.\"order\" SET status='EXPIRED',updated_at=$2 WHERE order_id=$1")
                .bind(order_id).bind(now).execute(&mut *tx).await.map_err(|_| LifecycleError::Database)?;
            tx.commit().await.map_err(|_| LifecycleError::Database)?;
        }
        Ok(())
    }

    pub(super) async fn find_postgres(&self, order_id: Uuid) -> Result<Option<Order>, sqlx::Error> {
        let pool = self.pool().expect("PostgreSQL repository required");
        sqlx::query("SELECT order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL")
            .bind(order_id).fetch_optional(pool).await?.map(|row| order_from_row(&row)).transpose()
    }

    pub(super) async fn list_postgres(
        &self,
        member_id: Uuid,
        seller: bool,
    ) -> Result<Vec<Order>, sqlx::Error> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let rows = if seller {
            sqlx::query("SELECT order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at FROM order_schema.\"order\" WHERE seller_id=$1 AND deleted_at IS NULL").bind(member_id).fetch_all(pool).await?
        } else {
            sqlx::query("SELECT order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at FROM order_schema.\"order\" WHERE member_id=$1 AND deleted_at IS NULL").bind(member_id).fetch_all(pool).await?
        };
        rows.iter().map(order_from_row).collect()
    }

    pub(super) async fn participants_postgres(
        &self,
        purchase_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar("SELECT member_id FROM order_schema.\"order\" WHERE group_purchase_id=$1 AND status='PAYMENT_COMPLETED' AND deleted_at IS NULL")
            .bind(purchase_id).fetch_all(self.pool().expect("PostgreSQL repository required")).await
    }

    pub(super) async fn confirm_postgres(
        &self,
        member_id: Uuid,
        order_id: Uuid,
    ) -> Result<(), ConfirmError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| ConfirmError::Database)?;
        let row = sqlx::query("SELECT member_id,status,seller_id,paid_price FROM order_schema.\"order\" WHERE order_id=$1 FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| ConfirmError::Database)?.ok_or(ConfirmError::NotFound)?;
        if row.get::<Uuid, _>("member_id") != member_id {
            return Err(ConfirmError::Forbidden);
        }
        if row.get::<String, _>("status") != "PAYMENT_COMPLETED" {
            return Err(ConfirmError::InvalidStatus);
        }
        sqlx::query("INSERT INTO order_schema.order_settlement(settlement_id,order_id,seller_id,amount,status,created_at) VALUES($1,$2,$3,$4,'COMPLETED',NOW()) ON CONFLICT(order_id) DO NOTHING")
            .bind(Uuid::new_v4()).bind(order_id).bind(row.get::<Uuid, _>("seller_id")).bind(row.get::<i64, _>("paid_price"))
            .execute(&mut *tx).await.map_err(|_| ConfirmError::Database)?;
        sqlx::query("UPDATE order_schema.\"order\" SET status='CONFIRMED',updated_at=NOW() WHERE order_id=$1")
            .bind(order_id).execute(&mut *tx).await.map_err(|_| ConfirmError::Database)?;
        tx.commit().await.map_err(|_| ConfirmError::Database)?;
        Ok(())
    }

    pub(super) async fn request_cancel_postgres(
        &self,
        member_id: Uuid,
        order_id: Uuid,
        reason: CancelReason,
        detail_reason: &str,
        idempotency_key: &str,
        now: OffsetDateTime,
    ) -> Result<(CanceledOrder, Option<Refund>), CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| CancelError::Database)?;
        lock_request(&mut tx, member_id, idempotency_key)
            .await
            .map_err(|_| CancelError::Database)?;
        let order = sqlx::query("SELECT member_id,seller_id,status,paid_price,group_purchase_id,quantity FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::OrderNotFound)?;
        if order.get::<Uuid, _>("member_id") != member_id {
            return Err(CancelError::BuyerForbidden);
        }
        if let Some(row) = sqlx::query("SELECT cancellation_id,order_id,member_id,seller_id,original_paid_amount,cancel_fee_amount,shipping_fee_amount,refund_amount,policy_id,policy_snapshot,status,reason,detail_reason,idempotency_key,canceled_at,returned_at,created_at,updated_at FROM order_schema.canceled_order WHERE order_id=$1")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)? {
            return Ok((cancellation_from_row(&row)?, None));
        }
        let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM order_schema.canceled_order WHERE member_id=$1 AND idempotency_key=$2)")
            .bind(member_id).bind(idempotency_key).fetch_one(&mut *tx).await.map_err(|_| CancelError::Database)?;
        if duplicate {
            return Err(CancelError::DuplicateRequest);
        }
        if order.get::<String, _>("status") != "PAYMENT_COMPLETED" {
            return Err(CancelError::InvalidOrderStatus);
        }
        if !reason.is_buyer_fault() && !reason.is_seller_fault() {
            return Err(CancelError::CancellationNotAllowed);
        }
        let purchase_id: Uuid = order.get("group_purchase_id");
        let purchase = sqlx::query("SELECT current_quantity,succeeded_at FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
            .bind(purchase_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::PurchaseNotFound)?;
        let succeeded_at = purchase
            .get::<Option<OffsetDateTime>, _>("succeeded_at")
            .map(format_time);
        let policy = resolve_policy(
            reason,
            order.get("paid_price"),
            succeeded_at.as_deref(),
            now,
        )
        .map_err(|_| CancelError::CancellationNotAllowed)?;
        let immediate = reason.is_buyer_fault();
        if immediate {
            decrease_quantity(&mut tx, purchase_id, order.get("quantity")).await?;
        }
        let cancellation_id = Uuid::new_v4();
        let status = if immediate { "REQUESTED" } else { "PENDING" };
        let snapshot = serde_json::json!({"policyId": policy.id, "originalPaidAmount": order.get::<i64,_>("paid_price"), "cancelFeeAmount": policy.fee, "shippingFeeAmount": policy.shipping, "refundAmount": policy.refund});
        sqlx::query("INSERT INTO order_schema.canceled_order(cancellation_id,order_id,member_id,seller_id,idempotency_key,reason,detail_reason,original_paid_amount,cancel_fee_amount,shipping_fee_amount,refund_amount,policy_id,policy_snapshot,status,canceled_at,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15)")
            .bind(cancellation_id).bind(order_id).bind(member_id).bind(order.get::<Uuid,_>("seller_id")).bind(idempotency_key).bind(reason.as_str()).bind(detail_reason)
            .bind(order.get::<i64,_>("paid_price")).bind(policy.fee).bind(policy.shipping).bind(policy.refund).bind(policy.id).bind(&snapshot).bind(status).bind(now)
            .execute(&mut *tx).await.map_err(|_| CancelError::Database)?;
        sqlx::query(
            "UPDATE order_schema.\"order\" SET status='CANCELLED',updated_at=$2 WHERE order_id=$1",
        )
        .bind(order_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|_| CancelError::Database)?;
        let refund = if immediate {
            Some(complete_refund_postgres(&mut tx, cancellation_id, now).await?)
        } else {
            None
        };
        let row = cancellation_row(&mut tx, cancellation_id).await?;
        tx.commit().await.map_err(|_| CancelError::Database)?;
        Ok((cancellation_from_row(&row)?, refund))
    }

    pub(super) async fn list_cancellations_postgres(
        &self,
        member_id: Uuid,
        seller_pending: bool,
    ) -> Result<Vec<CanceledOrder>, CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let rows = if seller_pending {
            sqlx::query("SELECT c.cancellation_id,c.order_id,c.member_id,c.seller_id,c.original_paid_amount,c.cancel_fee_amount,c.shipping_fee_amount,c.refund_amount,c.policy_id,c.policy_snapshot,c.status,c.reason,c.detail_reason,c.idempotency_key,c.canceled_at,c.returned_at,c.created_at,c.updated_at FROM order_schema.canceled_order c JOIN order_schema.\"order\" o ON o.order_id=c.order_id WHERE c.seller_id=$1 AND c.status='PENDING' AND o.deleted_at IS NULL")
                .bind(member_id).fetch_all(pool).await.map_err(|_| CancelError::Database)?
        } else {
            sqlx::query("SELECT c.cancellation_id,c.order_id,c.member_id,c.seller_id,c.original_paid_amount,c.cancel_fee_amount,c.shipping_fee_amount,c.refund_amount,c.policy_id,c.policy_snapshot,c.status,c.reason,c.detail_reason,c.idempotency_key,c.canceled_at,c.returned_at,c.created_at,c.updated_at FROM order_schema.canceled_order c JOIN order_schema.\"order\" o ON o.order_id=c.order_id WHERE c.member_id=$1 AND o.deleted_at IS NULL")
                .bind(member_id).fetch_all(pool).await.map_err(|_| CancelError::Database)?
        };
        rows.iter().map(cancellation_from_row).collect()
    }

    pub(super) async fn approve_cancel_postgres(
        &self,
        seller_id: Uuid,
        order_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<(CanceledOrder, Refund), CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| CancelError::Database)?;
        let cancellation = sqlx::query("SELECT cancellation_id,status FROM order_schema.canceled_order WHERE order_id=$1 FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::CancellationNotFound)?;
        let order = sqlx::query("SELECT seller_id,group_purchase_id,quantity FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::OrderNotFound)?;
        if order.get::<Uuid, _>("seller_id") != seller_id {
            return Err(CancelError::SellerForbidden);
        }
        if cancellation.get::<String, _>("status") != "PENDING" {
            return Err(CancelError::InvalidCancelStatus);
        }
        let purchase_id: Uuid = order.get("group_purchase_id");
        sqlx::query("SELECT group_purchase_id FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
            .bind(purchase_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::PurchaseNotFound)?;
        decrease_quantity(&mut tx, purchase_id, order.get("quantity")).await?;
        let cancellation_id: Uuid = cancellation.get("cancellation_id");
        let refund = complete_refund_postgres(&mut tx, cancellation_id, now).await?;
        let row = cancellation_row(&mut tx, cancellation_id).await?;
        tx.commit().await.map_err(|_| CancelError::Database)?;
        Ok((cancellation_from_row(&row)?, refund))
    }

    pub(super) async fn reject_cancel_postgres(
        &self,
        seller_id: Uuid,
        order_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<CanceledOrder, CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let mut tx = pool.begin().await.map_err(|_| CancelError::Database)?;
        let cancellation = sqlx::query("SELECT cancellation_id,status FROM order_schema.canceled_order WHERE order_id=$1 FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?.ok_or(CancelError::CancellationNotFound)?;
        let owner: Option<Uuid> = sqlx::query_scalar("SELECT seller_id FROM order_schema.\"order\" WHERE order_id=$1 AND deleted_at IS NULL FOR UPDATE")
            .bind(order_id).fetch_optional(&mut *tx).await.map_err(|_| CancelError::Database)?;
        if owner.ok_or(CancelError::OrderNotFound)? != seller_id {
            return Err(CancelError::SellerForbidden);
        }
        if cancellation.get::<String, _>("status") != "PENDING" {
            return Err(CancelError::InvalidCancelStatus);
        }
        let cancellation_id: Uuid = cancellation.get("cancellation_id");
        sqlx::query("UPDATE order_schema.canceled_order SET status='REJECTED',updated_at=$2 WHERE cancellation_id=$1")
            .bind(cancellation_id).bind(now).execute(&mut *tx).await.map_err(|_| CancelError::Database)?;
        sqlx::query("UPDATE order_schema.\"order\" SET status='PAYMENT_COMPLETED',updated_at=$2 WHERE order_id=$1")
            .bind(order_id).bind(now).execute(&mut *tx).await.map_err(|_| CancelError::Database)?;
        let row = cancellation_row(&mut tx, cancellation_id).await?;
        tx.commit().await.map_err(|_| CancelError::Database)?;
        cancellation_from_row(&row)
    }

    pub(super) async fn retry_cancellations_postgres(
        &self,
        now: OffsetDateTime,
    ) -> Result<Vec<Refund>, CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT cancellation_id FROM order_schema.canceled_order WHERE status IN ('REQUESTED','APPROVED') AND canceled_at < $1")
            .bind(now - Duration::minutes(15))
            .fetch_all(pool)
            .await
            .map_err(|_| CancelError::Database)?;
        let mut refunds = Vec::with_capacity(ids.len());
        for id in ids {
            let mut tx = pool.begin().await.map_err(|_| CancelError::Database)?;
            match complete_refund_postgres(&mut tx, id, now).await {
                Ok(refund) => {
                    tx.commit().await.map_err(|_| CancelError::Database)?;
                    refunds.push(refund);
                }
                Err(CancelError::InvalidCancelStatus) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(refunds)
    }

    pub(super) async fn auto_approve_cancellations_postgres(
        &self,
        now: OffsetDateTime,
    ) -> Result<Vec<Refund>, CancelError> {
        let pool = self.pool().expect("PostgreSQL repository required");
        let targets = sqlx::query("SELECT c.order_id,c.seller_id FROM order_schema.canceled_order c WHERE c.status='PENDING' AND c.canceled_at < $1")
            .bind(now - Duration::days(2))
            .fetch_all(pool)
            .await
            .map_err(|_| CancelError::Database)?;
        let mut refunds = Vec::with_capacity(targets.len());
        for target in targets {
            match self
                .approve_cancel_postgres(target.get("seller_id"), target.get("order_id"), now)
                .await
            {
                Ok((_, refund)) => refunds.push(refund),
                Err(CancelError::InvalidCancelStatus) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(refunds)
    }
}

async fn decrease_quantity(
    tx: &mut Transaction<'_, Postgres>,
    purchase_id: Uuid,
    quantity: i32,
) -> Result<(), CancelError> {
    let result = sqlx::query("UPDATE product_schema.group_purchase SET current_quantity=current_quantity-$2,updated_at=NOW() WHERE group_purchase_id=$1 AND current_quantity >= $2")
        .bind(purchase_id).bind(quantity).execute(&mut **tx).await.map_err(|_| CancelError::Database)?;
    if result.rows_affected() != 1 {
        return Err(CancelError::Quantity);
    }
    Ok(())
}

async fn complete_refund_postgres(
    tx: &mut Transaction<'_, Postgres>,
    cancellation_id: Uuid,
    now: OffsetDateTime,
) -> Result<Refund, CancelError> {
    let row = sqlx::query("SELECT order_id,member_id,seller_id,refund_amount,cancel_fee_amount,reason,status FROM order_schema.canceled_order WHERE cancellation_id=$1 FOR UPDATE")
        .bind(cancellation_id).fetch_one(&mut **tx).await.map_err(|_| CancelError::Database)?;
    if row.get::<String, _>("status") == "COMPLETED" {
        return Err(CancelError::InvalidCancelStatus);
    }
    let order_id: Uuid = row.get("order_id");
    let refunded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM order_schema.point_ledger WHERE order_id=$1 AND kind='REFUND')")
        .bind(order_id).fetch_one(&mut **tx).await.map_err(|_| CancelError::Database)?;
    if refunded {
        return Err(CancelError::InvalidCancelStatus);
    }
    let member_id: Uuid = row.get("member_id");
    sqlx::query("INSERT INTO order_schema.point_account(member_id,balance) VALUES($1,0) ON CONFLICT DO NOTHING")
        .bind(member_id).execute(&mut **tx).await.map_err(|_| CancelError::Database)?;
    let balance: i64 = sqlx::query_scalar(
        "SELECT balance FROM order_schema.point_account WHERE member_id=$1 FOR UPDATE",
    )
    .bind(member_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| CancelError::Database)?;
    let amount: i64 = row.get("refund_amount");
    let next = balance.checked_add(amount).ok_or(CancelError::Database)?;
    let transaction_id = Uuid::new_v4();
    sqlx::query("UPDATE order_schema.point_account SET balance=$2 WHERE member_id=$1")
        .bind(member_id)
        .bind(next)
        .execute(&mut **tx)
        .await
        .map_err(|_| CancelError::Database)?;
    sqlx::query("INSERT INTO order_schema.point_ledger(ledger_id,member_id,transaction_id,order_id,kind,amount,balance_after,created_at) VALUES($1,$2,$1,$3,'REFUND',$4,$5,$6)")
        .bind(transaction_id).bind(member_id).bind(order_id).bind(amount).bind(next).bind(now).execute(&mut **tx).await.map_err(|_| CancelError::Database)?;
    if row.get::<String, _>("reason") == "CHANGE_OF_MIND" {
        sqlx::query("INSERT INTO order_schema.order_settlement(settlement_id,order_id,seller_id,amount,status,created_at) VALUES($1,$2,$3,$4,'BUYER_CANCEL',$5) ON CONFLICT(order_id) DO NOTHING")
            .bind(Uuid::new_v4()).bind(order_id).bind(row.get::<Uuid,_>("seller_id")).bind(row.get::<i64,_>("cancel_fee_amount")).bind(now)
            .execute(&mut **tx).await.map_err(|_| CancelError::Database)?;
    }
    sqlx::query("UPDATE order_schema.canceled_order SET status='COMPLETED',returned_at=$2,updated_at=$2 WHERE cancellation_id=$1")
        .bind(cancellation_id).bind(now).execute(&mut **tx).await.map_err(|_| CancelError::Database)?;
    Ok(Refund {
        transaction_id,
        order_id,
        amount,
    })
}

async fn cancellation_row(
    tx: &mut Transaction<'_, Postgres>,
    cancellation_id: Uuid,
) -> Result<PgRow, CancelError> {
    sqlx::query("SELECT cancellation_id,order_id,member_id,seller_id,original_paid_amount,cancel_fee_amount,shipping_fee_amount,refund_amount,policy_id,policy_snapshot,status,reason,detail_reason,idempotency_key,canceled_at,returned_at,created_at,updated_at FROM order_schema.canceled_order WHERE cancellation_id=$1")
        .bind(cancellation_id).fetch_one(&mut **tx).await.map_err(|_| CancelError::Database)
}

fn cancellation_from_row(row: &PgRow) -> Result<CanceledOrder, CancelError> {
    Ok(CanceledOrder {
        canceled_order_id: row.get("cancellation_id"),
        order_id: row.get("order_id"),
        member_id: row.get("member_id"),
        seller_id: row.get("seller_id"),
        original_paid_amount: row.get("original_paid_amount"),
        cancel_fee_amount: row.get("cancel_fee_amount"),
        shipping_fee_amount: row.get("shipping_fee_amount"),
        refund_amount: row.get("refund_amount"),
        policy_id: cancellation_policy(row.get::<String, _>("policy_id").as_str())?,
        policy_snapshot: row
            .get::<serde_json::Value, _>("policy_snapshot")
            .to_string(),
        status: cancellation_status(row.get::<String, _>("status").as_str())?,
        reason: cancellation_reason(row.get::<String, _>("reason").as_str())?,
        detail_reason: row
            .get::<Option<String>, _>("detail_reason")
            .unwrap_or_default(),
        idempotency_key: row.get("idempotency_key"),
        canceled_at: format_time(row.get("canceled_at")),
        returned_at: row
            .get::<Option<OffsetDateTime>, _>("returned_at")
            .map(format_time),
        created_at: format_time(row.get("created_at")),
        updated_at: row
            .get::<Option<OffsetDateTime>, _>("updated_at")
            .map(format_time),
    })
}

fn cancellation_status(value: &str) -> Result<&'static str, CancelError> {
    match value {
        "REQUESTED" => Ok("REQUESTED"),
        "PENDING" => Ok("PENDING"),
        "COMPLETED" => Ok("COMPLETED"),
        "REJECTED" => Ok("REJECTED"),
        _ => Err(CancelError::Database),
    }
}
fn cancellation_reason(value: &str) -> Result<&'static str, CancelError> {
    match value {
        "GROUP_PURCHASE_FAILED" => Ok("GROUP_PURCHASE_FAILED"),
        "CHANGE_OF_MIND" => Ok("CHANGE_OF_MIND"),
        "PRODUCT_DEFECT" => Ok("PRODUCT_DEFECT"),
        "DELIVERY_DELAY" => Ok("DELIVERY_DELAY"),
        "OUT_OF_STOCK" => Ok("OUT_OF_STOCK"),
        _ => Err(CancelError::Database),
    }
}
fn cancellation_policy(value: &str) -> Result<&'static str, CancelError> {
    match value {
        "CANCEL_POLICY_VOID_V1" => Ok("CANCEL_POLICY_VOID_V1"),
        "CANCEL_POLICY_REVERSAL_V1" => Ok("CANCEL_POLICY_REVERSAL_V1"),
        "CANCEL_POLICY_REFUND_V1" => Ok("CANCEL_POLICY_REFUND_V1"),
        _ => Err(CancelError::Database),
    }
}

async fn lock_request(
    tx: &mut Transaction<'_, Postgres>,
    member_id: Uuid,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("{member_id}:{key}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn find_by_key(
    tx: &mut Transaction<'_, Postgres>,
    member_id: Uuid,
    key: &str,
) -> Result<Option<Order>, sqlx::Error> {
    sqlx::query("SELECT order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at FROM order_schema.\"order\" WHERE member_id=$1 AND idempotency_key=$2")
        .bind(member_id).bind(key).fetch_optional(&mut **tx).await?.map(|row| order_from_row(&row)).transpose()
}

fn validate_purchase(row: &PgRow, quantity: i32) -> Result<(), CreateError> {
    match row.get::<String, _>("status").as_str() {
        "OPEN" => {}
        "SUCCESS" => return Err(CreateError::Capacity),
        _ => return Err(CreateError::PurchaseNotOpen),
    }
    if row.get::<OffsetDateTime, _>("end_date") <= OffsetDateTime::now_utc() {
        return Err(CreateError::PurchaseExpired);
    }
    if row
        .get::<i32, _>("current_quantity")
        .checked_add(quantity)
        .is_none_or(|value| value > row.get("max_quantity"))
    {
        return Err(CreateError::Capacity);
    }
    Ok(())
}

async fn increase_quantity(
    tx: &mut Transaction<'_, Postgres>,
    purchase_id: Uuid,
    quantity: i32,
) -> Result<(), CreateError> {
    let updated = sqlx::query("UPDATE product_schema.group_purchase SET current_quantity=current_quantity+$2,status=CASE WHEN current_quantity+$2=max_quantity THEN 'SUCCESS' ELSE status END,succeeded_at=CASE WHEN current_quantity+$2=max_quantity THEN NOW() ELSE succeeded_at END,updated_at=NOW() WHERE group_purchase_id=$1 AND current_quantity+$2 <= max_quantity")
        .bind(purchase_id).bind(quantity).execute(&mut **tx).await.map_err(CreateError::Database)?;
    if updated.rows_affected() != 1 {
        return Err(CreateError::Capacity);
    }
    Ok(())
}

async fn insert_order(
    tx: &mut Transaction<'_, Postgres>,
    order: &Order,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO order_schema.\"order\"(order_id,order_number,quantity,price,paid_price,status,payment_method,paid_at,member_id,address,address_detail,postal_code,receiver_name,seller_id,group_purchase_id,idempotency_key,expired_at,created_at,updated_at,deleted_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)")
        .bind(order.order_id).bind(&order.order_number).bind(order.quantity).bind(order.price).bind(order.paid_price)
        .bind(order.status).bind(order.payment_method).bind(order.paid_at.as_deref().map(parse_time).transpose()?)
        .bind(order.member_id).bind(&order.address).bind(&order.address_detail).bind(&order.postal_code).bind(&order.receiver_name)
        .bind(order.seller_id).bind(order.group_purchase_id).bind(&order.idempotency_key).bind(parse_time(&order.expired_at)?)
        .bind(parse_time(&order.created_at)?).bind(order.updated_at.as_deref().map(parse_time).transpose()?)
        .bind(order.deleted_at.as_deref().map(parse_time).transpose()?).execute(&mut **tx).await?;
    Ok(())
}

fn order_from_row(row: &PgRow) -> Result<Order, sqlx::Error> {
    Ok(Order {
        order_id: row.get("order_id"),
        order_number: row.get("order_number"),
        quantity: row.get("quantity"),
        price: row.get("price"),
        paid_price: row.get("paid_price"),
        status: status(row.get::<String, _>("status").as_str())?,
        payment_method: row
            .get::<Option<String>, _>("payment_method")
            .as_deref()
            .map(payment_method)
            .transpose()?,
        paid_at: row
            .get::<Option<OffsetDateTime>, _>("paid_at")
            .map(format_time),
        member_id: row.get("member_id"),
        address: row.get("address"),
        address_detail: row.get("address_detail"),
        postal_code: row.get("postal_code"),
        receiver_name: row.get("receiver_name"),
        seller_id: row.get("seller_id"),
        group_purchase_id: row.get("group_purchase_id"),
        idempotency_key: row.get("idempotency_key"),
        expired_at: format_time(row.get("expired_at")),
        created_at: format_time(row.get("created_at")),
        updated_at: row
            .get::<Option<OffsetDateTime>, _>("updated_at")
            .map(format_time),
        deleted_at: row
            .get::<Option<OffsetDateTime>, _>("deleted_at")
            .map(format_time),
    })
}

fn status(value: &str) -> Result<&'static str, sqlx::Error> {
    match value {
        "PENDING" => Ok("PENDING"),
        "PAYMENT_COMPLETED" => Ok("PAYMENT_COMPLETED"),
        "EXPIRED" => Ok("EXPIRED"),
        "CONFIRMED" => Ok("CONFIRMED"),
        "CANCELED" => Ok("CANCELED"),
        "CANCELLED" => Ok("CANCELLED"),
        "CANCEL_REQUESTED" => Ok("CANCEL_REQUESTED"),
        _ => Err(sqlx::Error::Decode(
            format!("unknown order status: {value}").into(),
        )),
    }
}

fn payment_method(value: &str) -> Result<&'static str, sqlx::Error> {
    match value {
        "POINT" => Ok("POINT"),
        _ => Err(sqlx::Error::Decode(
            format!("unknown payment method: {value}").into(),
        )),
    }
}

fn parse_time(value: &str) -> Result<OffsetDateTime, sqlx::Error> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .expect("RFC3339 formatting must succeed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn postgres_serializes_capacity_and_payment_expiration_races() {
        let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&database_url)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let seller_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        sqlx::query("INSERT INTO product_schema.product(product_id,idempotency_key,name,price,category,description,stock,seller_id,created_at) VALUES($1,$2,'race product',1000,'ETC','race',10,$3,NOW())")
            .bind(product_id).bind(Uuid::new_v4().to_string()).bind(seller_id).execute(&pool).await.unwrap();

        let capacity_purchase = seed_purchase(&pool, product_id, seller_id, 1).await;
        let repository = OrderRepository::postgres(pool.clone());
        let member_id = Uuid::new_v4();
        let first = {
            let repository = repository.clone();
            tokio::spawn(async move {
                repository
                    .create_direct_postgres(
                        member_id,
                        1,
                        capacity_purchase,
                        "capacity-a",
                        delivery(),
                    )
                    .await
            })
        };
        let second = {
            let repository = repository.clone();
            tokio::spawn(async move {
                repository
                    .create_direct_postgres(
                        member_id,
                        1,
                        capacity_purchase,
                        "capacity-b",
                        delivery(),
                    )
                    .await
            })
        };
        let results = [first.await.unwrap(), second.await.unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(CreateError::Capacity)))
                .count(),
            1
        );
        let quantity: i32 = sqlx::query_scalar(
            "SELECT current_quantity FROM product_schema.group_purchase WHERE group_purchase_id=$1",
        )
        .bind(capacity_purchase)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(quantity, 1);

        let payment_purchase = seed_purchase(&pool, product_id, seller_id, 1).await;
        let order = repository
            .create_direct_postgres(
                member_id,
                1,
                payment_purchase,
                "payment-expiration",
                delivery(),
            )
            .await
            .unwrap();
        repository
            .charge_postgres(member_id, Uuid::new_v4(), 2_000)
            .await
            .unwrap();
        let boundary = OffsetDateTime::now_utc();
        sqlx::query("UPDATE order_schema.\"order\" SET expired_at=$2 WHERE order_id=$1")
            .bind(order.order_id)
            .bind(boundary)
            .execute(&pool)
            .await
            .unwrap();
        let payment = {
            let repository = repository.clone();
            tokio::spawn(async move {
                repository
                    .pay_postgres(
                        member_id,
                        order.order_id,
                        Uuid::new_v4(),
                        boundary - Duration::seconds(1),
                    )
                    .await
            })
        };
        let expiration = {
            let repository = repository.clone();
            tokio::spawn(async move {
                repository
                    .expire_postgres(boundary + Duration::seconds(1))
                    .await
            })
        };
        let payment = payment.await.unwrap();
        expiration.await.unwrap().unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM order_schema.\"order\" WHERE order_id=$1")
                .bind(order.order_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let quantity: i32 = sqlx::query_scalar(
            "SELECT current_quantity FROM product_schema.group_purchase WHERE group_purchase_id=$1",
        )
        .bind(payment_purchase)
        .fetch_one(&pool)
        .await
        .unwrap();
        match status.as_str() {
            "PAYMENT_COMPLETED" => {
                assert!(payment.is_ok());
                assert_eq!(quantity, 1);
            }
            "EXPIRED" => {
                assert!(matches!(payment, Err(LifecycleError::InvalidStatus)));
                assert_eq!(quantity, 0);
            }
            other => panic!("unexpected order status: {other}"),
        }
    }

    fn delivery() -> Delivery<'static> {
        Delivery {
            address: "서울시 테스트 주소",
            address_detail: "101호",
            postal_code: "12345",
            receiver_name: "구매자",
        }
    }

    async fn seed_purchase(
        pool: &sqlx::PgPool,
        product_id: Uuid,
        seller_id: Uuid,
        max: i32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO product_schema.group_purchase(group_purchase_id,min_quantity,max_quantity,title,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,status,created_at) VALUES($1,1,$2,'race purchase',1000,0,0,NOW()-INTERVAL '1 hour',NOW()+INTERVAL '1 hour',$3,$4,'OPEN',NOW())")
            .bind(id).bind(max).bind(seller_id).bind(product_id).execute(pool).await.unwrap();
        id
    }
}
