# Group-purchase module boundary contract

This unit changes only the Rust source layout. The existing group-purchase
routes, response envelopes, repository behavior, validation order, and error
mapping remain unchanged.

- Existing read and write behavior lives in `group_purchase/mod.rs`.
- The like slice owns `group_purchase/likes.rs`.
- The participation slice owns `group_purchase/participate.rs`.
- `main.rs` continues to import the module as `mod group_purchase`; callers do
  not observe the file-to-directory move.

The two feature files intentionally contain no behavior in this unit. Their
follow-up migration units define and test their own contracts.
