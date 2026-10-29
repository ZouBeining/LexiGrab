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
