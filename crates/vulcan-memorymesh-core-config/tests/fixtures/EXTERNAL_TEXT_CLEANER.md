# 外置文本清洗对照样本

- 来源仓库：`D:/openvulcan/VulcanMemoryMesh`。
- 来源提交：`82f05fa5e3c7657e770fefd84e220fba5654ce07`。
- 来源文件：`internal/platform/textutil/{memory_cleaner,postaction_sanitizer,token_budget,text}.go`。
- 生成方式：将这四个标准库依赖文件复制到隔离临时 Go 模块，仅将包名改为 `main`；测试入口逐条调用原函数。未修改来源算法或外置仓库。
- 内容：38 个合成场景，不包含用户会话、凭据或真实个人资料。每项保存原文、机器折叠、媒体清理、完整清洗、原文 Token 估算及默认预算裁剪的真实外置输出。
- 扩展覆盖：相对路径、编码扩展名、无效转义、查询及片段、端口及 IPv6、媒体标签属性、全部资源扩展名、非 ASCII 边界和 Unicode 字符分类。媒体阶段不会打开或下载任何链接。
- 用途：原生实现按阶段逐字节比较；不能以其中一个阶段通过宣称完整清洗或宿主接入已完成。
- 临时生成目录：`F:/packages/temp/vmm-parity-oracle-e1b88289d98a4e40a418f35df87652d7`，保留用于本轮后续校准；不是应用运行依赖，不随应用分发。
