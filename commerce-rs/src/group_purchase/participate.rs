use super::GroupPurchaseRepository;
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
        }
        Ok(())
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
            end_date: "2026-01-31T00:00:00Z".to_owned(),
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
}
