import "./styles.css";

export interface CommerceAppProps {
  target: "web" | "desktop";
}

export function CommerceApp({ target }: CommerceAppProps) {
  return (
    <main className="commerce-foundation">
      <p className="commerce-foundation__eyebrow">Commerce migration</p>
      <h1>공통 클라이언트 기반</h1>
      <p>
        {target === "desktop" ? "Tauri 데스크톱" : "웹"} 화면이 같은 React
        애플리케이션을 사용합니다.
      </p>
    </main>
  );
}
