import type { SubagentDefinitionSaveRequest } from "@/types"

// AgentActionProps groups service-backed actions used by the independent Agents page.
// AgentActionProps 归组独立智能体页面使用的服务端动作。
export interface AgentActionProps {
  // Refreshes editable agent and expert definitions.
  // 刷新可编辑智能体与专家定义。
  onRefreshAgentCatalog: () => Promise<void>
  // Opens the user-owned agent definition directory.
  // 打开用户拥有的智能体定义目录。
  onOpenAgentUserDirectory: () => Promise<void>
  // Toggles one agent definition enabled state.
  // 切换单个智能体定义启停状态。
  onSetAgentDefinitionEnabled: (agentId: string, enabled: boolean) => Promise<void>
  // Saves one editable agent definition into the user layer.
  // 将单个可编辑智能体定义保存到用户层。
  onSaveAgentDefinition: (definition: SubagentDefinitionSaveRequest) => Promise<void>
  // Restores one system-backed agent definition default.
  // 恢复单个系统集成智能体定义默认值。
  onRestoreAgentDefinitionDefault: (agentId: string) => Promise<void>
}
