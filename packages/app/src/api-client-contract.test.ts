import { ApiError, createCommerceClient } from "@commerce/api-client";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => vi.unstubAllGlobals());

describe("Commerce API client", () => {
  it("keeps member and bearer authentication at their documented boundaries", async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(response({ status: 200, data: page([]), message: "장바구니" }))
      .mockResolvedValueOnce(response({
        status: 200,
        data: { transactionId: "tx", amount: 1000, balance: 1000 },
        message: "포인트가 충전되었습니다.",
      }));
    vi.stubGlobal("fetch", fetch);
    const client = createCommerceClient("http://127.0.0.1:3000", () => ({
      accessToken: "secret-token",
      memberId: "member-1",
    }));

    await client.listCarts();
    await client.chargePoints({ amount: 1000, transactionId: "tx" });

    expect(fetch.mock.calls[0]?.[1]?.headers).toEqual({ "x-member-id": "member-1" });
    expect(fetch.mock.calls[1]?.[1]?.headers).toEqual({
      "content-type": "application/json",
      authorization: "Bearer secret-token",
    });
  });

  it("rejects a response outside the common envelope", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({ result: "unexpected" })));
    const client = createCommerceClient("http://127.0.0.1:3000", () => null);

    await expect(client.listGroupPurchases()).rejects.toEqual(
      new ApiError(200, "서버 응답 형식이 올바르지 않습니다."),
    );
  });
});

function response(body: unknown) {
  return { ok: true, status: 200, json: async () => body } as Response;
}

function page<T>(content: T[]) {
  return {
    content,
    totalPages: content.length ? 1 : 0,
    totalElements: content.length,
    first: true,
    last: true,
    size: 50,
    numberOfElements: content.length,
  };
}
