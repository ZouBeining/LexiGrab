import { defineConfig } from "vitest/config";

/** 只测纯逻辑（句子提取、桥接客户端、气泡组件），不加载扩展运行时。 */
export default defineConfig({
  test: {
    environment: "happy-dom",
    globals: true,
    include: ["tests/**/*.test.{ts,tsx}"],
    restoreMocks: true,
  },
});
