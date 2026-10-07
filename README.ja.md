<div align="center">

<img src="apps/vulcan-code-web/public/vulcan-logo.png" alt="Vulcan Code" width="120" />

# Vulcan Code

**オープンソースの GUI 型 AI Coding デスクトップワークベンチ**

Rust Core · Electron デスクトップシェル · React WebUI

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Issues](https://img.shields.io/github/issues/OpenVulcan/vulcan-code)](https://github.com/OpenVulcan/vulcan-code/issues)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/OpenVulcan/vulcan-code/pulls)

[简体中文](README.md) | [English](README.en.md) | [繁體中文](README.zh-TW.md) | **日本語** | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md) | [Português](README.pt-BR.md)

</div>

> 本書は[簡体字中国語版 README](README.md) の翻訳です。内容に相違がある場合は中国語版が優先されます。翻訳の修正は PR で歓迎します。

---

## プロジェクトについて

Vulcan Code は **GUI で使う AI Coding ソフトウェア**です。プロジェクト、セッション、ターミナル、ブラウザ、コードレビュー、Git コミットをひとつのデスクトップワークベンチにまとめ、AI がコードを理解し、タスクを進め、確認可能な成果を届けられるようにします。

本プロジェクトは長期間にわたって社内で開発・デバッグを重ね、このたび **MIT ライセンス**でオープンソース化しました。ひとつのチームの製品にとどめず、コミュニティと一緒に磨き上げていくツールにしたいと考えています。

