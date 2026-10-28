---
name: Migration unit
about: Migrate one observable commerce behavior slice
title: "[Migration] "
labels: ""
assignees: ""
---

## Goal

Describe the single observable behavior this unit migrates.

## Legacy scope

- Source paths:
- Relevant Java tests:

## Observable contract

- Inputs:
- Outputs:
- Errors:
- Transaction, idempotency, or concurrency behavior:

## Local replacements

List any external boundary replaced by a deterministic local adapter. Write
`None` when the unit has no external dependency.

## Done when

- [ ] The legacy contract is documented.
- [ ] Rust tests cover success and failure behavior.
- [ ] The Rust implementation preserves the contract.
- [ ] All available verification passes.
- [ ] `legacy/commerce` remains unchanged.

## Out of scope

List behavior deliberately excluded from this unit.
