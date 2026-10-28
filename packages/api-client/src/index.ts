import type {
  ApiResponse,
  PointChargeRequest,
  PointChargeResult,
  PointPaymentRequest,
  PointPaymentResult,
} from "@commerce/contracts";

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export interface CommerceClient {
  chargePoints(request: PointChargeRequest): Promise<PointChargeResult>;
  payOrderWithPoints(
    orderId: string,
    request: PointPaymentRequest,
  ): Promise<PointPaymentResult>;
}

export function createCommerceClient(
  baseUrl: string,
  getToken: () => string | null,
): CommerceClient {
  async function request<T>(path: string, body: unknown): Promise<T> {
    const token = getToken();
    const response = await fetch(new URL(path, baseUrl), {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(body),
    });
    const envelope: unknown = await response.json();
    if (!isEnvelope(envelope)) {
      throw new ApiError(response.status, "서버 응답 형식이 올바르지 않습니다.");
    }
    if (!response.ok || envelope.data === null) {
      throw new ApiError(envelope.status, envelope.message);
    }
    return envelope.data as T;
  }

  return {
    chargePoints: (body) => request("/api/points/charge", body),
    payOrderWithPoints: (orderId, body) =>
      request(`/api/orders/${encodeURIComponent(orderId)}/point-payment`, body),
  };
}

function isEnvelope(value: unknown): value is ApiResponse<unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { status?: unknown }).status === "number" &&
    typeof (value as { message?: unknown }).message === "string" &&
    "data" in value
  );
}
