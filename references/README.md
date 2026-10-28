# references

本目录放本地研究材料，不进入源码树。`references/*` 已被 `.gitignore` 忽略，只有本 README 入库。

## ODH

- 仓库：<https://github.com/ninja33/ODH>
- 本地副本：`references/ODH`
- 固定研究版本：`ce5e7daad25f83dbca1d5755905ba599f93fc3ac`（2026-10-19）
- 许可证：MIT，见 `references/ODH/LICENSE`
- 用途：研究浏览器划词、句子提取、在线词典脚本与 AnkiConnect 封装

克隆命令：

```powershell
git -c http.sslBackend=openssl clone --depth 1 https://github.com/ninja33/ODH.git references/ODH
```

如果本机 Git 的 Schannel 不可用，`http.sslBackend=openssl` 可以绕过凭证错误。

## 阅读地图

| 主题 | 文件 |
| --- | --- |
| 选区与块级祖先 | `src/fg/js/text.js` |
| Range 偏移与单词边界 | `src/fg/js/range.js` |
| 结果弹层与页面交互 | `src/fg/js/frontend.js`、`src/fg/js/popup.js` |
| 在线词典脚本 | `src/dict/*.js` |
| AnkiConnect 调用 | `src/bg/js/ankiconnect.js` |
| 扩展清单与权限 | `src/manifest.json` |
| 开发说明 | `doc/development.md` |

## 使用边界

- 只提炼算法与契约，不复制代码、类结构、HTML/CSS 或文案。
- ODH 为 MIT，本项目为 AGPL-3.0；复制片段会引入许可证与归属负担，MVP 明确禁止。
- 我们的实现使用 TypeScript、Shadow DOM、WXT、本地桥接与 Tauri，和 ODH 的 MV3 service worker + 页面注入结构不同。
- 研究结论写入对应的 PLAN 或 `PROGRESS.md`，不把参考仓库内容提交进仓库。
