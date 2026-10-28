use super::*;
use sqlx::{postgres::PgRow, Row};

impl GroupPurchaseRepository {
    pub(crate) async fn persisted_order_title(
        &self,
        purchase_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        Ok(self
            .persisted_find_by_id(purchase_id)
            .await?
            .map(|purchase| purchase.title))
    }

    pub(super) async fn persisted_find_by_id(
        &self,
        purchase_id: Uuid,
    ) -> Result<Option<GroupPurchase>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.find_by_id(purchase_id));
        };
        sqlx::query("SELECT group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at FROM product_schema.group_purchase WHERE group_purchase_id=$1")
            .bind(purchase_id)
            .fetch_optional(pool)
            .await?
            .map(|row| purchase_from_row(&row))
            .transpose()
    }

    pub(super) async fn persisted_find_all(
        &self,
        seller_id: Option<Uuid>,
        page: usize,
        size: usize,
        sort: Option<PurchaseSort>,
    ) -> Result<(Vec<GroupPurchase>, usize), sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.find_all(seller_id, page, size, sort));
        };
        let rows = if let Some(seller_id) = seller_id {
            sqlx::query("SELECT group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at FROM product_schema.group_purchase WHERE seller_id=$1")
                .bind(seller_id)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query("SELECT group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at FROM product_schema.group_purchase")
                .fetch_all(pool)
                .await?
        };
        let mut purchases = rows
            .iter()
            .map(purchase_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(sort) = sort {
            purchases.sort_by(|left, right| purchase_order(left, right, sort));
        }
        let total = purchases.len();
        Ok((
            purchases
                .into_iter()
                .skip(page.saturating_mul(size))
                .take(size)
                .collect(),
            total,
        ))
    }

    pub(super) async fn persisted_create(
        &self,
        new_purchase: NewGroupPurchase,
    ) -> Result<GroupPurchase, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.create(new_purchase));
        };
        let purchase_id = Uuid::new_v4();
        let row = sqlx::query("INSERT INTO product_schema.group_purchase (group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,0,0,$7,$8,$9,$10,$11,'SCHEDULED',NOW(),NOW()) RETURNING group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at")
            .bind(purchase_id)
            .bind(new_purchase.write.min_quantity)
            .bind(new_purchase.write.max_quantity)
            .bind(&new_purchase.write.title)
            .bind(&new_purchase.write.description)
            .bind(new_purchase.write.discounted_price)
            .bind(parse_timestamp(&new_purchase.write.start_date)?)
            .bind(parse_timestamp(&new_purchase.write.end_date)?)
            .bind(new_purchase.seller_id)
            .bind(new_purchase.write.product_id)
            .bind(&new_purchase.write.image_url)
            .fetch_one(pool)
            .await?;
        purchase_from_row(&row)
    }

    pub(super) async fn persisted_update(
        &self,
        purchase_id: Uuid,
        seller_id: Uuid,
        write: GroupPurchaseWrite,
    ) -> Result<UpdateGroupPurchaseResult, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.update(purchase_id, seller_id, write));
        };
        let mut tx = pool.begin().await?;
        let existing = sqlx::query("SELECT group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
            .bind(purchase_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(existing) = existing else {
            return Ok(UpdateGroupPurchaseResult::NotFound);
        };
        let existing = purchase_from_row(&existing)?;
        if existing.seller_id != seller_id {
            return Ok(UpdateGroupPurchaseResult::Forbidden);
        }
        if open_fields_changed(&existing, &write) {
            return Ok(UpdateGroupPurchaseResult::InvalidOpenUpdate);
        }
        let row = sqlx::query("UPDATE product_schema.group_purchase SET min_quantity=$2,max_quantity=$3,title=$4,description=$5,discounted_price=$6,start_date=$7,end_date=$8,product_id=$9,image_url=$10,updated_at=NOW() WHERE group_purchase_id=$1 RETURNING group_purchase_id,min_quantity,max_quantity,title,description,discounted_price,current_quantity,like_count,start_date,end_date,seller_id,product_id,image_url,status,created_at,updated_at")
            .bind(purchase_id)
            .bind(write.min_quantity)
            .bind(write.max_quantity)
            .bind(&write.title)
            .bind(&write.description)
            .bind(write.discounted_price)
            .bind(parse_timestamp(&write.start_date)?)
            .bind(parse_timestamp(&write.end_date)?)
            .bind(write.product_id)
            .bind(&write.image_url)
            .fetch_one(&mut *tx)
            .await?;
        let updated = purchase_from_row(&row)?;
        tx.commit().await?;
        Ok(UpdateGroupPurchaseResult::Updated(Box::new(updated)))
    }

    pub(super) async fn persisted_delete(
        &self,
        purchase_id: Uuid,
        seller_id: Uuid,
    ) -> Result<DeleteGroupPurchaseResult, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.delete(purchase_id, seller_id));
        };
        let mut tx = pool.begin().await?;
        let row = sqlx::query("SELECT status,seller_id FROM product_schema.group_purchase WHERE group_purchase_id=$1 FOR UPDATE")
            .bind(purchase_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Ok(DeleteGroupPurchaseResult::NotFound);
        };
        if row.try_get::<String, _>("status")? != "SCHEDULED" {
            return Ok(DeleteGroupPurchaseResult::InvalidStatus);
        }
        if row.try_get::<Uuid, _>("seller_id")? != seller_id {
            return Ok(DeleteGroupPurchaseResult::Forbidden);
        }
        sqlx::query("DELETE FROM product_schema.group_purchase WHERE group_purchase_id=$1")
            .bind(purchase_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DeleteGroupPurchaseResult::Deleted)
    }

    pub(crate) async fn persisted_cart_availability(
        &self,
        purchase_id: Uuid,
    ) -> Result<Option<bool>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(self.cart_availability(purchase_id));
        };
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT status FROM product_schema.group_purchase WHERE group_purchase_id=$1",
        )
        .bind(purchase_id)
        .fetch_optional(pool)
        .await?
        .map(|status| status == "OPEN"))
    }
}

fn purchase_from_row(row: &PgRow) -> Result<GroupPurchase, sqlx::Error> {
    Ok(GroupPurchase {
        group_purchase_id: row.try_get("group_purchase_id")?,
        min_quantity: row.try_get("min_quantity")?,
        max_quantity: row.try_get("max_quantity")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        discounted_price: row.try_get("discounted_price")?,
        current_quantity: row.try_get("current_quantity")?,
        like_count: row.try_get("like_count")?,
        start_date: format_timestamp(row.try_get("start_date")?)?,
        end_date: format_timestamp(row.try_get("end_date")?)?,
        seller_id: row.try_get("seller_id")?,
        product_id: row.try_get("product_id")?,
        image_url: row.try_get("image_url")?,
        status: row.try_get("status")?,
        created_at: format_timestamp(row.try_get("created_at")?)?,
        updated_at: row
            .try_get::<Option<OffsetDateTime>, _>("updated_at")?
            .map(format_timestamp)
            .transpose()?,
    })
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, sqlx::Error> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, sqlx::Error> {
    value
        .format(&Rfc3339)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}
