# 0909 Commerce Migration

[`Huiyeongkim/0909`](https://github.com/Huiyeongkim/0909)의 `commerce` 서비스만
Rust로 점진적으로 마이그레이션하는 저장소입니다.

## 범위

- `legacy/`: 원본 저장소의 읽기 전용 clone이며 Git에서 제외합니다.
- `commerce-rs/`: Rust 대상 서비스입니다.
- `migration-spec/`: 동작 계약, 결정, 이상점, 마이그레이션 순서를 기록합니다.
- 다른 원본 서비스는 commerce 계약을 이해하는 데 꼭 필요한 경우에만 읽습니다.
- 외부 서비스를 다시 구축하거나 운영 환경에 연결하지 않습니다.

Toss/PG 결제는 로컬 포인트 충전·차감·환불과 원장으로 대체합니다. 회원,
이미지 저장소, 메시지 브로커 등 다른 외부 의존성도 commerce가 필요로 하는
최소 계약만 결정적인 로컬 어댑터로 대체합니다.

## 원본 준비

```bash
git clone --branch dev https://github.com/Huiyeongkim/0909.git legacy
```

`legacy/`는 수정하지 않습니다. 제품 코드는 `commerce-rs/`에만 작성합니다.

## 작업 흐름

각 수직 기능 단위는 다음 흐름으로 진행합니다.

1. GitHub Issue에 범위와 완료 조건을 기록합니다.
2. `codex/<migration-unit>` 브랜치를 만듭니다.
3. 레거시의 관찰 가능한 동작을 계약으로 먼저 기록합니다.
4. Rust 테스트와 구현을 작성합니다.
5. 관련 Java 테스트와 모든 Rust 품질 검사를 실행합니다.
6. 이상점과 의도적인 변경을 기록합니다.
7. 한 기능 단위만 포함한 Pull Request를 만듭니다.

첫 준비 작업 이후의 다음 단위는 `migration-spec/plan.json`에서 확인합니다.

제목은 다음 형식을 사용합니다.

- Issue: `[Migration] <unit-id>: <short outcome>`
- Pull Request: `migrate: <unit-id>`
- 저장소 준비나 유지보수는 `[Bootstrap]` Issue와 `chore:` PR을 사용합니다.

## Rust 검증

```bash
cd commerce-rs
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```
