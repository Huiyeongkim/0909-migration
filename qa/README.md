# Commerce QA

Commerce 전용 QA 명세는 `harness.json`의 `kind: "qa"` 작업에 둔다. 범용 하네스는 작업 선택, 검증, 결과와 증거 수집을 담당하고 이 저장소에는 프로젝트의 명령과 사용자 흐름만 남긴다.

## 실행 원칙

- 로컬 PostgreSQL과 `127.0.0.1` 서비스만 사용한다.
- `DATABASE_URL`은 실행 환경으로 전달하고 보고서나 스크린샷에 기록하지 않는다.
- 상태를 변경하는 시나리오는 실행마다 `qa-<run-id>` 형식의 새 계정을 사용한다. 비밀번호는 런타임에 만들고 저장하지 않는다.
- 같은 DB와 백엔드를 사용하는 QA 작업은 `qa-runtime` 잠금으로 직렬 실행한다.
- QA 에이전트는 결함을 수정하지 않는다. 실패 시 재현 절차, 기대 결과, 실제 결과와 증거를 기록한다.
- 결함 수정은 별도 Issue, 브랜치와 PR에서 진행한다.

## 로컬 서비스

| 대상 | 주소 | 실행 조건 |
| --- | --- | --- |
| Rust API | `http://127.0.0.1:8080` | `DATABASE_URL` 필요 |
| React Web | `http://127.0.0.1:5174` | `/api`, `/auth`를 API로 프록시 |
| Tauri UI | `http://127.0.0.1:1421` | `pnpm --filter @commerce/desktop tauri dev` |

하네스의 자동 검증 명령은 서버를 시작하지 않는다. 장시간 실행되는 서버와 네이티브 창은 시나리오를 수행하는 QA 에이전트가 시작하고 종료한다.

## 현재 제약

공동구매 생성 API는 새 항목을 항상 `SCHEDULED`로 만든다. 실제 화면에서 장바구니 → 주문 → 포인트 결제를 끝까지 확인하려면 현재 시각에 `OPEN`인 승인된 로컬 fixture가 필요하다. fixture가 없으면 `login-cart-order-point` 결과를 우회하지 않고 `blocked`로 기록한다. QA를 위해 운영 우회 API나 DB 직접 수정 절차를 추가하지 않는다.

## 하네스 실행

범용 `worktree-task-runner`의 QA 명령을 사용한다.

```bash
python3 "$RUNNER" --config harness.json validate
python3 "$RUNNER" --config harness.json next
python3 "$RUNNER" --config harness.json start qa-web-ui --apply
python3 "$RUNNER" --config harness.json check qa-web-ui --apply
python3 "$RUNNER" --config harness.json qa-result qa-web-ui web-responsive-accessibility \
  --outcome passed --note "데스크톱·모바일과 키보드 흐름 확인" \
  --artifact qa/runs/<run-id>/web-mobile.png --apply
python3 "$RUNNER" --config harness.json qa-report qa-web-ui --apply
```

실패 결과에는 민감정보를 제거한 로그나 스크린샷을 첨부한다. `qa-report`가 만든 이슈 초안은 사람이 내용을 확인한 뒤 GitHub Issue로 등록한다.

## 반복 실행

같은 QA 작업을 다시 실행할 때는 이전 `qa-report` 이후 `qa-reset`을 미리 확인한 다음 적용한다.

```bash
python3 "$RUNNER" --config harness.json qa-reset qa-web-ui
python3 "$RUNNER" --config harness.json qa-reset qa-web-ui --apply
```

적용하기 전에 결과에 기록된 로컬 로그와 스크린샷을 QA 작업 트리 밖의 지속 저장소로 이동하거나 복사한다. 복사한 경우 작업 트리에 남은 원본을 정리하고 Git 작업 트리가 깨끗한지 확인한다. `qa-reset`은 이전 보고서 이력을 보존하지만, 작업 트리가 더럽거나 기록된 로컬 아티팩트가 아직 남아 있으면 초기화를 거부한다.

