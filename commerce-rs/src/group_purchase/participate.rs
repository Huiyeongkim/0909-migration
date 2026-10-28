use super::GroupPurchaseRepository;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired by the order-create migration unit")
)]
pub enum ParticipateError {
    CapacityReached,
    ProductNotFound,
}

#[derive(Clone, Copy)]
pub(crate) struct OrderParticipation {
    pub(crate) group_purchase_id: Uuid,
    pub(crate) quantity: i32,
}

#[derive(Clone)]
pub(crate) struct OrderPurchaseSnapshot {
    pub(crate) group_purchase_id: Uuid,
    pub(crate) discounted_price: i64,
    pub(crate) seller_id: Uuid,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OrderParticipationError {
    NotFound,
    NotOpen,
    Expired,
    CapacityReached,
    ProductNotFound,
}

pub(crate) enum OrderParticipationTransactionError<E> {
    Participation(OrderParticipationError),
    Commit(E),
}

#[derive(Clone)]
pub(crate) struct OrderCancellationSnapshot {
    pub(crate) group_purchase_id: Uuid,
    pub(crate) succeeded_at: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct OrderCancellation {
    pub(crate) group_purchase_id: Uuid,
    pub(crate) quantity: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OrderCancellationError {
    NotFound,
    Quantity,
}

pub(crate) enum OrderCancellationTransactionError<E> {
    Cancellation(OrderCancellationError),
    Commit(E),
}

impl GroupPurchaseRepository {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "wired by the order-create migration unit")
    )]
    pub fn participate(
        &self,
        purchase_id: Uuid,
        quantity: i32,
        product_exists: impl FnOnce(Uuid) -> bool,
    ) -> Result<(), ParticipateError> {
        let mut purchases = self.purchases.write().unwrap();
        let mut success_times = self.success_times.write().unwrap();
        let purchase = purchases
            .iter_mut()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .ok_or(ParticipateError::CapacityReached)?;
        let next_quantity = purchase
            .current_quantity
            .checked_add(quantity)
            .filter(|next| purchase.status == "OPEN" && *next <= purchase.max_quantity)
            .ok_or(ParticipateError::CapacityReached)?;
        if !product_exists(purchase.product_id) {
            return Err(ParticipateError::ProductNotFound);
        }

        purchase.current_quantity = next_quantity;
        if next_quantity == purchase.max_quantity {
            purchase.status = "SUCCESS".to_owned();
            let succeeded_at = super::now();
            purchase.updated_at = Some(succeeded_at.clone());
            success_times.insert(purchase.group_purchase_id, succeeded_at);
        }
        Ok(())
    }

