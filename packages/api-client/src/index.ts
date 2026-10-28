import type {
  ApiResponse, CancelOrderRequest, CartAddRequest, CartInfo, Credentials,
  DirectOrderRequest, GroupPurchaseDetail, GroupPurchaseSummary, LoginInfo,
  MemberInfo, OrderDetail, OrderInfo, OrderRegisterInfo, PageResponse,
  PointChargeRequest, PointChargeResult, PointPaymentRequest, PointPaymentResult,
  ProductDetail,
} from "@commerce/contracts";

export class ApiError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

export interface CommerceSession { accessToken: string; memberId: string }

export interface CommerceClient {
  register(credentials: Credentials): Promise<MemberInfo>;
  login(credentials: Credentials): Promise<LoginInfo>;
  logout(): Promise<void>;
  getProduct(productId: string): Promise<ProductDetail>;
  listGroupPurchases(): Promise<PageResponse<GroupPurchaseSummary>>;
  getGroupPurchase(groupPurchaseId: string): Promise<GroupPurchaseDetail>;
  addCart(request: CartAddRequest): Promise<CartInfo>;
  listCarts(): Promise<PageResponse<CartInfo>>;
  updateCart(cartId: string, quantity: number): Promise<CartInfo>;
  deleteCart(cartId: string): Promise<CartInfo>;
  createOrder(request: DirectOrderRequest): Promise<OrderRegisterInfo>;
  listOrders(): Promise<PageResponse<OrderInfo>>;
  getOrder(orderId: string): Promise<OrderDetail>;
  cancelOrder(orderId: string, request: CancelOrderRequest): Promise<void>;
  chargePoints(request: PointChargeRequest): Promise<PointChargeResult>;
  payOrderWithPoints(orderId: string, request: PointPaymentRequest): Promise<PointPaymentResult>;
}

interface RequestOptions {
  method?: "GET" | "POST" | "PATCH" | "DELETE";
  body?: unknown;
  auth?: "member" | "bearer";
}

export function createCommerceClient(
  baseUrl: string,
  getSession: () => CommerceSession | null,
): CommerceClient {
  async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
    const session = getSession();
    const headers: Record<string, string> = {};
    if (options.body !== undefined) headers["content-type"] = "application/json";
    if (options.auth === "member") {
      if (!session) throw new ApiError(401, "로그인이 필요합니다.");
      headers["x-member-id"] = session.memberId;
    }
    if (options.auth === "bearer") {
      if (!session) throw new ApiError(401, "로그인이 필요합니다.");
      headers.authorization = `Bearer ${session.accessToken}`;
    }

    let response: Response;
    try {
      response = await fetch(new URL(path, normalizedBaseUrl(baseUrl)), {
        method: options.method ?? "GET",
        headers,
        body: options.body === undefined ? undefined : JSON.stringify(options.body),
      });
    } catch {
      throw new ApiError(0, "서버에 연결할 수 없습니다.");
    }

    let envelope: unknown;
    try {
      envelope = await response.json();
    } catch {
      throw new ApiError(response.status, "서버 응답 형식이 올바르지 않습니다.");
    }
    if (!isEnvelope(envelope)) {
      throw new ApiError(response.status, "서버 응답 형식이 올바르지 않습니다.");
    }
    if (!response.ok) throw new ApiError(envelope.status, envelope.message);
    if (envelope.data === null) return undefined as T;
    return envelope.data as T;
  }

  return {
    register: (body) => request("/auth/register", { method: "POST", body }),
    login: (body) => request("/auth/login", { method: "POST", body }),
    logout: () => request("/auth/logout", { method: "POST", auth: "bearer" }),
    getProduct: (id) => request(`/api/products/${encodeURIComponent(id)}`),
    listGroupPurchases: () => request("/api/purchases?page=0&size=50&sort=createdAt,desc"),
    getGroupPurchase: (id) => request(`/api/purchases/${encodeURIComponent(id)}`),
    addCart: (body) => request("/api/carts", { method: "POST", body, auth: "member" }),
    listCarts: () => request("/api/carts?page=0&size=50", { auth: "member" }),
    updateCart: (cartId, quantity) => request("/api/carts", {
      method: "PATCH", body: { cartId, quantity }, auth: "member",
    }),
    deleteCart: (cartId) => request("/api/carts", {
      method: "DELETE", body: { cartId }, auth: "member",
    }),
    createOrder: (body) => request("/api/orders", { method: "POST", body, auth: "member" }),
    listOrders: () => request("/api/orders/consumer?page=0&size=50", { auth: "member" }),
    getOrder: (id) => request(`/api/orders/${encodeURIComponent(id)}`, { auth: "member" }),
    cancelOrder: (id, body) => request(`/api/orders/cancel/${encodeURIComponent(id)}`, {
      method: "POST", body, auth: "member",
    }),
    chargePoints: (body) => request("/api/points/charge", {
      method: "POST", body, auth: "bearer",
    }),
    payOrderWithPoints: (orderId, body) => request(
      `/api/orders/${encodeURIComponent(orderId)}/point-payment`,
      { method: "POST", body, auth: "bearer" },
    ),
  };
}

function normalizedBaseUrl(baseUrl: string): string {
  return baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
}

function isEnvelope(value: unknown): value is ApiResponse<unknown> {
  return typeof value === "object" && value !== null
    && typeof (value as { status?: unknown }).status === "number"
    && typeof (value as { message?: unknown }).message === "string"
    && "data" in value;
}
