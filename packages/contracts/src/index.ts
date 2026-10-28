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

export interface Credentials { username: string; password: string }
export interface MemberInfo { memberId: string; username: string }
export interface LoginInfo { accessToken: string; memberId: string }

export interface ProductDetail {
  productId: string;
  name: string;
  price: number;
  category: string;
  description: string;
  stock: number;
  originalLink: string | null;
  imageUrl: string | null;
  sellerId: string;
  createdAt: string;
  updatedAt: string | null;
}

export interface ProductListItem {
  productId: string;
  name: string;
  price: number;
  imageUrl: string | null;
  category: string;
  createdAt: string;
  updatedAt: string | null;
}

export interface ProductCreateRequest {
  name: string;
  price: number;
  category: string;
  description: string;
  stock: number;
  originalUrl: string | null;
  imageUrl: string | null;
  idempotencyKey: string;
}

export interface ProductUpdateRequest {
  name: string;
  price: number;
  category: string;
  description: string;
  stock: number;
  originalLink: string | null;
  imageUrl: string | null;
}

export interface ProductWriteResult {
  productId: string;
  name: string;
  price: number;
  category: string;
  description: string;
  stock: number;
  originalUrl?: string | null;
  originalLink?: string | null;
  imageUrl: string | null;
  sellerId: string;
  createdAt?: string;
}

export interface GroupPurchaseSummary {
  groupPurchaseId: string;
  minQuantity: number;
  maxQuantity: number;
  title: string;
  discountedPrice: number;
  originalPrice: number;
  currentQuantity: number;
  startDate: string;
  endDate: string;
  category: string;
  status: string;
  imageUrl: string | null;
  createdAt: string;
}

export interface GroupPurchaseDetail {
  groupPurchaseId: string;
  minQuantity: number;
  maxQuantity: number;
  title: string;
  description: string | null;
  price: number;
  discountedPrice: number;
  currentQuantity: number;
  startDate: string;
  endDate: string;
  sellerId: string;
  productId: string;
  originalUrl: string | null;
  imageUrl: string | null;
  category: string;
  status: string;
  createdAt: string;
}

export interface GroupPurchaseWriteRequest {
  minQuantity: number;
  maxQuantity: number;
  title: string;
  description: string | null;
  discountedPrice: number;
  startDate: string;
  endDate: string;
  productId: string;
  imageUrl: string | null;
}

export interface GroupPurchaseWriteResult {
  groupPurchaseId: string;
  minQuantity: number;
  maxQuantity: number;
  discountedPrice: number;
  title: string;
  description: string | null;
  status: string;
  imageUrl: string | null;
  startDate: string;
  endDate: string;
  sellerId: string;
  productId: string;
  createdAt: string;
  updatedAt: string | null;
}

export interface CartAddRequest { groupPurchaseId: string; quantity: number }
export interface CartInfo {
  cartId: string;
  memberId: string;
  groupPurchaseId: string;
  quantity: number;
  createdAt: string;
  updatedAt: string | null;
}

export interface DirectOrderRequest {
  quantity: number;
  address: string;
  addressDetail: string;
  postalCode: string;
  receiverName: string;
  sellerId: string;
  groupPurchaseId: string;
  requestId: string;
}

export interface OrderInfo {
  orderId: string;
  orderNumber: string;
  status: string;
  quantity: number;
  price: number;
  totalAmount: number;
  groupPurchaseId: string;
  groupPurchaseName: string | null;
  createdAt: string;
}

export interface OrderDetail {
  orderId: string;
  orderNumber: string;
  quantity: number;
  price: number;
  status: string;
  memberId: string;
  address: string;
  addressDetail: string;
  postalCode: string;
  receiverName: string;
  sellerId: string;
  groupPurchaseId: string;
  createdAt: string;
  updatedAt: string | null;
  deletedAt: string | null;
}
export interface OrderRegisterInfo extends OrderDetail {}

export type CancelReason = "CHANGE_OF_MIND" | "PRODUCT_DEFECT" | "DELIVERY_DELAY" | "OUT_OF_STOCK";
export interface CancelOrderRequest { reason: CancelReason; detailReason: string; idempotencyKey: string }

export interface PointChargeRequest { amount: number; transactionId: string }
export interface PointChargeResult { transactionId: string; amount: number; balance: number }
export interface PointPaymentRequest { transactionId: string }
export interface PointPaymentResult {
  orderId: string;
  transactionId: string;
  amount: number;
  balance: number;
  status: "PAYMENT_COMPLETED";
  paymentMethod: "POINT";
  paidAt: string;
}
