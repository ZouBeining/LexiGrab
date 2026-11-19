# M3 Firefox 真机验收自动化

用 Firefox Developer Edition 自带 Marionette 驱动真实浏览器，覆盖 `.xpi` 安装、
选项页配对、划词→查询→Anki 制卡、令牌轮换、重启保留、真实站点矩阵与端口冲突。
不依赖 geckodriver / selenium，只用 Python 标准库。

## 前置条件

- Windows 交互桌面；Firefox Developer Edition 安装于 `D:\Firefox Developer Edition`
  （路径写死在 `ffx.py` 的 `FIREFOX_EXE`）。
- `src-tauri/target/debug/lexigrab.exe` 已构建；词典夹具 `src-tauri/tests/fixtures/mini.mdx` 存在。
- Anki 已安装于 `D:\Anki\Anki.exe`，且 `addons21/2055492159` 安装了 AnkiConnect；
  `anki_api.ensure_fixtures()` 会创建 `LexiGrab E2E` 牌组与 `LexiGrab Basic` 笔记类型。
- 若 Firefox 访问境外站点，先确认系统代理可用（自动化 profile 使用 `network.proxy.type=5`）。

## 运行

```powershell
cd .agents/automation/m3-firefox
python -u run.py
```

结果写入 `artifacts/results.json`；截图、`firefox.log`、临时 profile 同在 `artifacts/`。
`run.py` 会先删除 `artifacts/profile`、重启桌面端与 Anki，最后写汇总并以 0/2 退出。

## 文件

| 文件 | 作用 |
| --- | --- |
| `marionette.py` | Marionette 协议客户端（长度前缀 JSON、命令、上传扩展） |
| `ffx.py` | 独立 profile 启动/关闭 Firefox，读取扩展 host 与 Add-on 状态 |
| `desktop.py` | 生成临时配置、从 `%TEMP%` 启动 `lexigrab.exe`、桥接 HTTP 助手 |
| `testpage.py` | 本地中英文/新闻/同词多句测试页（127.0.0.1:8766） |
| `anki_api.py` | AnkiConnect 助手：建牌组/模板、查笔记 |
| `run.py` | 29 个验收场景与结果汇总 |
| `probe_style.py` | 复验气泡在 Shadow DOM 内的计算样式（border/shadow） |
