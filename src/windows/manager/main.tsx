// SPDX-License-Identifier: AGPL-3.0-only
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../../styles/tailwind.css";
import { applyTheme, readStoredTheme } from "../../lib/theme";
import { App } from "./App";

applyTheme(readStoredTheme());

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
