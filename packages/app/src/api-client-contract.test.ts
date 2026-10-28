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

  it("uses the existing seller API authentication boundaries for management writes", async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(response({ status: 201, data: { productId: "product-1" }, message: "상품이 등록되었습니다." }))
      .mockResolvedValueOnce(response({ status: 200, data: { productId: "product-1" }, message: "상품 수정이 완료되었습니다." }))
      .mockResolvedValueOnce(response({ status: 201, data: { groupPurchaseId: "purchase-1" }, message: "공동구매가 등록되었습니다." }));
    vi.stubGlobal("fetch", fetch);
    const client = createCommerceClient("http://127.0.0.1:3000", () => ({ accessToken: "token", memberId: "member-1" }));
    const product = { name: "테스트 상품", price: 1000, category: "HOME", description: "설명", stock: 3, originalUrl: null, imageUrl: null, idempotencyKey: "request-1" };
    const purchase = { minQuantity: 1, maxQuantity: 10, title: "테스트 공동구매", description: null, discountedPrice: 900, startDate: "2027-01-01T00:00:00Z", endDate: "2027-01-02T00:00:00Z", productId: "product-1", imageUrl: null };

    await client.createProduct(product);
    await client.updateProduct("product-1", { ...product, originalLink: null });
    await client.createGroupPurchase(purchase);

    expect(fetch.mock.calls[0]?.[1]?.headers).toEqual({ "content-type": "application/json", "x-member-id": "member-1" });
    expect(fetch.mock.calls[1]?.[1]?.headers).toEqual({ "content-type": "application/json" });
    expect(fetch.mock.calls[2]?.[1]?.headers).toEqual({ "content-type": "application/json", "x-member-id": "member-1" });
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
