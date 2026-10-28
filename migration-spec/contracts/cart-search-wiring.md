# Cart and internal-search wiring contract

This unit adds only empty Rust router boundaries needed for isolated follow-up
work.

- `cart.rs` receives the local group-purchase repository and contributes an
  empty router to the application.
- `group_purchase/search.rs` contributes an empty router before the existing
  group-purchase state is attached.
- No HTTP path, response, persistence behavior, or external integration is
  added by this unit.

The cart and internal-search migration units own the behavior and focused
tests that will fill these boundaries.
