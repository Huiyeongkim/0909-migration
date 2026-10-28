import type { CommerceClient } from "@commerce/api-client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { CommerceApp, loginAndPersist, payOrderWithPoints } from "./commerce-app";

describe("CommerceApp", () => {
  it("renders the same shared application for the selected target", () => {
    const client = clientStub();
    expect(renderToStaticMarkup(<CommerceApp target="web" client={client} storage={memoryStorage()} />)).toContain("0909 Commerce · Web");
    expect(renderToStaticMarkup(<CommerceApp target="desktop" client={client} storage={memoryStorage()} />)).toContain("0909 Commerce · Desktop");
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
    listGroupPurchases: vi.fn().mockResolvedValue({
      content: [], totalPages: 0, totalElements: 0, first: true, last: true,
      size: 50, numberOfElements: 0,
    }),
    getGroupPurchase: vi.fn(), addCart: vi.fn(),
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
