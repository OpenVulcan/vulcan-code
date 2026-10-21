<div align="center">

<img src="apps/vulcan-code-web/public/vulcan-logo.png" alt="Vulcan Code" width="120" />

# Vulcan Code

**開源的圖形介面（GUI）AI Coding 桌面工作台**

Rust Core · Electron 桌面殼 · React WebUI

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Issues](https://img.shields.io/github/issues/OpenVulcan/vulcan-code)](https://github.com/OpenVulcan/vulcan-code/issues)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/OpenVulcan/vulcan-code/pulls)

[简体中文](README.md) | [English](README.en.md) | **繁體中文** | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md) | [Português](README.pt-BR.md)

</div>

> 本文為[簡體中文 README](README.md) 的翻譯；兩者不一致時，以簡體中文版為準。歡迎透過 PR 修正翻譯。

---

## 關於專案

Vulcan Code 是一款**圖形介面的 AI Coding 軟體**。它把專案、對話、終端機、瀏覽器、程式碼審查與 Git 提交整合到同一個桌面工作台，讓 AI 能理解你的程式碼、推進任務，並交付可以檢查的成果。

這個專案已在內部開發與除錯了很長一段時間，現在以 **MIT 授權**開源。我們希望它不只是一個團隊的產品，而是由社群一起打磨的工具：