    pub(crate) fn with_order_participation<T, E>(
        &self,
        requests: &[OrderParticipation],
        product_exists: impl Fn(Uuid) -> bool,
        commit: impl FnOnce(&[OrderPurchaseSnapshot]) -> Result<T, E>,
    ) -> Result<T, OrderParticipationTransactionError<E>> {
        let mut purchases = self.purchases.write().unwrap();
        let mut success_times = self.success_times.write().unwrap();
        let original = purchases.clone();
        let original_success_times = success_times.clone();
        let result = requests
            .iter()
            .map(|request| {
                let purchase = purchases
                    .iter_mut()
                    .find(|purchase| purchase.group_purchase_id == request.group_purchase_id)
                    .ok_or(OrderParticipationError::NotFound)?;
                if purchase.status != "OPEN" {
                    return Err(if purchase.status == "SUCCESS" {
                        OrderParticipationError::CapacityReached
                    } else {
                        OrderParticipationError::NotOpen
                    });
                }
                if OffsetDateTime::parse(&purchase.end_date, &Rfc3339)
                    .is_ok_and(|end_date| end_date < OffsetDateTime::now_utc())
                {
                    return Err(OrderParticipationError::Expired);
                }
                let next_quantity = purchase
                    .current_quantity
                    .checked_add(request.quantity)
                    .filter(|next| *next <= purchase.max_quantity)
                    .ok_or(OrderParticipationError::CapacityReached)?;
                if !product_exists(purchase.product_id) {
                    return Err(OrderParticipationError::ProductNotFound);
                }
                purchase.current_quantity = next_quantity;
                if next_quantity == purchase.max_quantity {
                    purchase.status = "SUCCESS".to_owned();
                    let succeeded_at = super::now();
                    purchase.updated_at = Some(succeeded_at.clone());
                    success_times.insert(purchase.group_purchase_id, succeeded_at);
                }
                Ok(OrderPurchaseSnapshot {
                    group_purchase_id: purchase.group_purchase_id,
                    discounted_price: purchase.discounted_price,
                    seller_id: purchase.seller_id,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(OrderParticipationTransactionError::Participation);
        let snapshots = match result {
            Ok(snapshots) => snapshots,
            Err(error) => {
                *purchases = original;
                *success_times = original_success_times;
                return Err(error);
            }
        };
        match commit(&snapshots) {
            Ok(result) => Ok(result),
            Err(error) => {
                *purchases = original;
                *success_times = original_success_times;
                Err(OrderParticipationTransactionError::Commit(error))
            }
        }
    }

    pub(crate) fn order_cancellation_snapshot(
        &self,
        purchase_id: Uuid,
    ) -> Result<OrderCancellationSnapshot, OrderCancellationError> {
        let purchases = self.purchases.read().unwrap();
        let success_times = self.success_times.read().unwrap();
        let purchase = purchases
            .iter()
            .find(|purchase| purchase.group_purchase_id == purchase_id)
            .ok_or(OrderCancellationError::NotFound)?;
        Ok(OrderCancellationSnapshot {
            group_purchase_id: purchase.group_purchase_id,
            succeeded_at: success_times.get(&purchase.group_purchase_id).cloned(),
        })
    }

    pub(crate) fn with_order_cancellation<T, E>(
        &self,
        purchase_id: Uuid,
        quantity: i32,
        commit: impl FnOnce(&OrderCancellationSnapshot) -> Result<T, E>,
    ) -> Result<T, OrderCancellationTransactionError<E>> {
        self.with_order_cancellations(
            &[OrderCancellation {
                group_purchase_id: purchase_id,
                quantity,
            }],
            |snapshots| commit(&snapshots[0]),
        )
    }

    pub(crate) fn with_order_cancellations<T, E>(
        &self,
        requests: &[OrderCancellation],
        commit: impl FnOnce(&[OrderCancellationSnapshot]) -> Result<T, E>,
    ) -> Result<T, OrderCancellationTransactionError<E>> {
        let mut purchases = self.purchases.write().unwrap();
        let success_times = self.success_times.read().unwrap();
        let original = purchases.clone();
        let snapshots = requests
            .iter()
            .map(|request| {
                let purchase = purchases
                    .iter_mut()
                    .find(|purchase| purchase.group_purchase_id == request.group_purchase_id)
                    .ok_or(OrderCancellationError::NotFound)?;
                purchase.current_quantity = purchase
                    .current_quantity
                    .checked_sub(request.quantity)
                    .filter(|next| *next >= 0)
                    .ok_or(OrderCancellationError::Quantity)?;
                Ok(OrderCancellationSnapshot {
                    group_purchase_id: purchase.group_purchase_id,
                    succeeded_at: success_times.get(&purchase.group_purchase_id).cloned(),
                })
            })
            .collect::<Result<Vec<_>, _>>();
        let snapshots = match snapshots {
            Ok(snapshots) => snapshots,
            Err(error) => {
                *purchases = original;
                return Err(OrderCancellationTransactionError::Cancellation(error));
            }
        };
        match commit(&snapshots) {
            Ok(result) => Ok(result),
            Err(error) => {
                *purchases = original;
                Err(OrderCancellationTransactionError::Commit(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalProductRepository, ProductDetail, ProductRepository};
    use std::sync::Arc;
    use std::thread;

    fn product(product_id: Uuid) -> ProductDetail {
        ProductDetail {
            product_id,
            name: "사과".to_owned(),
            price: 10_000,
            category: "FOOD".to_owned(),
            description: "상품 설명".to_owned(),
            stock: 100,
            original_link: None,
            image_url: None,
            seller_id: Uuid::new_v4(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    fn purchase(
        purchase_id: Uuid,
        product_id: Uuid,
        current_quantity: i32,
        max_quantity: i32,
        status: &str,
    ) -> super::super::GroupPurchase {
        super::super::GroupPurchase {
            group_purchase_id: purchase_id,
            min_quantity: 1,
            max_quantity,
            title: "사과 공동구매".to_owned(),
            description: None,
            discounted_price: 8_000,
            current_quantity,
            like_count: 0,
            start_date: "2026-01-01T00:00:00Z".to_owned(),
            end_date: "2099-01-31T00:00:00Z".to_owned(),
            seller_id: Uuid::new_v4(),
            product_id,
            image_url: None,
            status: status.to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: None,
        }
    }

    #[test]
    fn participation_increases_quantity_and_marks_exact_capacity_successful() {
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases =
            GroupPurchaseRepository::new([purchase(purchase_id, product_id, 8, 10, "OPEN")]);
        let products = LocalProductRepository::new([product(product_id)]);

        assert_eq!(
            purchases.participate(purchase_id, 2, |id| products.find_by_id(id).is_some()),
            Ok(())
        );

        let updated = purchases.find_by_id(purchase_id).unwrap();
        assert_eq!(updated.current_quantity, 10);
        assert_eq!(updated.status, "SUCCESS");
        assert!(updated.updated_at.is_some());
        assert!(purchases
            .order_cancellation_snapshot(purchase_id)
            .unwrap()
            .succeeded_at
            .is_some());
    }

    #[test]
    fn closed_full_and_missing_purchases_share_the_legacy_capacity_error() {
        let product_id = Uuid::new_v4();
        let full_id = Uuid::new_v4();
        let closed_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(full_id, product_id, 10, 10, "OPEN"),
            purchase(closed_id, product_id, 1, 10, "SCHEDULED"),
        ]);
        let products = LocalProductRepository::new([product(product_id)]);

        for purchase_id in [full_id, closed_id, Uuid::new_v4()] {
            assert_eq!(
                purchases.participate(purchase_id, 1, |id| products.find_by_id(id).is_some()),
                Err(ParticipateError::CapacityReached)
            );
        }
    }

    #[test]
    fn missing_product_rolls_back_the_quantity_change() {
        let purchase_id = Uuid::new_v4();
        let purchases =
            GroupPurchaseRepository::new([purchase(purchase_id, Uuid::new_v4(), 3, 10, "OPEN")]);
        let products = LocalProductRepository::new([]);

        assert_eq!(
            purchases.participate(purchase_id, 2, |id| products.find_by_id(id).is_some()),
            Err(ParticipateError::ProductNotFound)
        );
        assert_eq!(
            purchases.find_by_id(purchase_id).unwrap().current_quantity,
            3
        );
    }

    #[test]
    fn concurrent_participation_never_exceeds_capacity() {
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases = Arc::new(GroupPurchaseRepository::new([purchase(
            purchase_id,
            product_id,
            0,
            100,
            "OPEN",
        )]));
        let products = Arc::new(LocalProductRepository::new([product(product_id)]));

        let attempts = (0..150)
            .map(|_| {
                let purchases = Arc::clone(&purchases);
                let products = Arc::clone(&products);
                thread::spawn(move || {
                    purchases.participate(purchase_id, 1, |id| products.find_by_id(id).is_some())
                })
            })
            .collect::<Vec<_>>();
        let results = attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 100);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(ParticipateError::CapacityReached))
                .count(),
            50
        );
        let updated = purchases.find_by_id(purchase_id).unwrap();
        assert_eq!(updated.current_quantity, 100);
        assert_eq!(updated.status, "SUCCESS");
    }

    #[test]
    fn order_batch_rolls_back_when_commit_fails() {
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases =
            GroupPurchaseRepository::new([purchase(purchase_id, product_id, 3, 10, "OPEN")]);
        let result = purchases.with_order_participation(
            &[OrderParticipation {
                group_purchase_id: purchase_id,
                quantity: 2,
            }],
            |_| true,
            |_| Err::<(), _>("repository write failed"),
        );

        assert!(matches!(
            result,
            Err(OrderParticipationTransactionError::Commit(
                "repository write failed"
            ))
        ));
        assert_eq!(
            purchases.find_by_id(purchase_id).unwrap().current_quantity,
            3
        );
    }

    #[test]
    fn order_batch_rolls_back_prior_items_when_a_product_is_missing() {
        let first_id = Uuid::new_v4();
        let first_product = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let missing_product = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(first_id, first_product, 1, 10, "OPEN"),
            purchase(second_id, missing_product, 2, 10, "OPEN"),
        ]);
        let result = purchases.with_order_participation(
            &[
                OrderParticipation {
                    group_purchase_id: first_id,
                    quantity: 3,
                },
                OrderParticipation {
                    group_purchase_id: second_id,
                    quantity: 4,
                },
            ],
            |product_id| product_id == first_product,
            |_| Ok::<_, ()>(()),
        );

        assert!(matches!(
            result,
            Err(OrderParticipationTransactionError::Participation(
                OrderParticipationError::ProductNotFound
            ))
        ));
        assert_eq!(purchases.find_by_id(first_id).unwrap().current_quantity, 1);
        assert_eq!(purchases.find_by_id(second_id).unwrap().current_quantity, 2);
    }

    #[test]
    fn order_batch_rejects_an_expired_open_purchase() {
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let mut expired = purchase(purchase_id, product_id, 3, 10, "OPEN");
        expired.end_date = "2020-01-01T00:00:00Z".to_owned();
        let purchases = GroupPurchaseRepository::new([expired]);

        let result = purchases.with_order_participation(
            &[OrderParticipation {
                group_purchase_id: purchase_id,
                quantity: 2,
            }],
            |_| true,
            |_| Ok::<_, ()>(()),
        );

        assert!(matches!(
            result,
            Err(OrderParticipationTransactionError::Participation(
                OrderParticipationError::Expired
            ))
        ));
        assert_eq!(
            purchases.find_by_id(purchase_id).unwrap().current_quantity,
            3
        );
    }

    #[test]
    fn order_cancellation_decreases_quantity_and_rolls_back_failed_commit() {
        let purchase_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases =
            GroupPurchaseRepository::new([purchase(purchase_id, product_id, 7, 10, "SUCCESS")]);

        let result = purchases
            .with_order_cancellation(purchase_id, 2, |_| Err::<(), _>("refund write failed"));

        assert!(matches!(
            result,
            Err(OrderCancellationTransactionError::Commit(
                "refund write failed"
            ))
        ));
        assert_eq!(
            purchases.find_by_id(purchase_id).unwrap().current_quantity,
            7
        );
    }

    #[test]
    fn order_cancellation_batch_rolls_back_prior_decrease() {
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        let purchases = GroupPurchaseRepository::new([
            purchase(first_id, product_id, 4, 10, "OPEN"),
            purchase(second_id, product_id, 1, 10, "OPEN"),
        ]);

        let result = purchases.with_order_cancellations(
            &[
                OrderCancellation {
                    group_purchase_id: first_id,
                    quantity: 2,
                },
                OrderCancellation {
                    group_purchase_id: second_id,
                    quantity: 2,
                },
            ],
            |_| Ok::<_, ()>(()),
        );

        assert!(matches!(
            result,
            Err(OrderCancellationTransactionError::Cancellation(
                OrderCancellationError::Quantity
            ))
        ));
        assert_eq!(purchases.find_by_id(first_id).unwrap().current_quantity, 4);
        assert_eq!(purchases.find_by_id(second_id).unwrap().current_quantity, 1);
    }
}
