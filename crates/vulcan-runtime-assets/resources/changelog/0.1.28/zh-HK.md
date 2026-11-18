Vulcan Code 0.1.28 為外部 MCP 客戶端加入獨立工作區及呼叫記錄，並在啟動時更新低於最低版本要求的受管 ROOT 技能。

### 外部 MCP

```ui-checklist
ok | 獨立專案與會話 | 外部客戶端可登記工作目錄並建立自己的會話。檔案、CodeKit、Shell 及知識庫工具使用會話 ID 綁定的工作區。
ok | 背景指令與終端 | 取得相應權限後，客戶端可在 MCP 會話中執行背景指令、查詢或停止工作，亦可使用獨立終端會話。
ok | 選用的呼叫記錄 | 在客戶端設定開啟記錄工具呼叫後，可按客戶端、專案或會話篩選，查看參數、結果及狀態。
```

### 升級提示

```ui-checklist
ok | 獨立 MCP 工具改用會話 ID | 先用 `vc_mcp_project` 登記專案，再用 `vc_mcp_session` 建立會話。後續將返回的 `sid` 傳入獨立工具，工作目錄由會話決定。
```

### 技能與開發驗證

```ui-checklist
ok | 啟動時更新受管 ROOT 技能 | CodeKit、Vulcan Curl 及 Vulcan Work 低於要求版本時會從設定的來源更新。失敗時保留可用舊版，下次啟動重試。
ok | VMM 策略 HTTP 驗收 | 開發人員可執行 `make test-vmm-http`，驗證 WebUI 策略控制項和本機服務透過 HTTP 互動。
```
