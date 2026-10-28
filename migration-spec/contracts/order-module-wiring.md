# Order module wiring contract

This unit adds only source boundaries for the remaining order slices.

- `order/mod.rs` contributes an empty router to the application.
- `create.rs`, `read.rs`, `confirm.rs`, `cancel.rs`, `lifecycle.rs`, and
  `participants.rs` reserve independent feature ownership.
- No HTTP route, order model, storage behavior, payment behavior, settlement,
  or event is added here.

`order-create` will define the shared local order state and wire empty child
routers before the read and participant slices run in parallel.
