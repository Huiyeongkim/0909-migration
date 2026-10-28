export interface ApiResponse<T> {
  status: number;
  data: T | null;
  message: string;
}

export interface PageResponse<T> {
  content: T[];
  totalPages: number;
  totalElements: number;
  first: boolean;
  last: boolean;
  size: number;
  numberOfElements: number;
}

export interface PointChargeRequest {
  amount: number;
  transactionId: string;
}

export interface PointChargeResult {
  transactionId: string;
  amount: number;
  balance: number;
}

export interface PointPaymentRequest {
  transactionId: string;
}

export interface PointPaymentResult {
  orderId: string;
  transactionId: string;
  amount: number;
  balance: number;
  status: "PAYMENT_COMPLETED";
  paymentMethod: "POINT";
  paidAt: string;
}
