# Local authentication anomalies

- Legacy login uses email/password and issues stateless access/refresh JWT cookies. The approved local replacement uses username/password and one opaque bearer session so it can be revoked without Redis, JWT signing keys, or production credentials.
- Legacy logout is a GET that only expires client cookies and does not revoke issued tokens. Local logout is POST and records server-side revocation.
- Refresh tokens, OAuth, email verification, roles, seller registration, and point payment are outside this slice.
- Existing Commerce routes still accept the trusted `x-member-id` boundary. `/auth/session` now provides the authenticated session-to-UUID resolution that a later integration can wire into those routes.
- The inspected legacy `AuthServiceTest` does not pass on the source baseline: login tests stub `findByEmail` while the service calls `findUndeletedMemberByEmail`, and the refresh test supplies a repository member whose generated ID differs from the token member ID. The legacy source remains unchanged.
- No live PostgreSQL integration test was available; SQLx queries are exercised structurally while focused behavior tests use the repository test adapter.
