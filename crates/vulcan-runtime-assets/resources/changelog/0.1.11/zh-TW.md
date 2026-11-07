Vulcan Code 0.1.11 加入了意見回饋、訊息中心與在本機修復問題的入口，並為 OpenAI、TokenRhythm、OpenCode Go 與 OpenDesign 更新了模型。

### 更新到此版本

```ui-checklist
ok | 增量更新 | 從 0.1.8、0.1.9 或 0.1.10 更新時只下載有變化的檔案；更早的版本仍以完整更新的方式安裝
ok | 應用程式資料庫 | 首次啟動時會先備份應用程式資料庫，再自動升級到新結構，無需手動操作
```

### 意見回饋與訊息

```ui-checklist
ok | 傳送意見回饋 | 登入後可從「傳送意見回饋」提交問題或建議；傳送前會在本機過濾金鑰、路徑、使用者名稱、網址與個人資訊，並預覽將要傳送的確切內容
ok | 從錯誤卡片回報 | 對話出錯時，錯誤卡片可以一鍵回報；只在可能由 Vulcan Code 引起的失敗上提供，驗證、餘額、限流、服務故障、逾時、內容政策與取消不會提供
ok | 訊息中心 | 在「訊息中心」查看公告、個人訊息與維護者對意見回饋的回覆，重要公告以橫幅顯示
ok | 在本機修復這個問題 | 選擇一個資料夾後，會準備好 vulcan-code 原始碼作為專案，並開啟一個帶修復技能的工作階段；在該工作階段中確認提取要求之前，不會推送或發布任何內容
ok | GitHub 入口 | 帳戶選單新增「提交 GitHub 建議（Issue）」與「提交 GitHub PR」
ok | 終端客戶端 | 終端客戶端也提供意見回饋頁面
```

### 模型與供應商

```ui-checklist
ok | OpenAI | 公開 API 的 8 個模型宣告嚴格的 json_schema 結構化輸出；GPT-5.6 Sol/Terra/Luna、GPT-6 Sol/Luna 與 GPT-6.1 Terra 可以關閉推理
ok | TokenRhythm | 新增 DeepSeek V4.1 Flash 1017、LongCat 2.0、MiniMax M3.1 Flash 以及測試模型 Egg-Alpha 與 Neohorse 1 9B，移除 minimax-m2.5 與 mimo-v2.5-pro；宣告結構化輸出，Messages 模型改為傳送思考強度與關閉思考；DeepSeek V4 Pro 0813 與 GLM-5.2 改走 Responses，Kimi K2.6 改走 Chat
ok | OpenCode Go | 新增 Gemini 4 Argon、Hy4 以及免費的 Beam、Iceman 與 Crouton；10 個模型宣告結構化輸出
ok | OpenDesign | GPT-6 Luna 改走 Responses，Claude Haiku 5.5 改走 Messages；新增 deepseek-v4.1-flash-eco、hy4-preview、minimax-m3.1-flash 與 14 個 Plus/Pro/Max 對話模型；方案包含時可用 GPT Image 2、Nano Banana 2/2 Lite、Seedream 5.0/5.0 Pro 生成圖片；思考等級只保留各廠商公開的級別
ok | 記憶模型 | 可關閉思考並支援結構化輸出的模型增多，更多模型可選作記憶模型
```
