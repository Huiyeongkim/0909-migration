import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CommerceApp } from "./commerce-app";

describe("CommerceApp", () => {
  it("identifies the selected client target", () => {
    expect(renderToStaticMarkup(<CommerceApp target="web" />)).toContain("웹 화면");
    expect(renderToStaticMarkup(<CommerceApp target="desktop" />)).toContain(
      "Tauri 데스크톱 화면",
    );
  });
});
