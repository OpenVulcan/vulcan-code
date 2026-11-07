Vulcan Code 0.1.11 加入了反馈、消息中心和在本机修复问题的入口，并为 OpenAI、TokenRhythm、OpenCode Go 与 OpenDesign 更新了模型。

### 更新到此版本

```ui-checklist
ok | 增量更新 | 从 0.1.8、0.1.9 或 0.1.10 更新时只下载有变化的文件；更早的版本仍以完整更新的方式安装
ok | 应用数据库 | 首次启动时会先备份应用数据库，再自动升级到新结构，无需手动操作
```

### 反馈与消息

```ui-checklist
ok | 发送反馈 | 登录后可从「发送反馈」提交问题或建议；发送前会在本机过滤密钥、路径、用户名、网址和个人信息，并预览将要发送的确切内容
ok | 从错误卡片报告 | 对话出错时，错误卡片可以一键报告；只在可能由 Vulcan Code 引起的失败上提供，认证、余额、限流、服务故障、超时、内容策略和取消不会提供
ok | 消息中心 | 在「消息中心」查看公告、个人消息和维护者对反馈的回复，重要公告以横幅显示
ok | 在本机修复这个问题 | 选择一个文件夹后，会准备好 vulcan-code 源码作为项目，并打开一个带修复技能的对话；在该对话中确认拉取请求之前，不会推送或发布任何内容
ok | GitHub 入口 | 账户菜单新增「提交 GitHub 建议（Issue）」和「提交 GitHub PR」
ok | 终端客户端 | 终端客户端也提供反馈页面
```

### 模型与供应商

```ui-checklist
ok | OpenAI | 公开 API 的 8 个模型声明严格的 json_schema 结构化输出；GPT-5.6 Sol/Terra/Luna、GPT-6 Sol/Luna 与 GPT-6.1 Terra 可以关闭推理
ok | TokenRhythm | 新增 DeepSeek V4.1 Flash 1017、LongCat 2.0、MiniMax M3.1 Flash 以及测试模型 Egg-Alpha 和 Neohorse 1 9B，移除 minimax-m2.5 与 mimo-v2.5-pro；声明结构化输出，Messages 模型改为发送思考强度和关闭思考；DeepSeek V4 Pro 0813 与 GLM-5.2 改走 Responses，Kimi K2.6 改走 Chat
ok | OpenCode Go | 新增 Gemini 4 Argon、Hy4 以及免费的 Beam、Iceman 和 Crouton；10 个模型声明结构化输出
ok | OpenDesign | GPT-6 Luna 改走 Responses，Claude Haiku 5.5 改走 Messages；新增 deepseek-v4.1-flash-eco、hy4-preview、minimax-m3.1-flash 与 14 个 Plus/Pro/Max 对话模型；套餐包含时可用 GPT Image 2、Nano Banana 2/2 Lite、Seedream 5.0/5.0 Pro 生成图片；思考档位只保留各厂商公开的级别
ok | 记忆模型 | 可关闭思考并支持结构化输出的模型增多，更多模型可以选作记忆模型
```
