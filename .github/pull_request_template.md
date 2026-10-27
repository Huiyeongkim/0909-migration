## Summary

Closes #

Describe the single migration unit and its outcome.

## Legacy contract

Summarize the observable behavior preserved by this change and link the
contract under `migration-spec/contracts/`.

## Intentional changes and anomalies

List deliberate differences and unresolved legacy behavior, or write `None`.

## Verification

- [ ] Relevant Java tests pass when available.
- [ ] `cargo fmt --check`
- [ ] `cargo check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] `git -C legacy diff --exit-code -- commerce`

## Scope check

- [ ] This PR contains exactly one migration unit.
- [ ] No production integration, credentials, or data access was added.
- [ ] External services are represented only by the smallest required local boundary.
