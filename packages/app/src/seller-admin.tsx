import type { CommerceClient, CommerceSession } from "@commerce/api-client";
import type {
  GroupPurchaseDetail, GroupPurchaseSummary, GroupPurchaseWriteRequest,
  ProductCreateRequest, ProductDetail, ProductListItem, ProductUpdateRequest,
} from "@commerce/contracts";
import { type FormEvent, useCallback, useEffect, useState } from "react";

const CATEGORIES = ["HOME", "FOOD", "HEALTH", "BEAUTY", "FASHION", "ELECTRONICS", "KIDS", "HOBBY", "PET"];

interface SellerAdminProps {
  client: CommerceClient;
  session: CommerceSession;
  idFactory(): string;
  onFeedback(message: string, error?: boolean): void;
}

type ProductForm = Omit<ProductCreateRequest, "idempotencyKey" | "imageUrl"> & { productId?: string };
type PurchaseForm = Omit<GroupPurchaseWriteRequest, "imageUrl"> & { groupPurchaseId?: string };

const emptyProduct: ProductForm = { name: "", price: 0, category: "HOME", description: "", stock: 0, originalUrl: null };
const emptyPurchase: PurchaseForm = {
  minQuantity: 1, maxQuantity: 10, title: "", description: "", discountedPrice: 0,
  startDate: "", endDate: "", productId: "",
};

