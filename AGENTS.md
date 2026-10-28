# Commerce Migration Monorepo

This workspace migrates the `commerce` service to a Rust backend and ships the
same Commerce experience through a React web app and a Tauri desktop shell.

## Boundaries

- Treat `legacy/` as read-only source material. Never edit tracked files there.
- Migrate only `legacy/commerce`. Read shared or neighboring services only when
  required to identify a commerce-facing contract.
- Write backend product code under `commerce-rs/`.
- Write deployable clients under `apps/web/` and `apps/desktop/`.
- Write client code shared by web and desktop under `packages/`.
- Write migration contracts, decisions, and anomaly reports only under
  `migration-spec/`.
- Work on exactly one migration unit per branch and worktree. Multiple units may
  run concurrently only when `harness.json` dependencies and locks allow it.
- Preserve observable behavior before proposing improvements.
- Do not add production integrations, deployment, credentials, or production
  data access.

## Client boundaries

- `packages/app` owns shared Commerce screens and flows.
- `packages/ui` owns the smallest reusable presentational components.
- `packages/api-client` owns HTTP transport and response validation boundaries.
- `packages/contracts` owns API request and response types derived from
  `migration-spec/contracts`.
- `apps/web` is only the browser entry point and browser-specific wiring.
- `apps/desktop` is the Tauri entry point. React remains the UI layer; Rust is
  limited to native shell commands and capabilities.
- Define or update the API contract before changing backend and clients in
  parallel. Do not let a client infer an undocumented response shape.
- Do not add Tauri capabilities, file-system access, shell execution, or remote
  navigation unless a migration unit explicitly requires them.

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

For client changes also run the task-specific pnpm typecheck, tests, and
production build. For Tauri changes additionally run `cargo check` against the
desktop manifest.

If the same failure repeats three times, stop and report the blocker.
