// SPDX-License-Identifier: AGPL-3.0-only
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./options.css";

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(<App />);
}
