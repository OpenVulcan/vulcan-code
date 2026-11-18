Vulcan Code 0.1.28 為外部 MCP 用戶端加入獨立工作區及呼叫記錄，並在啟動時更新低於最低版本要求的受管 ROOT 技能。

### 外部 MCP

```ui-checklist
ok | 獨立專案與對話 | 外部用戶端可登記工作目錄並建立自己的對話。檔案、CodeKit、Shell 與知識庫工具使用對話 ID 所綁定的工作區。
ok | 背景命令與終端機 | 取得對應權限後，用戶端可在 MCP 對話中執行背景命令、查詢或停止工作，也可使用獨立的終端機工作階段。
ok | 選用的呼叫記錄 | 在用戶端設定中開啟記錄工具呼叫後，可依用戶端、專案或對話篩選，檢視參數、結果及狀態。
```

### 升級提示

```ui-checklist
ok | 獨立 MCP 工具改用對話 ID | 先用 `vc_mcp_project` 登記專案，再用 `vc_mcp_session` 建立對話。後續將回傳的 `sid` 傳入獨立工具，工作目錄由該對話決定。
```

### 技能與開發驗證

```ui-checklist
ok | 啟動時更新受管 ROOT 技能 | CodeKit、Vulcan Curl 與 Vulcan Work 低於要求版本時，會從設定的來源更新。失敗時保留可用舊版，下次啟動重試。
ok | VMM 策略 HTTP 驗收 | 開發人員可執行 `make test-vmm-http`，驗證 WebUI 策略控制項與本機服務透過 HTTP 互動。
```
