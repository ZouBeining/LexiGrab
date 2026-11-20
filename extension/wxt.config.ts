// SPDX-License-Identifier: AGPL-3.0-only
import { defineConfig } from "wxt";
import tailwindcss from "@tailwindcss/vite";

/**
 * Firefox MV2 扩展（TECH_STACK 第 3 节）。
 *
 * - gecko.id 固定 `lexigrab@localhost`，否则重装后 storage.local 里的配对信息丢失。
 * - 宿主权限只给 `http://127.0.0.1/*`，不申请 tabs/history/bookmarks。
 * - content script 的 CSS 编译进 Shadow Root，不受页面样式影响。
 */
export default defineConfig({
  manifestVersion: 2,
  modules: ["@wxt-dev/module-react"],
  manifest: {
    name: "LexiGrab",
    description: "在 Firefox 中划词，用桌面端词典查询并直接制卡。",
    version: "0.1.0",
    permissions: ["storage", "http://127.0.0.1/*"],
    browser_specific_settings: {
      gecko: {
        id: "lexigrab@localhost",
        strict_min_version: "109.0",
      },
    },
  },
  vite: () => ({
    plugins: [tailwindcss()],
  }),
});