export function SellerAdmin({ client, session, idFactory, onFeedback }: SellerAdminProps) {
  const [tab, setTab] = useState<"products" | "purchases">("products");
  const [products, setProducts] = useState<ProductListItem[]>([]);
  const [purchases, setPurchases] = useState<GroupPurchaseSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [productForm, setProductForm] = useState<ProductForm | null>(null);
  const [purchaseForm, setPurchaseForm] = useState<PurchaseForm | null>(null);
  const [deleteTarget, setDeleteTarget] = useState("");

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const [productPage, purchasePage] = await Promise.all([
        client.listSellerProducts(), client.listSellerGroupPurchases(session.memberId),
      ]);
      setProducts(productPage.content);
      setPurchases(purchasePage.content);
    } catch (error) {
      onFeedback(messageOf(error), true);
    } finally {
      setLoading(false);
    }
  }, [client, onFeedback, session.memberId]);

  useEffect(() => { void reload(); }, [reload]);

  async function editProduct(productId: string) {
    setBusy(`product-${productId}`);
    try {
      const product = await client.getProduct(productId);
      setProductForm(productToForm(product));
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  async function saveProduct(event: FormEvent) {
    event.preventDefault();
    if (!productForm) return;
    setBusy("product-save");
    try {
      if (productForm.productId) {
        const request: ProductUpdateRequest = {
          name: productForm.name, price: productForm.price, category: productForm.category,
          description: productForm.description, stock: productForm.stock,
          originalLink: productForm.originalUrl, imageUrl: null,
        };
        await client.updateProduct(productForm.productId, request);
        onFeedback("상품을 수정했습니다.");
      } else {
        await client.createProduct({ ...productForm, imageUrl: null, idempotencyKey: idFactory() });
        onFeedback("상품을 등록했습니다.");
      }
      setProductForm(null);
      await reload();
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  async function removeProduct(productId: string) {
    setBusy(`product-${productId}`);
    try {
      await client.deleteProduct(productId);
      setDeleteTarget("");
      onFeedback("상품을 삭제했습니다.");
      await reload();
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  async function editPurchase(groupPurchaseId: string) {
    setBusy(`purchase-${groupPurchaseId}`);
    try {
      const purchase = await client.getGroupPurchase(groupPurchaseId);
      setPurchaseForm(purchaseToForm(purchase));
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  async function savePurchase(event: FormEvent) {
    event.preventDefault();
    if (!purchaseForm) return;
    const { groupPurchaseId, ...values } = purchaseForm;
    const request: GroupPurchaseWriteRequest = {
      ...values, imageUrl: null,
      startDate: new Date(values.startDate).toISOString(),
      endDate: new Date(values.endDate).toISOString(),
    };
    setBusy("purchase-save");
    try {
      if (groupPurchaseId) {
        await client.updateGroupPurchase(groupPurchaseId, request);
        onFeedback("공동구매를 수정했습니다.");
      } else {
        await client.createGroupPurchase(request);
        onFeedback("공동구매를 등록했습니다.");
      }
      setPurchaseForm(null);
      await reload();
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  async function removePurchase(groupPurchaseId: string) {
    setBusy(`purchase-${groupPurchaseId}`);
    try {
      await client.deleteGroupPurchase(groupPurchaseId);
      setDeleteTarget("");
      onFeedback("공동구매를 삭제했습니다.");
      await reload();
    } catch (error) { onFeedback(messageOf(error), true); }
    finally { setBusy(""); }
  }

  return <section aria-labelledby="seller-admin-title">
    <div className="page-heading admin-heading"><div><span className="eyebrow">SELLER CONSOLE</span><h1 id="seller-admin-title">판매 관리</h1></div><p>로그인한 판매자의 상품과 공동구매만 관리합니다.</p></div>
    <div className="admin-tabs" aria-label="판매 관리 대상">
      <button aria-pressed={tab === "products"} className={tab === "products" ? "is-active" : ""} onClick={() => setTab("products")}>상품 <span>{products.length}</span></button>
      <button aria-pressed={tab === "purchases"} className={tab === "purchases" ? "is-active" : ""} onClick={() => setTab("purchases")}>공동구매 <span>{purchases.length}</span></button>
    </div>
    {loading && <AdminState message="판매 정보를 불러오는 중입니다." />}
    {!loading && tab === "products" && <>
      <div className="admin-toolbar"><div><strong>등록 상품</strong><span>공동구매를 만들기 전에 상품을 먼저 등록하세요.</span></div><button className="button" onClick={() => setProductForm({ ...emptyProduct })}>새 상품</button></div>
      {productForm && <ProductEditor value={productForm} busy={busy === "product-save"} onChange={setProductForm} onCancel={() => setProductForm(null)} onSubmit={saveProduct} />}
      {products.length === 0 ? <AdminState message="등록한 상품이 없습니다." /> : <div className="admin-list">{products.map((product) => <article className="admin-row" key={product.productId}>
        <div><span className="eyebrow">{product.category}</span><strong>{product.name}</strong><span>{product.price.toLocaleString()}원</span></div>
        <div className="admin-row-actions"><button className="button button--secondary" disabled={busy === `product-${product.productId}`} onClick={() => void editProduct(product.productId)}>수정</button>{deleteTarget === product.productId ? <><span className="delete-copy">정말 삭제할까요?</span><button className="button button--danger" onClick={() => void removeProduct(product.productId)}>삭제 확인</button><button className="button button--secondary" onClick={() => setDeleteTarget("")}>취소</button></> : <button className="button button--danger" onClick={() => setDeleteTarget(product.productId)}>삭제</button>}</div>
      </article>)}</div>}
    </>}
    {!loading && tab === "purchases" && <>
      <div className="admin-toolbar"><div><strong>공동구매</strong><span>등록한 상품을 골라 판매 일정과 수량을 설정하세요. 오픈 예정 공동구매만 삭제할 수 있습니다.</span></div><button className="button" disabled={products.length === 0} onClick={() => setPurchaseForm({ ...emptyPurchase, productId: products[0]?.productId ?? "" })}>새 공동구매</button></div>
      {purchaseForm && <PurchaseEditor value={purchaseForm} products={products} busy={busy === "purchase-save"} onChange={setPurchaseForm} onCancel={() => setPurchaseForm(null)} onSubmit={savePurchase} />}
      {purchases.length === 0 ? <AdminState message="등록한 공동구매가 없습니다." /> : <div className="admin-list">{purchases.map((purchase) => <article className="admin-row" key={purchase.groupPurchaseId}>
        <div><span className={`badge badge--${purchase.status.toLowerCase()}`}>{statusLabel(purchase.status)}</span><strong>{purchase.title}</strong><span>{purchase.discountedPrice.toLocaleString()}원 · {purchase.currentQuantity}/{purchase.maxQuantity}명</span></div>
        <div className="admin-row-actions"><button className="button button--secondary" disabled={busy === `purchase-${purchase.groupPurchaseId}`} onClick={() => void editPurchase(purchase.groupPurchaseId)}>수정</button>{deleteTarget === purchase.groupPurchaseId ? <><span className="delete-copy">정말 삭제할까요?</span><button className="button button--danger" onClick={() => void removePurchase(purchase.groupPurchaseId)}>삭제 확인</button><button className="button button--secondary" onClick={() => setDeleteTarget("")}>취소</button></> : <button className="button button--danger" disabled={purchase.status !== "SCHEDULED"} onClick={() => setDeleteTarget(purchase.groupPurchaseId)}>삭제</button>}</div>
      </article>)}</div>}
    </>}
  </section>;
}

function ProductEditor({ value, busy, onChange, onCancel, onSubmit }: { value: ProductForm; busy: boolean; onChange(value: ProductForm): void; onCancel(): void; onSubmit(event: FormEvent): void }) {
  return <form className="admin-editor" onSubmit={onSubmit}><div className="admin-editor-title"><strong>{value.productId ? "상품 수정" : "새 상품"}</strong><button type="button" className="text-button" onClick={onCancel}>닫기 ×</button></div><div className="admin-form-grid">
    <label>상품명<input required maxLength={100} value={value.name} onChange={(e) => onChange({ ...value, name: e.target.value })} /></label>
    <label>가격<input required type="number" min="0" value={value.price} onChange={(e) => onChange({ ...value, price: Number(e.target.value) })} /></label>
    <label>카테고리<select value={value.category} onChange={(e) => onChange({ ...value, category: e.target.value })}>{CATEGORIES.map((category) => <option key={category}>{category}</option>)}</select></label>
    <label>재고<input required type="number" min="0" value={value.stock} onChange={(e) => onChange({ ...value, stock: Number(e.target.value) })} /></label>
    <label className="span-2">설명<textarea required maxLength={2000} value={value.description} onChange={(e) => onChange({ ...value, description: e.target.value })} /></label>
    <label className="span-2">원상품 링크 <span className="optional">선택</span><input type="url" value={value.originalUrl ?? ""} onChange={(e) => onChange({ ...value, originalUrl: e.target.value || null })} /></label>
  </div><div className="editor-actions"><button type="button" className="button button--secondary" onClick={onCancel}>취소</button><button className="button" disabled={busy}>{busy ? "저장 중…" : "저장"}</button></div></form>;
}

function PurchaseEditor({ value, products, busy, onChange, onCancel, onSubmit }: { value: PurchaseForm; products: ProductListItem[]; busy: boolean; onChange(value: PurchaseForm): void; onCancel(): void; onSubmit(event: FormEvent): void }) {
  return <form className="admin-editor" onSubmit={onSubmit}><div className="admin-editor-title"><strong>{value.groupPurchaseId ? "공동구매 수정" : "새 공동구매"}</strong><button type="button" className="text-button" onClick={onCancel}>닫기 ×</button></div><div className="admin-form-grid">
    <label className="span-2">상품<select required value={value.productId} onChange={(e) => onChange({ ...value, productId: e.target.value })}>{products.map((product) => <option value={product.productId} key={product.productId}>{product.name}</option>)}</select></label>
    <label className="span-2">제목<input required maxLength={100} value={value.title} onChange={(e) => onChange({ ...value, title: e.target.value })} /></label>
    <label>최소 수량<input required type="number" min="1" value={value.minQuantity} onChange={(e) => onChange({ ...value, minQuantity: Number(e.target.value) })} /></label>
    <label>최대 수량<input required type="number" min={value.minQuantity} value={value.maxQuantity} onChange={(e) => onChange({ ...value, maxQuantity: Number(e.target.value) })} /></label>
    <label>공동구매가<input required type="number" min="0" value={value.discountedPrice} onChange={(e) => onChange({ ...value, discountedPrice: Number(e.target.value) })} /></label>
    <label>시작 일시<input required type="datetime-local" value={value.startDate} onChange={(e) => onChange({ ...value, startDate: e.target.value })} /></label>
    <label>종료 일시<input required type="datetime-local" min={value.startDate} value={value.endDate} onChange={(e) => onChange({ ...value, endDate: e.target.value })} /></label>
    <label className="span-2">설명 <span className="optional">선택</span><textarea maxLength={2000} value={value.description ?? ""} onChange={(e) => onChange({ ...value, description: e.target.value || null })} /></label>
  </div><div className="editor-actions"><button type="button" className="button button--secondary" onClick={onCancel}>취소</button><button className="button" disabled={busy}>{busy ? "저장 중…" : "저장"}</button></div></form>;
}

function AdminState({ message }: { message: string }) { return <div className="state-panel" role="status"><p>{message}</p></div>; }
function productToForm(product: ProductDetail): ProductForm { return { productId: product.productId, name: product.name, price: product.price, category: product.category, description: product.description, stock: product.stock, originalUrl: product.originalLink }; }
function purchaseToForm(purchase: GroupPurchaseDetail): PurchaseForm { return { groupPurchaseId: purchase.groupPurchaseId, minQuantity: purchase.minQuantity, maxQuantity: purchase.maxQuantity, title: purchase.title, description: purchase.description, discountedPrice: purchase.discountedPrice, startDate: toLocalDateTime(purchase.startDate), endDate: toLocalDateTime(purchase.endDate), productId: purchase.productId }; }
function toLocalDateTime(value: string) { const date = new Date(value); const offset = date.getTimezoneOffset() * 60_000; return new Date(date.getTime() - offset).toISOString().slice(0, 16); }
function statusLabel(status: string) { return ({ SCHEDULED: "오픈 예정", OPEN: "진행 중", CLOSED: "마감", SUCCESS: "성공", FAILED: "무산" } as Record<string, string>)[status] ?? status; }
function messageOf(error: unknown) { return error instanceof Error ? error.message : "요청을 처리하지 못했습니다."; }
