# 0909 Commerce Migration

This repository migrates only the `commerce` service from
[`Huiyeongkim/0909`](https://github.com/Huiyeongkim/0909) to Rust.

Migration work is performed incrementally through one GitHub issue and pull
request per vertical slice. External services are replaced with deterministic
local adapters; Toss/PG payment is replaced by local point payment.
