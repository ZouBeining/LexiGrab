// SPDX-License-Identifier: AGPL-3.0-only
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../../styles/tailwind.css";
import { applyTheme, readStoredTheme } from "../../lib/theme";
import { PopupApp } from "./PopupApp";

applyTheme(readStoredTheme());

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <PopupApp />
    </StrictMode>,
  );
}
