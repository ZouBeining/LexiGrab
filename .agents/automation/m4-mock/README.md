# M4 远程源与缓存验收自动化

用 Python 标准库起一个本地 mock（远程词典 JSONPath / 脚本、JEV-like 选择器、
OpenAI-compatible LLM），再用开发 CLI 跑真实 HTTP 端到端，验证：

- 远程 JSONPath 源与脚本源的解析、`source_name` 填充、HTML 清洗与相对资源丢弃；
- 失败源（HTTP 500）被隔离，本地结果仍正常返回；
- 空结果不报错（CLI 退出码 4）；
- 同仓 `cargo test --test sense_assist_tests` 覆盖选择器/LLM 的 wiremock 契约与状态机；
- `cargo test --test perf_tests -- --ignored --nocapture` 给出缓存与 JSONPath 延迟。

## 前置条件

- Windows；`python` 可用；
- 已 `cargo fetch`（首次构建需要网络）；
- 夹具 `src-tauri/tests/fixtures/mini.mdx` 存在。

## 运行

```powershell
python -u .agents/automation/m4-mock/run.py
```

脚本自动构建 `lexigrab-dev`、选择空闲端口、生成临时配置与脚本，结束后写
`artifacts/results.json` 并以 0/1 退出。`artifacts/` 不入库。

## 文件

| 文件 | 作用 |
| --- | --- |
| `mock_server.py` | 本地 mock：`/dict`、`/script`、`/fail`、`/v1/select`、`/v1/chat/completions` |
| `run.py` | 启动 mock、生成配置、调用 CLI、断言并汇总 |
