# 상품 PostgreSQL 동시성 보강

- 상품 API의 기존 필드, 오류 응답과 전역 멱등 키 계약은 변경하지 않는다.
- 생성 요청은 멱등 키별 PostgreSQL 트랜잭션 advisory lock을 획득한 뒤 기존 상품을 조회하거나 새 상품을 저장한다. 같은 키의 동시 요청은 한 상품만 만들고 모두 같은 상품을 반환한다.
- 상품 조회·목록·수정·삭제는 서버 메모리가 아니라 `product_schema.product`를 사용하므로 재시작과 다중 인스턴스에서 같은 상태를 본다.
- 공동구매·주문 저장소는 다음 `commerce-transaction-hardening` 작업 전까지 메모리 저장소다. 주문 생성 시 상품 존재 여부는 요청 시점의 PostgreSQL 스냅샷으로 확인하며, 후속 작업에서 주문과 공동구매를 같은 DB 트랜잭션으로 묶는다.
- 실제 PostgreSQL 경쟁 테스트는 `TEST_DATABASE_URL`이 설정된 테스트 전용 DB에서만 실행한다. 일반 `cargo test`는 개발 DB를 임의로 변경하지 않는다.
