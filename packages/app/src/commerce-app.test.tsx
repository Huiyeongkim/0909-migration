import type { CommerceClient } from "@commerce/api-client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { CommerceApp, loginAndPersist, payOrderWithPoints } from "./commerce-app";

describe("CommerceApp", () => {
  it("renders the same shared application for the selected target", () => {
    const client = clientStub();
    const web = renderToStaticMarkup(<CommerceApp target="web" client={client} storage={memoryStorage()} />);
    const desktop = renderToStaticMarkup(<CommerceApp target="desktop" client={client} storage={memoryStorage()} />);
    expect(web).toContain("0909 COMMERCE");
    expect(web).toContain("WEB");
    expect(desktop).toContain("DESKTOP");
  });

  it("renders purchase detail on an independent route", () => {
    const markup = renderToStaticMarkup(<CommerceApp target="web" client={clientStub()} storage={memoryStorage()} initialPath="/purchases/purchase-1" />);

    expect(markup).toContain("PURCHASE DETAIL");
    expect(markup).toContain("상세 정보를 불러오는 중입니다.");
    expect(markup).not.toContain("ALL DROP");
  });

  it("protects the seller console behind the existing local session", () => {
    const markup = renderToStaticMarkup(<CommerceApp target="web" client={clientStub()} storage={memoryStorage()} initialPath="/admin" />);

    expect(markup).toContain("계정에 먼저 로그인해 주세요");
    expect(markup).toContain("판매자 관리는 로그인 후 사용할 수 있습니다.");
    expect(markup).not.toContain("SELLER CONSOLE");
  });

  it("logs in with credentials and persists only the returned local session", async () => {
    const client = clientStub();
    client.login = vi.fn().mockResolvedValue({ accessToken: "token-1", memberId: "member-1" });
    const storage = memoryStorage();

    const session = await loginAndPersist(client, storage, {
      username: "rookie",
      password: "password123",
    });

    expect(client.login).toHaveBeenCalledWith({ username: "rookie", password: "password123" });
    expect(session).toEqual({ accessToken: "token-1", memberId: "member-1" });
    expect(JSON.parse(storage.getItem("commerce.local-session")!)).toEqual(session);
    expect(storage.getItem("password")).toBeNull();
  });

  it("uses a client-generated transaction id for point payment and returns the balance", async () => {
    const client = clientStub();
    client.payOrderWithPoints = vi.fn().mockResolvedValue({
      orderId: "order-1", transactionId: "transaction-1", amount: 10000,
      balance: 2500, status: "PAYMENT_COMPLETED", paymentMethod: "POINT",
      paidAt: "2026-01-01T00:01:00Z",
    });

    const result = await payOrderWithPoints(client, "order-1", "transaction-1");

    expect(client.payOrderWithPoints).toHaveBeenCalledWith("order-1", { transactionId: "transaction-1" });
    expect(result.balance).toBe(2500);
  });
});

function clientStub(): CommerceClient {
  return {
    register: vi.fn(), login: vi.fn(), logout: vi.fn(), getProduct: vi.fn(),
    listSellerProducts: vi.fn(), createProduct: vi.fn(), updateProduct: vi.fn(), deleteProduct: vi.fn(),
    listGroupPurchases: vi.fn().mockResolvedValue({
      content: [], totalPages: 0, totalElements: 0, first: true, last: true,
      size: 50, numberOfElements: 0,
    }),
    listSellerGroupPurchases: vi.fn(), getGroupPurchase: vi.fn(),
    createGroupPurchase: vi.fn(), updateGroupPurchase: vi.fn(), deleteGroupPurchase: vi.fn(), addCart: vi.fn(),
    listCarts: vi.fn().mockResolvedValue({
      content: [], totalPages: 0, totalElements: 0, first: true, last: true,
      size: 50, numberOfElements: 0,
    }),
    updateCart: vi.fn(), deleteCart: vi.fn(), createOrder: vi.fn(),
    listOrders: vi.fn().mockResolvedValue({
      content: [], totalPages: 0, totalElements: 0, first: true, last: true,
      size: 50, numberOfElements: 0,
    }),
    getOrder: vi.fn(), cancelOrder: vi.fn(), chargePoints: vi.fn(), payOrderWithPoints: vi.fn(),
  };
}

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => { data.set(key, value); },
    removeItem: (key: string) => { data.delete(key); },
  };
}
