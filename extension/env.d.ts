/// <reference types="vite/client" />

/** Vite 的 `?inline` CSS 导入返回字符串，用于注入 Shadow Root。 */
declare module "*.css?inline" {
  const css: string;
  export default css;
}
