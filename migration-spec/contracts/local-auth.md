# Local authentication contract

This slice replaces the member-service/JWT dependency with the smallest local identity and revocable session boundary needed by Commerce.

## Routes

- `POST /auth/register` accepts JSON `username` and `password`, creates a UUID member, and returns HTTP 201 with `memberId` and `username`.
- `POST /auth/login` accepts the same credentials and returns HTTP 200 with `accessToken` and `memberId`.
- `GET /auth/session` accepts `Authorization: Bearer <accessToken>` and resolves the active session to `{ "memberId": <UUID> }`.
- `POST /auth/logout` accepts the bearer token, revokes its session, and returns HTTP 200. The token is rejected by subsequent session lookups.

## Validation and errors

- Usernames are trimmed and must contain 3–64 bytes. Passwords must contain 8–256 bytes and are otherwise treated verbatim.
- Malformed JSON or invalid credentials at registration return HTTP 400 `적절하지 않은 요청 값이 존재합니다.`.
- Duplicate usernames return HTTP 409 `이미 사용중인 아이디입니다.`.
- Unknown usernames and wrong passwords deliberately share HTTP 401 `로그인 인증에 실패했습니다.`.
- Missing, malformed, expired, unknown, or revoked bearer tokens return HTTP 401 `로그인 정보가 없습니다.`.

## Security and persistence

- Passwords are stored only as Argon2id PHC strings with a unique random salt. CPU-heavy hashing and verification run on Tokio blocking workers.
- Access tokens are opaque UUIDv4 values. PostgreSQL stores only their SHA-256 digest, member UUID, creation time, 24-hour expiry, and revocation time.
- `auth_schema.local_account` and `auth_schema.local_session` provide the production repository boundary; focused tests use an in-memory adapter with the same account/session semantics.
- The session lookup response is the UUID member boundary consumed by Commerce. Replacing every existing trusted `x-member-id` call site is a later integration step.

Sources: legacy member `AuthController`, `AuthService`, `AuthServiceTest`, `Member`, shared error codes, and the migration decision to replace member authentication locally.