- 🐛 問題を見つけた、アイデアがある — [Issue](https://github.com/OpenVulcan/vulcan-code/issues) をお寄せください
- 🔧 バグを直した、機能を作った — [Pull Request](https://github.com/OpenVulcan/vulcan-code/pulls) をお送りください
- 🤝 ユーザーでも開発者でも、誰もが参加できます。みんなに愛される AI Coding ソフトウェアを一緒に作りましょう

## 主な機能

### 🧭 デスクトップワークベンチ

- 複数プロジェクト・複数セッションを管理し、ファイルツリー、レビュー、メモリ、サブエージェントなどのパネルを自由に組み合わせ可能
- タブ対応のプロジェクトターミナルを内蔵。AI は長時間実行や対話型のコマンドセッションを保持できます
- ストリーミング応答。Markdown、コードブロック、Mermaid/SVG 図、ツール呼び出しカードをその場で描画
- 過去の質問を編集して再生成したり、完了済みの任意のターンから新しいセッションを分岐したりできます
- 中国語・英語 UI、ライト/ダークテーマ

### 🎯 タスクを前に進める

- **Goal モード**：目標とトークン予算を設定すると、AI がターンごとに作業を継続。一時停止・再開・クリアが可能
- **Plan モード**：まず調査して計画を作成し、確認後に実行
- **実行中の指示**：タスク実行中に追加の指示を送ったり、後続のリクエストをキューに入れたりできます
- **サブエージェント**：Explore、Plan などの組み込みロールに加え、[agency-agents](https://github.com/msitarzewski/agency-agents) の 260 以上のロールテンプレートを同梱（既定では無効、必要に応じて有効化）
- **構造化された質問**：AI があなたの判断を必要とするとき、選択肢カードで質問します

### 🧠 コード理解と経験の蓄積

- **VCE セマンティックコードインデックス**：プロジェクトのベクトルインデックスを構築し、AI が関連する実装を見つけてから修正に取りかかります。インデックスはコードの変更に合わせて自動同期
- **プロジェクトナレッジベース**：Markdown フォルダを取り込み、BM25 とベクトルのハイブリッド検索に対応。プロジェクト単位で許可し、セッション単位で選択可能
- **MemoryMesh 長期メモリ**：ターン終了後に経験を抽出し、次のターン開始前に自動で想起。メモリは閲覧・修正・削除でき、ゴミ箱と出典の追跡も備えています

### 🔍 確認できる成果

- **Diff レビュー**：統合/分割ビュー、単語単位の差分、全文検索、ステージと取り消しに対応
- **Git コミット**：AI がコミットメッセージを生成し、現在のブランチまたは新しいブランチにコミットしてそのままプッシュ
- **AI ブラウザ**：AI が実際の Chromium ページでナビゲート、クリック、スクリーンショット、コンソール読み取りを実行。いつでも操作を引き継げます

### 🧩 拡張性

- **プロンプトスキル**（`SKILL.md`）：チームの仕事の進め方を蓄積し、次回そのまま再利用
- **LuaSkills**：よく使う操作を、設定・インストール可能なプログラム型の機能パッケージにまとめられます
- **MCP**：ローカルプロセスと HTTP の両方の MCP サーバーに対応し、接続診断とツールスキーマの確認が可能
- **プラグインシステム**：プラグインはツールを提供でき、独自の UI も組み込めます。組み込みプラグイン：Context7 ドキュメント検索、AI ブラウザ、RTK コマンド出力圧縮、使用量統計、CDP/DAP デバッグツール

### ⚙️ モデルとプロバイダー

- OpenAI、Anthropic、Gemini、Cohere などのプロトコルファミリーに対応。OpenAI、Anthropic、Google、DeepSeek、Kimi、MiniMax、Alibaba、OpenRouter などのプロバイダー設定を内蔵し、カスタムプロバイダーも追加できます
- 会話、メモリ検索、コードインデックス、メディア生成でそれぞれ別のモデルを選択可能
- 拡張機能：Web 検索と取得、画像/動画/音声の理解、画像・動画生成、音声合成と文字起こし
- 階層化されたコンテキストウィンドウポリシーと、自動または手動（`/compact`）のコンテキスト圧縮
- トークン使用量、レイテンシ、参考コストの統計

## ロードマップ

現在利用できるのは **GUI デスクトップ版**です。今後の予定：

- [ ] **TUI 版**：ターミナルで Vulcan Code を使う
- [ ] **スマートフォンからのリモート接続**：デスクトップで動いているセッションをスマホから確認・操作
- [ ] **Linux / macOS 対応の検証**：ビルドスクリプトは用意済みで、各プラットフォームでの完全な検証が必要です

興味のある項目や新しいアイデアがあれば、ぜひ Issue で議論しましょう。

## アーキテクチャ

```
┌──────────────────────────┐      ┌──────────────────────────────────────────┐
│  Electron desktop shell  │      │  Rust Core (vulcan-code-service)         │
│  window / tray / host    │◀────▶│  session kernel · agent runtime · tools  │
│  ┌────────────────────┐  │ RPC  │  provider protocols · MCP · LuaSkills    │
│  │ React WebUI        │  │events│  VCE index · knowledge base · MemoryMesh │
│  └────────────────────┘  │      │  terminal host · browser host · plugins  │
└──────────────────────────┘      └──────────────────────────────────────────┘
```

Core サービスはバックグラウンド（システムトレイ）で単独動作できるため、デスクトップウィンドウを閉じても実行中のタスクは中断されません。

| ディレクトリ | 説明 |
| --- | --- |
| `apps/vulcan-code-service` | Core サービスホスト。セッションカーネル、エージェントランタイム、WebUI RPC/ストリーミングサービスを担当 |
| `apps/vulcan-code-desktop` | Electron デスクトップシェル。ウィンドウとトレイを提供し、ローカル Core を起動して WebUI を読み込み |
| `apps/vulcan-code-web` | React/Vite WebUI。ワークベンチ、設定、セッション、Diff、スキルなどの画面 |
| `crates/vulcan-agent-runtime` | 共有エージェントランタイム。プロバイダー、ツール、LuaSkills、MCP、ストリーミング実行 |
| `crates/vulcan-service-*` | バックエンドサービス API、コアサービスカーネル、ランタイムアダプター |
| `crates/vulcan-memorymesh-*` | MemoryMesh 長期メモリ |
| `crates/vulcan-runtime-assets` | システムプロンプト、プロバイダーマニフェスト、サブエージェントテンプレートなどの正本リソース |
| `plugins/` | 組み込みプラグイン |
| `docs/contracts/` | 開発規約ドキュメント |

## はじめに

> 現在、完全な検証が済んでいるのは **Windows x86_64** です。Linux と macOS には対応する `make.sh` スクリプトがありますが、まだ一通りの動作確認ができていません。検証へのご協力を歓迎します。

### 必要な環境

- `make`
- [Rustup](https://rustup.rs/)：ツールチェーンのバージョンは `rust-toolchain.toml` で固定されており、自動でインストールされます
- PowerShell 7
- Node.js と pnpm

### ビルドと実行

```bash
make init-system   # 初回：Rust ツール、ホスト依存、管理対象の Node/pnpm をインストール
make sync          # 設定、システムスキル、プラグイン、ブラウザランタイムを output/ に同期
make build         # Core、WebUI、Electron デスクトップシェルをビルド
make run           # デスクトップアプリを起動
```

`make br` でビルドと実行を一度に行うこともできます。リリースビルドには `make release` を使います。

環境の詳細は [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md) を参照してください。

### 日々の開発

Vite 開発サーバー、Core サービス、Electron デスクトップシェルは互いに独立しており、個別に起動・再起動できます。

```bash
make vite start      # WebUI の Vite 開発サーバーを起動（status / stop にも対応）
make service         # デスクトップウィンドウを開かずにバックグラウンドの Core のみ起動
make core-restart    # Rust Core のみ変更した場合：増分ビルドして Core をホットスワップ（Windows Debug）
make sync            # リソース、設定、スキル、プラグインを変更したら再同期
```

- WebUI のみを変更する場合は、Vite とデスクトップシェルを起動したままにすればホットリロードされます
- debug モードでは `make run` / `make br` が Vite の起動を自動で確認します
- `output/` を直接編集しないでください。正本のリソースは `crates/vulcan-runtime-assets/resources/` にあります

## コントリビュート

バグ報告、ドキュメント改善、UX 改善、新しいプラットフォームやプロバイダーへの対応、プラグインやスキルの開発など、あらゆる形の貢献を歓迎します。

**Issue を作成する**

- バグ：OS、再現手順、期待する動作と実際の動作、関連するログやスクリーンショットを添えてください
- 機能提案：利用シーンと解決したい課題を説明してください

**PR を送る**

1. リポジトリを Fork し、`main` からブランチを作成
2. 変更前に [`AGENTS.md`](AGENTS.md) を読んでください。リポジトリの長期的な規約が記載されています。特定の領域に関わる場合は、対応する [`docs/contracts/`](docs/contracts/) のドキュメントも読んでください
3. 提出前にローカルで検証：

   ```bash
   # Rust：変更した crate に絞ってチェック
   cargo check -p <crate> --all-targets
   cargo nextest run -p <crate>

   # WebUI
   pnpm --dir apps/vulcan-code-web run lint
   pnpm --dir apps/vulcan-code-web run test
   pnpm --dir apps/vulcan-code-web run build
   ```

4. PR の説明に、何を・なぜ変更したか、どう検証したかを書いてください

いくつかの規約：

- AI 向けのツール名、説明、スキーマは英語で記述します。ユーザー向けの UI 文言はローカライズされます
- ビルドとテストの並列数は既定で 4 に制限されています。検証コマンドでこの制限を外さないでください
- レビューしやすいよう、1 つの PR では 1 つのことだけを扱ってください

## 謝辞

Vulcan Code は数多くの優れたオープンソースプロジェクトの上に成り立っています。各プロジェクトの作者とコミュニティに感謝します。

**同梱コンポーネントとツール**

- [RTK](https://github.com/rtk-ai/rtk)：コマンド出力を圧縮してトークン消費を削減（Vulcan RTK プラグイン）
- [Context7](https://github.com/upstash/context7)：バージョンに応じた最新のライブラリドキュメント検索（Context7 プラグイン）
- [Playwright](https://github.com/microsoft/playwright) / [Playwright MCP](https://github.com/microsoft/playwright-mcp)：AI ブラウザのドライバーとツール
- [LuaSkills](https://github.com/LuaSkills/luaskills)：LuaSkills ランタイム
- [ripgrep](https://github.com/BurntSushi/ripgrep) と [ast-grep](https://github.com/ast-grep/ast-grep)：同梱のテキスト検索・構文木検索ツール

**デスクトップと UI**

- [Electron](https://www.electronjs.org/)：デスクトップシェル
- [React](https://react.dev/) と [Vite](https://vite.dev/)：WebUI フレームワークとビルドツール
- [shadcn/ui](https://ui.shadcn.com/)、[Radix UI](https://www.radix-ui.com/)、[Tailwind CSS](https://tailwindcss.com/)：コンポーネントとスタイル
- [xterm.js](https://xtermjs.org/)：ターミナル描画
- [Shiki](https://shiki.style/)：コードと Diff のシンタックスハイライト
- [Mermaid](https://mermaid.js.org/)：図の描画
- [Lucide](https://lucide.dev/)、[Iconify](https://iconify.design/)：アイコン

**Rust Core**

- [Tokio](https://tokio.rs/) と [Axum](https://github.com/tokio-rs/axum)：非同期ランタイムとサービスフレームワーク
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk)：MCP 公式 Rust SDK
- [LanceDB](https://github.com/lancedb/lancedb) と [Apache Arrow](https://arrow.apache.org/)：ベクトルインデックスと列指向データ
- [SQLite](https://sqlite.org/)（[rusqlite](https://github.com/rusqlite/rusqlite)）：ローカルデータストレージ
- [jieba-rs](https://github.com/messense/jieba-rs)：ナレッジベース全文検索のための中国語分かち書き
- [portable-pty](https://github.com/wezterm/wezterm)（WezTerm）：クロスプラットフォームの疑似端末

**設計と実装の参考**

- [OpenAI Codex](https://github.com/openai/codex)：Responses プロトコル通信、リトライ、コンテキスト圧縮の実装参考
- [cc-switch](https://github.com/farion1231/cc-switch)：マルチプロトコル変換、ストリーミングツール呼び出し、モデル価格データの参考
- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)：Gemini プロトコルと互換変換の参考
- [OpenCode](https://github.com/anomalyco/opencode)：プロバイダーカタログとシステムプロンプトの参考
- [agency-agents](https://github.com/msitarzewski/agency-agents)：サブエージェントのロールテンプレートの出典
- [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html)：必要に応じた HTML 回答の可視化と対話的表示の設計参考。プロトコルと隔離ビューアーは独自実装で、同プロジェクトの CLI には依存しません

**コミュニティ**

- [LINUX DO](https://linux.do/)：LINUX DO コミュニティの支援と交流に感謝します

サードパーティ依存関係とライセンスの完全な一覧は [`THIRD_PARTY_NOTICES.md`](crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md) にあります。アプリ内の **Settings → About and licenses** でも確認できます。

## ライセンス

[MIT](LICENSE) © 2026 OpenVulcan
