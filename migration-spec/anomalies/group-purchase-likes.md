# Group-purchase like anomalies

## Unlike does not distinguish a missing purchase

The legacy unlike flow looks up the member/purchase like first and returns
`LIKE_NOT_FOUND` when it is absent. It does not first return
`GROUP_PURCHASE_NOT_FOUND`. The Rust migration preserves that error order.

## Local persistence only

Likes and the aggregate count are process-local and protected by one repository
write boundary. Durable database persistence is outside this slice.
