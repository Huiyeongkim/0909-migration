import { CommerceApp } from "@commerce/app";
import React from "react";
import ReactDOM from "react-dom/client";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <CommerceApp target="web" />
  </React.StrictMode>,
);
