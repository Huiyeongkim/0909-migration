import { ApiError, createCommerceClient, type CommerceClient, type CommerceSession } from "@commerce/api-client";
import type {
  CartInfo, Credentials, DirectOrderRequest, GroupPurchaseDetail, GroupPurchaseSummary,
  OrderInfo, PointPaymentResult, ProductDetail,
} from "@commerce/contracts";
import { type ButtonHTMLAttributes, type CSSProperties, type FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { BrowserRouter, HashRouter, MemoryRouter, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { SellerAdmin } from "./seller-admin";
import "./styles.css";

const SESSION_KEY = "commerce.local-session";
type View = "shop" | "cart" | "orders" | "points" | "account" | "admin";
type LoadState = "idle" | "loading" | "ready" | "error";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "danger";
}

function Button({ className = "", variant = "primary", type = "button", ...props }: ButtonProps) {
  return <button type={type} className={`button button--${variant} ${className}`.trim()} {...props} />;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface CommerceAppProps {
  target: "web" | "desktop";
  client?: CommerceClient;
  storage?: StorageLike;
  idFactory?: () => string;
  initialPath?: string;
}

export function CommerceApp({ target, client, storage, idFactory = createId, initialPath }: CommerceAppProps) {
  const content = <CommerceRoutes target={target} client={client} storage={storage} idFactory={idFactory} />;
  if (initialPath || typeof window === "undefined") return <MemoryRouter initialEntries={[initialPath ?? "/"]}>{content}</MemoryRouter>;
  return target === "desktop" ? <HashRouter>{content}</HashRouter> : <BrowserRouter>{content}</BrowserRouter>;
}

function CommerceRoutes({ target, client, storage, idFactory = createId }: CommerceAppProps) {
  const sessionStore = useMemo(() => storage ?? browserStorage(), [storage]);
  const [session, setSession] = useState<CommerceSession | null>(() => readSession(sessionStore));
  const api = useMemo(
    () => client ?? createCommerceClient(defaultApiBaseUrl(), () => session),
    [client, session],
  );
  const navigate = useNavigate();
  const location = useLocation();
  const view = pathView(location.pathname);
  const [notice, setNotice] = useState("");
  const [actionError, setActionError] = useState("");
  const [busyAction, setBusyAction] = useState("");
  const handleFeedback = useCallback((message: string, error = false) => {
    setNotice(error ? "" : message);
    setActionError(error ? message : "");
  }, []);

  const [purchases, setPurchases] = useState<GroupPurchaseSummary[]>([]);
  const [shopState, setShopState] = useState<LoadState>("loading");
  const [shopError, setShopError] = useState("");
  const [carts, setCarts] = useState<CartInfo[]>([]);
  const [cartState, setCartState] = useState<LoadState>("idle");
  const [orders, setOrders] = useState<OrderInfo[]>([]);
  const [orderState, setOrderState] = useState<LoadState>("idle");
  const [balance, setBalance] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    setShopState("loading");
    api.listGroupPurchases().then(
      (page) => {
        if (!active) return;
        setPurchases(page.content);
        setShopState("ready");
      },
      (error: unknown) => {
        if (!active) return;
        setShopError(messageOf(error));
        setShopState("error");
      },
    );
    return () => { active = false; };
  }, [api]);

  const loadCarts = useCallback(async () => {
    if (!session) return;
    setCartState("loading");
    try {
      setCarts((await api.listCarts()).content);
      setCartState("ready");
    } catch (error) {
      setActionError(messageOf(error));
      setCartState("error");
    }
  }, [api, session]);

  const loadOrders = useCallback(async () => {
    if (!session) return;
    setOrderState("loading");
    try {
      setOrders((await api.listOrders()).content);
      setOrderState("ready");
    } catch (error) {
      setActionError(messageOf(error));
      setOrderState("error");
    }
  }, [api, session]);

  useEffect(() => {
    if (view === "cart") void loadCarts();
    if (view === "orders") void loadOrders();
  }, [loadCarts, loadOrders, view]);

  useEffect(() => {
    if (typeof window !== "undefined") window.scrollTo({ top: 0, behavior: "smooth" });
  }, [location.pathname]);

  function changeView(next: View) {
    navigate(viewPath(next));
    setActionError("");
    setNotice("");
  }

  async function addToCart(purchaseId: string, quantity: number) {
    if (!requireSession(session, changeView, setActionError)) return;
    await runAction("cart", async () => {
      await api.addCart({ groupPurchaseId: purchaseId, quantity });
      setNotice("장바구니에 담았습니다.");
    });
  }

  async function createOrder(request: DirectOrderRequest) {
    if (!requireSession(session, changeView, setActionError)) return;
    await runAction("order", async () => {
      await api.createOrder(request);
      setNotice("주문을 만들었습니다. 포인트로 결제해 주세요.");
      navigate("/orders");
    });
  }

  async function runAction(key: string, action: () => Promise<void>) {
    setBusyAction(key);
    setActionError("");
    setNotice("");
    try {
      await action();
    } catch (error) {
      setActionError(messageOf(error));
    } finally {
      setBusyAction("");
    }
  }

  function saveSession(next: CommerceSession | null) {
    setSession(next);
    if (next) sessionStore.setItem(SESSION_KEY, JSON.stringify(next));
    else sessionStore.removeItem(SESSION_KEY);
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <button className="brand" onClick={() => changeView("shop")} aria-label="공동구매 홈으로 이동">
          0909 COMMERCE
          <span>{target === "desktop" ? "DESKTOP" : "WEB"}</span>
        </button>
        <p className="topbar-copy">지금, 더 좋은 것들을 함께</p>
        <nav aria-label="주요 메뉴" className="nav-tabs">
          {(["shop", "cart", "orders", "points", "account"] as const).map((item) => (
            <button key={item} className={view === item ? "is-active" : ""} onClick={() => changeView(item)}>
              <span className="nav-index">0{(["shop", "cart", "orders", "points", "account"] as const).indexOf(item) + 1}</span>
              {viewLabel(item)}
            </button>
          ))}
        </nav>
      </header>

      <main className="app-main">
        {(notice || actionError) && (
          <div role={actionError ? "alert" : "status"} className={`feedback ${actionError ? "feedback--error" : ""}`}>
            {actionError || notice}
          </div>
        )}

        <Routes>
        <Route path="/" element={<ShopScreen purchases={purchases} state={shopState} error={shopError} onSelect={(id) => navigate(`/purchases/${id}`)} />} />
        <Route path="/purchases/:purchaseId" element={
          <PurchaseDetailScreen api={api} busy={busyAction} idFactory={idFactory} onBack={() => navigate("/")} onAddCart={addToCart} onCreateOrder={createOrder} />
        } />
        <Route path="/cart" element={
          <CartScreen
            authenticated={Boolean(session)} carts={carts} state={cartState} busy={busyAction}
            onLogin={() => changeView("account")}
            onReload={loadCarts}
            onUpdate={(id, quantity) => runAction(`cart-${id}`, async () => {
              await api.updateCart(id, quantity); await loadCarts();
            })}
            onDelete={(id) => runAction(`cart-${id}`, async () => {
              await api.deleteCart(id); await loadCarts();
            })}
          />
        } />
        <Route path="/orders" element={
          <OrdersScreen
            authenticated={Boolean(session)} orders={orders} state={orderState} busy={busyAction}
            onLogin={() => changeView("account")}
            onReload={loadOrders}
            onPay={(id) => runAction(`pay-${id}`, async () => {
              const result = await payOrderWithPoints(api, id, idFactory());
              setBalance(result.balance); setNotice("포인트 결제가 완료되었습니다."); await loadOrders();
            })}
            onCancel={(id, reason) => runAction(`cancel-${id}`, async () => {
              await api.cancelOrder(id, { reason: "CHANGE_OF_MIND", detailReason: reason, idempotencyKey: idFactory() });
              setNotice("주문 취소를 요청했습니다."); await loadOrders();
            })}
          />
        } />
        <Route path="/points" element={
          <PointScreen
            authenticated={Boolean(session)} balance={balance} busy={busyAction}
            onLogin={() => changeView("account")}
            onCharge={(amount) => runAction("charge", async () => {
              const result = await api.chargePoints({ amount, transactionId: idFactory() });
              setBalance(result.balance); setNotice(`${amount.toLocaleString()}P를 충전했습니다.`);
            })}
          />
        } />
        <Route path="/account" element={
          <AccountScreen
            session={session} busy={busyAction}
            onAdmin={() => changeView("admin")}
            onLogin={(credentials) => runAction("auth", async () => {
              const next = await loginAndPersist(api, sessionStore, credentials);
              saveSession(next);
              setNotice("로그인했습니다."); navigate("/");
            })}
            onRegister={(credentials) => runAction("auth", async () => {
              await api.register(credentials); setNotice("가입했습니다. 같은 정보로 로그인해 주세요.");
            })}
            onLogout={() => runAction("logout", async () => {
              await api.logout(); saveSession(null); setCarts([]); setOrders([]); setBalance(null);
              setNotice("로그아웃했습니다.");
            })}
          />
        } />
        <Route path="/admin" element={
          session
            ? <SellerAdmin client={api} session={session} idFactory={idFactory} onFeedback={handleFeedback} />
            : <SignInRequired onLogin={() => changeView("account")} message="판매자 관리는 로그인 후 사용할 수 있습니다." />
        } />
        <Route path="*" element={<StatePanel error message="요청한 페이지를 찾을 수 없습니다." action={async () => navigate("/")} actionLabel="홈으로 이동" />} />
        </Routes>
      </main>
    </div>
  );
}