- 🐛 遇到問題或有想法，歡迎提出 [Issue](https://github.com/OpenVulcan/vulcan-code/issues)
- 🔧 修了 bug 或做了新功能，歡迎提交 [Pull Request](https://github.com/OpenVulcan/vulcan-code/pulls)
- 🤝 無論你是使用者還是開發者，都可以參與共建，一起打造大家都喜愛的 AI Coding 軟體

## 特色功能

### 🧭 桌面工作台

- 多專案、多對話管理，檔案樹、審查、記憶、子代理等面板自由組合
- 內建專案終端機，支援多分頁；AI 可以持有長時間執行或需要互動的命令工作階段
- 串流回覆，Markdown、程式碼區塊、Mermaid/SVG 圖形與工具呼叫卡片直接渲染
- 可以編輯歷史提問重新生成，也可以從任一已完成的回合分岔出新對話
- 中英文介面，明暗主題

### 🎯 任務推進

- **Goal 目標模式**：設定目標與 Token 預算，AI 逐回合持續推進，可暫停、恢復、清除
- **Plan 計畫模式**：先探索、產出計畫，確認後再執行
- **執行中引導**：任務執行時可以追加說明、排隊後續請求
- **子代理協作**：內建探索、計畫等代理角色，另附 [agency-agents](https://github.com/msitarzewski/agency-agents) 的 260+ 個角色範本（預設關閉，按需啟用）
- **結構化提問**：AI 遇到需要你決定的問題時，以選項卡片向你提問

### 🧠 理解程式碼與累積經驗

- **VCE 程式碼語意索引**：為專案建立向量索引，AI 先找到相關實作再動手修改；索引隨程式碼變更自動同步
- **專案知識庫**：匯入 Markdown 資料目錄，支援 BM25 與向量混合檢索，可依專案授權、依對話選擇
- **MemoryMesh 長期記憶**：回合結束後自動提煉經驗，回合開始前自動召回；記憶可以檢視、修正、刪除，並提供資源回收筒與來源追溯

### 🔍 成果可檢查

- **Diff 審查**：統一/並排檢視、字詞級差異、全文搜尋，支援暫存、還原
- **Git 提交**：AI 產生提交訊息，可提交到目前分支或新分支，並直接推送
- **AI 瀏覽器**：AI 在真實的 Chromium 頁面中導覽、點擊、截圖、讀取主控台；你可以隨時接管控制權

### 🧩 可擴充

- **提示詞技能**（`SKILL.md`）：把團隊的做事方法沉澱下來，下次直接重用
- **LuaSkills**：把常用動作封裝成可設定、可安裝的程式化能力包
- **MCP**：支援本機程序與 HTTP 兩種 MCP 伺服器，附連線診斷與工具 Schema 檢視
- **外掛系統**：外掛可以提供工具，也可以掛載自己的介面。內建外掛：Context7 文件檢索、AI 瀏覽器、RTK 命令輸出壓縮、用量統計、CDP/DAP 除錯工具

### ⚙️ 模型與供應商

- 支援 OpenAI、Anthropic、Gemini、Cohere 等協定族，內建 OpenAI、Anthropic、Google、DeepSeek、Kimi、MiniMax、阿里、OpenRouter 等供應商設定，也可以新增自訂供應商
- 對話、記憶檢索、程式碼索引、媒體生成可以各自選用不同的模型
- 擴充能力：網頁搜尋與擷取，圖片/影片/音訊理解，圖片與影片生成，語音合成與轉寫
- 上下文視窗分層策略，自動或手動（`/compact`）壓縮上下文
- Token 用量、延遲、參考費用統計

## 路線圖

目前已可使用的是 **GUI 桌面版**，接下來計畫：

- [ ] **TUI 版本**：在終端機中使用 Vulcan Code
- [ ] **遠端手機連線**：用手機遠端檢視與操控桌面上執行的對話
- [ ] **Linux / macOS 適配驗證**：建置腳本已具備，仍需在各平台完成完整驗證

如果你對其中某項有興趣，或有新的想法，歡迎在 Issue 中討論。

## 架構

```
┌──────────────────────────┐      ┌──────────────────────────────────────────┐
│  Electron desktop shell  │      │  Rust Core (vulcan-code-service)         │
│  window / tray / host    │◀────▶│  session kernel · agent runtime · tools  │
│  ┌────────────────────┐  │ RPC  │  provider protocols · MCP · LuaSkills    │
│  │ React WebUI        │  │events│  VCE index · knowledge base · MemoryMesh │
│  └────────────────────┘  │      │  terminal host · browser host · plugins  │
└──────────────────────────┘      └──────────────────────────────────────────┘
```

Core 服務可以獨立在背景（系統匣）執行，關閉桌面視窗不會中斷正在執行的任務。

| 目錄 | 說明 |
| --- | --- |
| `apps/vulcan-code-service` | Core 服務宿主，承載對話核心、Agent 執行階段與 WebUI RPC/串流服務 |
| `apps/vulcan-code-desktop` | Electron 桌面殼：視窗、系統匣，負責啟動本機 Core 並載入 WebUI |
| `apps/vulcan-code-web` | React/Vite WebUI：工作台、設定、對話、Diff、技能等介面 |
| `crates/vulcan-agent-runtime` | 共用代理執行階段：Provider、工具、LuaSkills、MCP、串流執行 |
| `crates/vulcan-service-*` | 後端服務 API、核心服務核心與執行階段適配 |
| `crates/vulcan-memorymesh-*` | MemoryMesh 長期記憶 |
| `crates/vulcan-runtime-assets` | 系統提示詞、供應商清單、子代理範本等權威資源 |
| `plugins/` | 內建外掛 |
| `docs/contracts/` | 開發契約文件 |

## 快速開始

> 目前已在 **Windows x86_64** 上完成完整驗證。Linux 與 macOS 有對應的 `make.sh` 腳本，但尚未完整跑通，歡迎協助驗證。

### 環境需求

- `make`
- [Rustup](https://rustup.rs/)：倉庫透過 `rust-toolchain.toml` 固定工具鏈版本，進入目錄後會自動安裝
- PowerShell 7
- Node.js 與 pnpm

### 建置與執行

```bash
make init-system   # 首次：安裝 Rust 工具、宿主相依與受管 Node/pnpm
make sync          # 同步設定、系統技能、外掛與瀏覽器執行環境到 output/
make build         # 建置 Core、WebUI 與 Electron 桌面殼
make run           # 啟動桌面端
```

也可以用 `make br` 一步完成建置與執行。發行建置使用 `make release`。

詳細的環境說明請見 [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md)。

### 日常開發

Vite 開發伺服器、Core 服務與 Electron 桌面殼彼此獨立，可以分別啟動與重新啟動：

```bash
make vite start      # 啟動 WebUI Vite 開發伺服器（也支援 status / stop）
make service         # 只啟動背景 Core，不開啟桌面視窗
make core-restart    # 只改了 Rust Core 時：增量編譯並熱替換 Core（Windows Debug）
make sync            # 修改資源、設定、技能或外掛後需要重新同步
```

- 只改 WebUI 頁面時，保持 Vite 與桌面殼執行即可熱更新
- debug 模式下 `make run` / `make br` 會自動確保 Vite 已啟動
- 不要直接編輯 `output/`，權威資源位於 `crates/vulcan-runtime-assets/resources/`

## 參與共建

我們歡迎任何形式的貢獻：回報 bug、完善文件、改進互動、適配新平台、接入新供應商、開發外掛與技能……

**提出 Issue**

- Bug：請附上作業系統、重現步驟、預期與實際行為，以及相關日誌或截圖
- 功能建議：說明使用情境與想解決的問題

**提交 PR**

1. Fork 倉庫並從 `main` 建立分支
2. 修改前先閱讀 [`AGENTS.md`](AGENTS.md)。它記錄了倉庫的長期約定，涉及特定領域時還需閱讀對應的 [`docs/contracts/`](docs/contracts/) 文件
3. 提交前在本機完成驗證：

   ```bash
   # Rust：優先對修改的 crate 做定向檢查
   cargo check -p <crate> --all-targets
   cargo nextest run -p <crate>

   # WebUI
   pnpm --dir apps/vulcan-code-web run lint
   pnpm --dir apps/vulcan-code-web run test
   pnpm --dir apps/vulcan-code-web run build
   ```

4. 在 PR 描述中說明改了什麼、為什麼改，以及如何驗證

幾條約定：

- 面向 AI 的工具名稱、描述、Schema 使用英文；面向使用者的介面文案可以使用中文
- 倉庫預設將編譯與測試並行數限制為 4，請不要在驗證命令中移除此限制
- 一個 PR 盡量只做一件事，方便審查

## 致謝

Vulcan Code 建立在眾多優秀的開源專案之上，感謝這些專案的作者與社群。

**內建元件與工具**

- [RTK](https://github.com/rtk-ai/rtk)：命令輸出壓縮，減少 Token 消耗（Vulcan RTK 外掛）
- [Context7](https://github.com/upstash/context7)：依版本檢索最新的函式庫文件（Context7 外掛）
- [Playwright](https://github.com/microsoft/playwright) / [Playwright MCP](https://github.com/microsoft/playwright-mcp)：AI 瀏覽器的驅動與工具
- [LuaSkills](https://github.com/LuaSkills/luaskills)：LuaSkills 技能執行環境
- [ripgrep](https://github.com/BurntSushi/ripgrep) 與 [ast-grep](https://github.com/ast-grep/ast-grep)：隨附的文字與語法樹搜尋工具
- [OpenAI Codex](https://github.com/openai/codex) Code Mode：程式碼模式的 JavaScript 沙盒（V8 執行環境、宿主程序、gRPC 協定與用戶端）直接移植自 Codex（Apache-2.0），本地改動標註在原始碼中，並記錄在各 crate 的 `NOTICE.md`（`crates/vulcan-code-mode*/`、`native/code-mode-host/*/`）
- [rusty_v8](https://github.com/denoland/rusty_v8)：程式碼模式沙盒使用的 V8 引擎 Rust 繫結

**桌面端與介面**

- [Electron](https://www.electronjs.org/)：桌面殼
- [React](https://react.dev/) 與 [Vite](https://vite.dev/)：WebUI 框架與建置工具
- [shadcn/ui](https://ui.shadcn.com/)、[Radix UI](https://www.radix-ui.com/)、[Tailwind CSS](https://tailwindcss.com/)：元件體系與樣式
- [xterm.js](https://xtermjs.org/)：終端機渲染
- [Shiki](https://shiki.style/)：程式碼與 Diff 語法高亮
- [Mermaid](https://mermaid.js.org/)：圖表渲染
- [Lucide](https://lucide.dev/)、[Iconify](https://iconify.design/)：圖示

**Rust Core**

- [Tokio](https://tokio.rs/) 與 [Axum](https://github.com/tokio-rs/axum)：非同步執行環境與服務框架
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk)：MCP 官方 Rust SDK
- [LanceDB](https://github.com/lancedb/lancedb) 與 [Apache Arrow](https://arrow.apache.org/)：向量索引與欄式資料
- [SQLite](https://sqlite.org/)（[rusqlite](https://github.com/rusqlite/rusqlite)）：本機資料儲存
- [jieba-rs](https://github.com/messense/jieba-rs)：中文分詞，用於知識庫全文檢索
- [portable-pty](https://github.com/wezterm/wezterm)（WezTerm）：跨平台虛擬終端機

**設計與實作參考**

- [OpenAI Codex](https://github.com/openai/codex)：Responses 協定通訊、重試與上下文壓縮的實作參考；Apply Patch 語法與串流解析器改編自 Codex
- [cc-switch](https://github.com/farion1231/cc-switch)：多協定轉換、串流工具呼叫與模型價格資料參考
- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)：Gemini 協定與相容轉換參考
- [OpenCode](https://github.com/anomalyco/opencode)：供應商目錄與系統提示詞參考
- [agency-agents](https://github.com/msitarzewski/agency-agents)：子代理角色範本來源
- [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html)：回答元件（發現、比較、流程等結構化區塊）的設計參考；本專案自行定義元件語法並在會話內原生渲染，不依賴其 CLI

**社群**

- [LINUX DO](https://linux.do/)：感謝 LINUX DO 社群的支持與交流

完整的第三方相依及授權請見 [`THIRD_PARTY_NOTICES.md`](crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md)，也可以在應用程式內的「設定 → 關於與許可證」中檢視。

## 授權

[MIT](LICENSE) © 2026 OpenVulcan
