# 06：旧测试修复记录

当前状态：下方前两项保留第四阶段证据；最终共修复3项有意义旧测试，23项运行时用例全部保留。当前定位见[最终完整映射](06-FINAL-MOVES.md)。

## 已核实问题

当前唯一授权记录路径由authorization::plugin_authorization_state_path构造：state_root/plugin_id/authorization.json；安装通过该路径写入授权，卸载的state_paths通过同一助手选择清理目标。

原两项测试却断言plugin_state/local-plugin.auth.json不存在。该文件并非当前生产写入路径，断言不能证明拒绝安装不残留授权或卸载删除真实授权。原测试仍有意义，按用户关于旧测试的授权修复，不删除测试或回退生产路径。

| 原测试 | 原始行 | 明确修复 | 为什么不是弱化 |
| --- | ---: | --- | --- |
| user_plugin_install_binds_authorization_to_exact_content_fingerprint | 8902 | 将旧路径字面量改为plugin_state/local-plugin/authorization.json | 对真实生产文件断言不应存在，原错误/包不存在断言保持 |
| user_plugin_install_environment_and_uninstall_are_transactional | 9056 | 同样修正删除后路径，并在成功安装后新增真实授权文件is_file断言 | 先证明授权确实创建，再证明卸载移除，原包/私有环境断言保持 |

## 校验方式

- 固定原始218节点基线不改写；生产186节点必须继续严格等价。
- 仅允许上述两个明确测试身份中的旧字面量各替换一次；仅卸载用例允许在原安装身份断言后插入一次授权存在性断言。
- 其余原测试体、全部属性、Windows条件与9辅助节点严格保持；发现/执行数量仍23运行时、59包级。
- 自动核验同时报告未经修复的原词法对比和应用这份实名差异后的完整对比，不能把这两个有意修复写成218节点完全未变。

## 执行状态

第四阶段已实施并闭环。4项安装回归通过（0.21秒），最终23运行时通过（4.06秒）、59包级通过（4.46秒），均0失败/0忽略；14项服务层回归也通过（0.02秒）。216原节点严格词法等价，2测试严格匹配上述实名修复，186生产节点没有业务变化。当前全部runtime产物不再包含旧local-plugin.auth.json路径，原测试属性与Windows条件保持。

旧测试问题已解决；这不代表06其余桥接/租约职责已完成。新旧完整路径与行号见06-TEST-MOVES.md。

## 第十三阶段：Context7空通过修复

`bundled_context7_plugin_loads_with_typed_json_surface`原始8613行，当前位于`runtime/tests/bundled.rs:480`。真实Context7清单声明Lua运行时而不声明Hook；旧`prewarm_plugin_runtime`只为带Hook的已启用包预热，所以Ok(false)也会通过，未验证测试名称承诺的加载。

改为从真实目录读取明确停用的Context7，先断言清单存在Lua运行时，再`begin_enabling`并调用`probe_plugin`。实际生产链创建、加载、探测并关闭租约；不会触发在线查询或启用回调。没有修改插件清单、Lua包或生产函数，没有删除或弱化原有效断言。

定向用例通过（0.15秒），最终59包级测试再次通过（4.77秒）。包含前两项授权测试修复在内，三项差异各自精确记录于[词法差异数据](06-LEXICAL-EXCEPTIONS.json)，原始基线保持不变。06整体终审见[最终审核](06-FINAL-REVIEW.md)。