interface ShopProps {
  purchases: GroupPurchaseSummary[]; state: LoadState; error: string;
  onSelect(id: string): void;
}

function ShopScreen(props: ShopProps) {
  return (
    <section aria-labelledby="shop-title">
      <div className="shop-hero">
        <div>
          <span className="hero-kicker">COLLECTIVE BUYING / 2026</span>
          <h1 id="shop-title">지금 같이<br />사는 것들</h1>
        </div>
        <p>좋은 것은<br />혼자보다 함께일 때<br />더 가까워집니다.</p>
        <p className="hero-note">일상의 좋은 것들을<br />함께 고르는<br />0909의 공동구매</p>
      </div>
      <div className="catalog-toolbar" aria-label="상품 목록 정보">
        <strong>ALL DROP</strong>
        <span>{String(props.purchases.length).padStart(2, "0")} ITEMS</span>
        <span>SEOUL / KOREA</span>
      </div>
      {props.state === "loading" && <StatePanel message="공동구매를 불러오는 중입니다." />}
      {props.state === "error" && <StatePanel error message={props.error} />}
      {props.state === "ready" && props.purchases.length === 0 && <StatePanel message="현재 등록된 공동구매가 없습니다." />}
      <div className="purchase-grid">
        {props.purchases.map((purchase) => (
          <article className="purchase-card" key={purchase.groupPurchaseId}>
            <div className="product-visual" style={productVisualStyle(purchase.title, purchase.category)} aria-hidden="true" />
            <div className="card-status">
              <span>{purchase.category.toUpperCase()}</span>
              <span className={`badge badge--${purchase.status.toLowerCase()}`}>{statusLabel(purchase.status)}</span>
            </div>
            <div className="card-body">
              <span className="catalog-number">NO. {shortId(purchase.groupPurchaseId).toUpperCase()}</span>
              <h2>{purchase.title}</h2>
              <p className="price"><strong>{purchase.discountedPrice.toLocaleString()}원</strong><s>{purchase.originalPrice.toLocaleString()}원</s></p>
              <div className="progress" aria-label={`${purchase.currentQuantity}명 참여, 최대 ${purchase.maxQuantity}명`}><span style={{ width: `${Math.min(100, purchase.maxQuantity ? purchase.currentQuantity / purchase.maxQuantity * 100 : 0)}%` }} /></div>
              <div className="card-footer"><span>{purchase.currentQuantity} / {purchase.maxQuantity}명</span><Button variant="secondary" onClick={() => props.onSelect(purchase.groupPurchaseId)}>자세히 보기 <span aria-hidden="true">→</span></Button></div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

interface PurchaseDetailProps {
  purchase: GroupPurchaseDetail; product: ProductDetail | null; busy: string; idFactory: () => string;
  onProduct(id: string): Promise<void>; onAddCart(id: string, quantity: number): Promise<void>;
  onCreateOrder(request: DirectOrderRequest): Promise<void>;
}

function PurchaseDetailScreen({ api, busy, idFactory, onBack, onAddCart, onCreateOrder }: { api: CommerceClient; busy: string; idFactory: () => string; onBack(): void; onAddCart(id: string, quantity: number): Promise<void>; onCreateOrder(request: DirectOrderRequest): Promise<void> }) {
  const { purchaseId } = useParams();
  const [purchase, setPurchase] = useState<GroupPurchaseDetail | null>(null);
  const [product, setProduct] = useState<ProductDetail | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const [error, setError] = useState("");
  const [productBusy, setProductBusy] = useState(false);

  useEffect(() => {
    let active = true;
    if (!purchaseId) { setState("error"); setError("공동구매 번호가 없습니다."); return; }
    setState("loading");
    api.getGroupPurchase(purchaseId).then(
      (value) => { if (active) { setPurchase(value); setState("ready"); } },
      (reason: unknown) => { if (active) { setError(messageOf(reason)); setState("error"); } },
    );
    return () => { active = false; };
  }, [api, purchaseId]);

  useEffect(() => {
    if (state !== "ready" || !purchase || typeof document === "undefined") return;
    document.getElementById("purchase-detail-title")?.focus({ preventScroll: true });
  }, [purchase, state]);

  async function loadProduct(productId: string) {
    setProductBusy(true);
    try { setProduct(await api.getProduct(productId)); }
    catch (reason) { setError(messageOf(reason)); }
    finally { setProductBusy(false); }
  }

  return <section aria-labelledby="purchase-detail-title">
    <div className="detail-heading"><Button variant="secondary" onClick={onBack}>← 목록으로</Button><span className="eyebrow">PURCHASE DETAIL</span></div>
    {state === "loading" && <StatePanel message="상세 정보를 불러오는 중입니다." />}
    {state === "error" && <StatePanel error message={error} action={async () => onBack()} actionLabel="목록으로 이동" />}
    {state === "ready" && error && <div className="feedback feedback--error" role="alert">{error}</div>}
    {purchase && state === "ready" && <PurchaseDetailPanel purchase={purchase} product={product} busy={productBusy ? "product" : busy} idFactory={idFactory} onProduct={loadProduct} onAddCart={onAddCart} onCreateOrder={onCreateOrder} />}
  </section>;
}

function PurchaseDetailPanel(props: PurchaseDetailProps) {
  const [quantity, setQuantity] = useState(1);
  const [showOrder, setShowOrder] = useState(false);
  const [delivery, setDelivery] = useState({ address: "", addressDetail: "", postalCode: "", receiverName: "" });
  const purchase = props.purchase;
  function submitOrder(event: FormEvent) {
    event.preventDefault();
    if (delivery.address.trim().length < 5 || !delivery.addressDetail.trim() || !delivery.postalCode.trim() || !delivery.receiverName.trim()) return;
    void props.onCreateOrder({
      quantity, ...delivery, sellerId: purchase.sellerId,
      groupPurchaseId: purchase.groupPurchaseId, requestId: props.idFactory(),
    });
  }
  return (
    <article className="detail-panel" aria-labelledby="purchase-detail-title">
      <div className="detail-visual product-visual" style={productVisualStyle(purchase.title, purchase.category)} aria-hidden="true" />
      <div className="detail-copy"><span className="eyebrow">SELECTED DROP / {purchase.category.toUpperCase()}</span><h2 id="purchase-detail-title" tabIndex={-1}>{purchase.title}</h2>
        <p>{purchase.description ?? "상세 설명이 없습니다."}</p>
        <dl className="facts"><div><dt>상태</dt><dd>{statusLabel(purchase.status)}</dd></div><div><dt>가격</dt><dd>{purchase.discountedPrice.toLocaleString()}원</dd></div><div><dt>기간</dt><dd>{formatDate(purchase.startDate)} – {formatDate(purchase.endDate)}</dd></div></dl>
        <Button variant="secondary" disabled={props.busy === "product"} onClick={() => void props.onProduct(purchase.productId)}>원상품 정보 보기</Button>
        {props.product && <div className="product-note"><strong>{props.product.name}</strong><span>{props.product.description}</span><span>재고 {props.product.stock}개</span></div>}
      </div>
      <div className="action-panel">
        <label>수량<input type="number" min="1" max={Math.max(1, purchase.maxQuantity - purchase.currentQuantity)} value={quantity} onChange={(e) => setQuantity(Math.max(1, Number(e.target.value)))} /></label>
        {purchase.status === "SCHEDULED" && <p className="availability-note"><strong>{formatDate(purchase.startDate)}</strong>부터 장바구니와 주문을 이용할 수 있습니다.</p>}
        <Button disabled={props.busy === "cart" || purchase.status !== "OPEN"} onClick={() => void props.onAddCart(purchase.groupPurchaseId, quantity)}>장바구니 담기</Button>
        <Button variant="secondary" disabled={purchase.status !== "OPEN"} onClick={() => setShowOrder((value) => !value)}>바로 주문</Button>
        {showOrder && <form className="stack-form" onSubmit={submitOrder}>
          <label>받는 사람<input required maxLength={100} value={delivery.receiverName} onChange={(e) => setDelivery({ ...delivery, receiverName: e.target.value })} /></label>
          <label>주소<input required minLength={5} maxLength={100} value={delivery.address} onChange={(e) => setDelivery({ ...delivery, address: e.target.value })} /></label>
          <label>상세 주소<input required maxLength={100} value={delivery.addressDetail} onChange={(e) => setDelivery({ ...delivery, addressDetail: e.target.value })} /></label>
          <label>우편번호<input required maxLength={50} value={delivery.postalCode} onChange={(e) => setDelivery({ ...delivery, postalCode: e.target.value })} /></label>
          <Button type="submit" disabled={props.busy === "order"}>주문 만들기</Button>
        </form>}
      </div>
    </article>
  );
}

interface CartProps {
  authenticated: boolean; carts: CartInfo[]; state: LoadState; busy: string;
  onLogin(): void; onReload(): Promise<void>; onUpdate(id: string, quantity: number): Promise<void>; onDelete(id: string): Promise<void>;
}
function CartScreen(props: CartProps) {
  if (!props.authenticated) return <SignInRequired onLogin={props.onLogin} />;
  return <section><PageTitle eyebrow="장바구니" title="담아둔 공동구매" />
    {props.state === "loading" && <StatePanel message="장바구니를 불러오는 중입니다." />}
    {props.state === "error" && <StatePanel error message="장바구니를 불러오지 못했습니다." action={props.onReload} />}
    {props.state === "ready" && props.carts.length === 0 && <StatePanel message="장바구니가 비어 있습니다." />}
    <div className="list-stack">{props.carts.map((cart) => <CartRow key={cart.cartId} cart={cart} busy={props.busy === `cart-${cart.cartId}`} onUpdate={props.onUpdate} onDelete={props.onDelete} />)}</div>
  </section>;
}

function CartRow({ cart, busy, onUpdate, onDelete }: { cart: CartInfo; busy: boolean; onUpdate(id: string, quantity: number): Promise<void>; onDelete(id: string): Promise<void> }) {
  const [quantity, setQuantity] = useState(cart.quantity);
  return <article className="list-row"><div><strong>공동구매 {shortId(cart.groupPurchaseId)}</strong><span className="muted">담은 날 {formatDate(cart.createdAt)}</span></div><div className="row-actions"><label className="compact-label">수량<input type="number" min="1" value={quantity} onChange={(e) => setQuantity(Math.max(1, Number(e.target.value)))} /></label><Button disabled={busy || quantity === cart.quantity} onClick={() => void onUpdate(cart.cartId, quantity)}>변경</Button><Button variant="danger" disabled={busy} onClick={() => void onDelete(cart.cartId)}>삭제</Button></div></article>;
}

interface OrdersProps {
  authenticated: boolean; orders: OrderInfo[]; state: LoadState; busy: string;
  onLogin(): void; onReload(): Promise<void>; onPay(id: string): Promise<void>; onCancel(id: string, reason: string): Promise<void>;
}
function OrdersScreen(props: OrdersProps) {
  if (!props.authenticated) return <SignInRequired onLogin={props.onLogin} />;
  return <section><PageTitle eyebrow="주문" title="내 주문" />
    {props.state === "loading" && <StatePanel message="주문을 불러오는 중입니다." />}
    {props.state === "error" && <StatePanel error message="주문을 불러오지 못했습니다." action={props.onReload} />}
    {props.state === "ready" && props.orders.length === 0 && <StatePanel message="아직 주문이 없습니다." />}
    <div className="list-stack">{props.orders.map((order) => <OrderRow key={order.orderId} order={order} busy={props.busy} onPay={props.onPay} onCancel={props.onCancel} />)}</div>
  </section>;
}

function OrderRow({ order, busy, onPay, onCancel }: { order: OrderInfo; busy: string; onPay(id: string): Promise<void>; onCancel(id: string, reason: string): Promise<void> }) {
  const [showCancel, setShowCancel] = useState(false);
  const [reason, setReason] = useState("");
  return <article className="list-row order-row"><div><span className="badge">{order.status}</span><strong>{order.groupPurchaseName ?? `공동구매 ${shortId(order.groupPurchaseId)}`}</strong><span>{order.totalAmount.toLocaleString()}원 · {order.quantity}개</span><span className="muted">{order.orderNumber}</span></div><div className="row-actions">
    {order.status === "PENDING" && <Button disabled={busy === `pay-${order.orderId}`} onClick={() => void onPay(order.orderId)}>포인트 결제</Button>}
    {order.status === "PAYMENT_COMPLETED" && <Button variant="secondary" onClick={() => setShowCancel((value) => !value)}>주문 취소</Button>}
    {showCancel && <form className="inline-form" onSubmit={(e) => { e.preventDefault(); if (reason.trim()) void onCancel(order.orderId, reason); }}><label>취소 사유<input required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} /></label><Button variant="danger" type="submit" disabled={busy === `cancel-${order.orderId}`}>취소 요청</Button></form>}
  </div></article>;
}

function PointScreen({ authenticated, balance, busy, onLogin, onCharge }: { authenticated: boolean; balance: number | null; busy: string; onLogin(): void; onCharge(amount: number): Promise<void> }) {
  const [amount, setAmount] = useState(10000);
  if (!authenticated) return <SignInRequired onLogin={onLogin} />;
  return <section><PageTitle eyebrow="로컬 포인트" title="결제 잔액" /><div className="point-card"><span>현재 세션에서 확인한 잔액</span><strong>{balance === null ? "충전 또는 결제 후 표시됩니다" : `${balance.toLocaleString()}P`}</strong><form className="charge-form" onSubmit={(e) => { e.preventDefault(); if (amount > 0) void onCharge(amount); }}><label>충전할 포인트<input type="number" min="1" step="1000" value={amount} onChange={(e) => setAmount(Number(e.target.value))} /></label><Button type="submit" disabled={busy === "charge" || amount < 1}>포인트 충전</Button></form><p className="muted">로컬 개발 환경에서만 사용하는 테스트 포인트입니다.</p></div></section>;
}

function AccountScreen({ session, busy, onAdmin, onLogin, onRegister, onLogout }: { session: CommerceSession | null; busy: string; onAdmin(): void; onLogin(credentials: { username: string; password: string }): Promise<void>; onRegister(credentials: { username: string; password: string }): Promise<void>; onLogout(): Promise<void> }) {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const valid = username.trim().length >= 3 && password.length >= 8;
  if (session) return <section><PageTitle eyebrow="계정" title="로그인 상태" /><div className="account-card"><span className="muted">회원 ID</span><strong>{session.memberId}</strong><Button onClick={onAdmin}>판매 관리로 이동</Button><Button variant="secondary" disabled={busy === "logout"} onClick={() => void onLogout()}>로그아웃</Button></div></section>;
  return <section><PageTitle eyebrow="계정" title={mode === "login" ? "로그인" : "회원가입"} /><div className="auth-card"><div className="segmented"><button className={mode === "login" ? "is-active" : ""} onClick={() => setMode("login")}>로그인</button><button className={mode === "register" ? "is-active" : ""} onClick={() => setMode("register")}>회원가입</button></div><form className="stack-form" onSubmit={(e) => { e.preventDefault(); if (!valid) return; void (mode === "login" ? onLogin({ username: username.trim(), password }) : onRegister({ username: username.trim(), password })); }}><label>아이디<input autoComplete="username" minLength={3} maxLength={64} required value={username} onChange={(e) => setUsername(e.target.value)} /></label><label>비밀번호<input type="password" autoComplete={mode === "login" ? "current-password" : "new-password"} minLength={8} maxLength={256} required value={password} onChange={(e) => setPassword(e.target.value)} /></label><span className="form-help">아이디 3자 이상, 비밀번호 8자 이상 입력해 주세요.</span><Button type="submit" disabled={!valid || busy === "auth"}>{mode === "login" ? "로그인" : "가입하기"}</Button></form></div></section>;
}

function SignInRequired({ onLogin, message = "장바구니, 주문과 포인트는 로그인 후 사용할 수 있습니다." }: { onLogin(): void; message?: string }) { return <section><PageTitle eyebrow="로그인 필요" title="계정에 먼저 로그인해 주세요" /><StatePanel message={message} action={async () => onLogin()} actionLabel="로그인으로 이동" /></section>; }
function PageTitle({ eyebrow, title }: { eyebrow: string; title: string }) { return <div className="page-heading"><div><span className="eyebrow">{eyebrow}</span><h1>{title}</h1></div></div>; }
function StatePanel({ message, error = false, action, actionLabel = "다시 시도" }: { message: string; error?: boolean; action?: () => Promise<void>; actionLabel?: string }) { return <div className={`state-panel ${error ? "state-panel--error" : ""}`} role={error ? "alert" : "status"}><p>{message}</p>{action && <Button variant="secondary" onClick={() => void action()}>{actionLabel}</Button>}</div>; }

function requireSession(session: CommerceSession | null, navigate: (view: View) => void, setError: (message: string) => void): session is CommerceSession {
  if (session) return true;
  setError("로그인 후 이용해 주세요."); navigate("account"); return false;
}
function viewLabel(view: View) { return ({ shop: "공동구매", cart: "장바구니", orders: "주문", points: "포인트", account: "계정", admin: "판매 관리" } as const)[view]; }
function viewPath(view: View) { return ({ shop: "/", cart: "/cart", orders: "/orders", points: "/points", account: "/account", admin: "/admin" } as const)[view]; }
function pathView(pathname: string): View {
  if (pathname.startsWith("/purchases/")) return "shop";
  return ({ "/": "shop", "/cart": "cart", "/orders": "orders", "/points": "points", "/account": "account", "/admin": "admin" } as Record<string, View>)[pathname] ?? "shop";
}
function statusLabel(status: string) { return ({ SCHEDULED: "오픈 예정", OPEN: "진행 중", CLOSED: "마감", FAILED: "무산" } as Record<string, string>)[status] ?? status; }
function productVisualIndex(title: string, category: string) {
  const value = `${title} ${category}`.toLowerCase();
  if (/사과|apple/.test(value)) return 1;
  if (/커피|coffee/.test(value)) return 2;
  if (/램프|조명|lamp/.test(value)) return 3;
  if (/스피커|사운드|speaker/.test(value)) return 4;
  if (/요가|홈트|mat/.test(value)) return 5;
  return 0;
}
function productVisualStyle(title: string, category: string): CSSProperties {
  const index = productVisualIndex(title, category);
  return { "--visual-x": `${(index % 3) * 50}%`, "--visual-y": `${Math.floor(index / 3) * 100}%` } as CSSProperties;
}
function messageOf(error: unknown) { return error instanceof ApiError || error instanceof Error ? error.message : "요청을 처리하지 못했습니다."; }
function shortId(value: string) { return value.slice(0, 8); }
function formatDate(value: string) { const date = new Date(value); return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat("ko-KR", { dateStyle: "medium" }).format(date); }
function createId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (character) => {
    const random = Math.floor(Math.random() * 16);
    return (character === "x" ? random : (random & 0x3) | 0x8).toString(16);
  });
}
function defaultApiBaseUrl() { return typeof location !== "undefined" && /^https?:$/.test(location.protocol) ? location.origin : "http://127.0.0.1:8080"; }
function browserStorage(): StorageLike { try { return localStorage; } catch { const data = new Map<string, string>(); return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => { data.set(key, value); }, removeItem: (key) => { data.delete(key); } }; } }
function readSession(storage: StorageLike): CommerceSession | null { try { const raw = storage.getItem(SESSION_KEY); if (!raw) return null; const value = JSON.parse(raw) as Partial<CommerceSession>; return typeof value.accessToken === "string" && typeof value.memberId === "string" ? { accessToken: value.accessToken, memberId: value.memberId } : null; } catch { return null; } }

export async function loginAndPersist(
  client: CommerceClient,
  storage: StorageLike,
  credentials: Credentials,
): Promise<CommerceSession> {
  const login = await client.login(credentials);
  const session = { accessToken: login.accessToken, memberId: login.memberId };
  storage.setItem(SESSION_KEY, JSON.stringify(session));
  return session;
}

export function payOrderWithPoints(
  client: CommerceClient,
  orderId: string,
  transactionId: string,
): Promise<PointPaymentResult> {
  return client.payOrderWithPoints(orderId, { transactionId });
}
