Vulcan Code 0.1.29 為外部 MCP 的長時間工具呼叫加入心跳，並更新 OpenAI 模型目錄。

### 外部 MCP

```ui-checklist
ok | 長時間呼叫會自動傳送心跳 | 對支援 SSE 的用戶端，執行時間較長的工具呼叫會定期傳送心跳，減少連線因閒置而中斷。工具完成後，同一個請求會收到最終結果。
ok | 長時間命令可在背景繼續執行 | 心跳可維持連線，用戶端的等待時間上限依然適用。前景 Shell 斷線時會清理程序樹。預計執行較久時，請在首次呼叫傳入 `background: true`，再以相同 `sid` 呼叫 `shell_job` 查詢結果。背景執行需要 `shell_job` 授權。
ok | 更容易排查權杖問題 | 服務日誌會區分缺少或格式錯誤的驗證標頭與無效權杖，也會記錄手動輪替事件。日誌只保留診斷類別或用戶端 ID。
```

### 模型

```ui-checklist
ok | GPT-6.2 Sol 與 Astra | OpenAI API 與 Codex 訂閱都可選用這兩款模型。Codex 訂閱使用模型目錄回報的內容視窗。
ok | GPT Image 2.6 | OpenAI 媒體選擇器新增 Flare 與 Sunburst 兩款模型。
```
