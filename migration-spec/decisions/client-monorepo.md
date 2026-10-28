# Client monorepo architecture

## Decision

The Rust Commerce backend, React web application, and Tauri desktop
application live in this repository. They remain separate build and runtime
units even though they share version control, issues, contracts, and CI.

```text
0909-migration/
├── commerce-rs/          Rust HTTP backend
├── apps/
│   ├── web/              browser entry point
│   └── desktop/          Tauri entry point and Rust native shell
├── packages/
│   ├── app/              shared React Commerce screens and flows
│   ├── ui/               small presentational components
│   ├── api-client/       typed HTTP boundary
│   └── contracts/        API request and response types
└── migration-spec/       source contracts, decisions, and anomalies
```

The desktop UI is not a second UI implementation. Tauri hosts the same React
application used by the browser. Rust code under `src-tauri` is reserved for
native shell behavior that cannot be provided by the shared web application.

## Shared and application-specific code

`packages/app` is the stable React entry point and exports
`CommerceApp({ target })`. Web and desktop pass their target only for the small
number of environment-specific choices. Business screens, accessibility,
loading, empty, and error states remain shared.

HTTP request and response types come from `packages/contracts`, which mirrors
the approved Markdown contracts. `packages/api-client` owns base URL, bearer
token attachment, JSON parsing, and typed errors. Components do not call
`fetch` directly. `packages/ui` contains a component only after both a real
screen and its interaction justify sharing it.

Browser-only code stays in `apps/web`. Tauri configuration, capabilities, and
Rust commands stay in `apps/desktop`. Native commands must not duplicate an
HTTP backend operation.

## Contract-first parallel work

An API contract is merged before backend and client implementation branches
start. A client may use deterministic mocks while the backend is in progress,
but the mock must implement the merged request, response, and error contract.

The worktree harness owns concurrency:

- backend lifecycle work locks the order, identity, point, and group-purchase
  boundaries;
- React work owns `apps/web` and shared `packages`;
- Tauri work owns only `apps/desktop` and consumes the stable
  `@commerce/app` export;
- tasks never edit the same path concurrently;
- merged worktrees are verified one at a time on `main`.

This keeps three agents independent without copying UI code or allowing a
desktop branch to redefine the API.

## Build and run boundaries

The repository uses pnpm workspaces for client packages. Web and desktop UI
have independent Vite builds. Tauri has its own Cargo manifest and Rust check.
The existing `commerce-rs` Cargo project remains independent; a root Cargo
workspace is intentionally not introduced because it would couple backend and
desktop release settings without a current need.

Expected commands are:

```text
pnpm install --frozen-lockfile
pnpm typecheck:web
pnpm test:web
pnpm build:web
pnpm build:desktop-ui
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Security

- The local backend remains loopback-bound during this migration.
- Bearer tokens are supplied at the HTTP boundary and are never logged.
- Tauri starts with only core capabilities. File-system, shell, process,
  clipboard, and unrestricted remote navigation permissions are excluded.
- The development point-charge endpoint is not an operating payment product.
  It must be disabled or replaced by an authorized funding boundary before a
  non-local deployment.
- Structured logs may contain transaction and order identifiers and amounts,
  but not credentials, member identifiers, addresses, or receiver data.

## Deliberate exclusions

This foundation does not add a router framework, global state library, design
system, code generator, OpenAPI generator, native updater, file-system plugin,
or deployment configuration. Add one only when a concrete screen or native
capability requires it.

Related issues: #66 (foundation), #67 (backend lifecycle), #64 (React web),
and #65 (Tauri desktop).
