Vulcan Code 0.1.29 为外部 MCP 的长时间工具调用加入心跳，并更新了 OpenAI 模型目录。

### 外部 MCP

```ui-checklist
ok | 长调用自动发送心跳 | 对支持 SSE 的客户端，长时间工具调用会定期发送心跳，减少因连接空闲而中断。工具完成后，同一请求会收到最终结果。
ok | 长命令可在后台继续 | 心跳维持连接，客户端自己的等待上限依然生效。前台 Shell 断开时会清理进程树。预计运行较久时，在首次调用传入 `background: true`，再用同一 `sid` 的 `shell_job` 查询结果。后台执行需要 `shell_job` 授权。
ok | 更容易排查令牌问题 | 服务日志区分缺失、格式错误的认证头和无效令牌，并记录手动轮换事件。日志只保留诊断类别或客户端 ID。
```

### 模型

```ui-checklist
ok | GPT-6.2 Sol 与 Astra | OpenAI API 和 Codex 订阅都可选择这两款模型。Codex 订阅按模型目录上报的上下文窗口运行。
ok | GPT Image 2.6 | OpenAI 媒体选择器新增 Flare 和 Sunburst 两个版本。
```
