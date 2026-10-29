# MDX 测试夹具

本目录的二进制夹具用于 M1 的 MDX 解析、资源、路由与 CLI 集成测试。内容为自拟的合成数据，不含任何受版权保护的词典内容，随仓库以 AGPL-3.0 分发。

## 文件

| 文件 | 大小 | SHA256 | 说明 |
| --- | --- | --- | --- |
| `mini.mdx` | 951 B | `3162c27c3fa34478ce22d056aacf358c305fd27b140b377f0a396d3aba9a9e68` | MDX v2.0，3 条：`hello`、`world`、`lexigrab` |
| `mini.mdd` | 663 B | `b1527b5ab036e8d44992c7dba77c82e96e19f59169ea1f8ba6d7312bdf8a9a82` | 2 个资源：`img/hello.png`（1×1 PNG）、`audio/hello.mp3`（占位字节） |
| `broken.mdx` | 100 B | `e4641da27cd29ffdca4b5f0a5971f76a6c1560dc05ae0936586c5e119878807b` | `mini.mdx` 前 100 字节，用于验证打开失败 |
| `mini-config.toml` | 1.2 KB | — | 指向 `mini.mdx` 的完整 M1 配置 |

`hello` 的释义为 `<ol><li>` 列表，供 T1.11 义项提取测试；同时含粗体、图片与音频占位，供 T1.5 资源改写测试。

## 生成方式

原计划用 Python `mdict-utils` 生成，但为避免额外 Python 依赖，改为使用项目已依赖的 `mdictlib`（MIT）自带写入器：

```powershell
cd src-tauri
cargo run --example gen_fixtures
```

生成器源码：[`src-tauri/examples/gen_fixtures.rs`](../../examples/gen_fixtures.rs)。脚本会重写本目录的 `mini.mdx` / `mini.mdd` / `broken.mdx` 并自检 `hello` 查询与 PNG 资源读取。

CI 不运行生成器，只使用提交的二进制夹具。修改夹具后需同步更新上表的大小与 SHA256。
