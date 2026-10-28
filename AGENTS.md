# Commerce Migration Workspace

This workspace migrates one vertical slice at a time from the `commerce`
service in `Huiyeongkim/0909` to `commerce-rs`.

## Boundaries

- Treat `legacy/` as read-only source material. Never edit tracked files there.
- Migrate only `legacy/commerce`. Read shared or neighboring services only when
  required to identify a commerce-facing contract.
- Write product code only under `commerce-rs/`.
- Write migration contracts, decisions, and anomaly reports only under
  `migration-spec/`.
- Work on exactly one migration unit with status `ready` or `in_progress`.
- Preserve observable behavior before proposing improvements.
- Do not add production integrations, deployment, credentials, or production
  data access.

## Local replacements

- Replace Toss/PG payment with local point charge, debit, refund, and an
  auditable point ledger.
- Replace member/auth calls with the smallest local identity, session, and role
  contract required by commerce.
- Replace S3 with deterministic local image storage.
- Replace Kafka and other brokers with a deterministic local event adapter.
- Replace every other non-commerce dependency only at the boundary required by
  commerce. Do not reimplement the external service.

## GitHub workflow

- Use one GitHub Issue and one pull request per migration unit.
- Branch from `main` as `codex/<migration-unit>`.
- Keep unrelated migration units out of the branch.
- Link the pull request to its issue and include verification evidence.
- Do not merge a pull request unless the user explicitly asks.

## Worktree harness

- Use the `worktree-task-runner` Codex skill with the repository `harness.json`.
- Keep reusable orchestration behavior in the harness repository and keep only Commerce paths, commands, locks, and completion conditions here.
- Run mutating harness commands only after previewing them without `--apply`.
- A task is not dependency-complete until its pull request is merged and `sync --apply` confirms the merge.

## Verification

Run the relevant Java test first when possible, then run:

1. `cargo fmt --check`
2. `cargo check`
3. `cargo clippy --all-targets -- -D warnings`
4. `cargo test`
5. Confirm `git -C legacy diff --exit-code -- commerce`

If the same failure repeats three times, stop and report the blocker.
