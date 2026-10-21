import { serviceClient } from "@/service/webui-service-runtime";
import type { McpServiceClientSaveRequest, McpServiceIssuedToken, McpServiceSnapshot } from "@/types";

// loadMcpService reads the MCP endpoint switch, its clients, and the grantable media models.
// loadMcpService 读取 MCP 端点开关、客户端与可授权的媒体模型。
export async function loadMcpService(): Promise<McpServiceSnapshot> {
  return serviceClient.request<McpServiceSnapshot>("service_mcp_service_snapshot", {});
}

// setMcpServiceEnabled turns the endpoint on or off without changing any client.
// setMcpServiceEnabled 开启或关闭端点，不改动任何客户端。
export async function setMcpServiceEnabled(enabled: boolean): Promise<McpServiceSnapshot> {
  return serviceClient.request<McpServiceSnapshot>("service_set_mcp_service_enabled", { enabled });
}

// createMcpClient adds one client and returns its token, which is shown only this once.
// createMcpClient 新增一个客户端并返回其令牌，令牌只展示这一次。
export async function createMcpClient(request: McpServiceClientSaveRequest): Promise<McpServiceIssuedToken> {
  return serviceClient.request<McpServiceIssuedToken>("service_create_mcp_client", { request });
}

// updateMcpClient replaces one client's name, switch, and grants; the token is unchanged.
// updateMcpClient 替换一个客户端的名称、开关与授权；令牌保持不变。
export async function updateMcpClient(request: McpServiceClientSaveRequest): Promise<McpServiceSnapshot> {
  return serviceClient.request<McpServiceSnapshot>("service_update_mcp_client", { request });
}

// rotateMcpClientToken issues a new token; the old one stops working immediately.
// rotateMcpClientToken 签发新令牌；旧令牌立即失效。
export async function rotateMcpClientToken(clientId: string): Promise<McpServiceIssuedToken> {
  return serviceClient.request<McpServiceIssuedToken>("service_rotate_mcp_client_token", { clientId });
}

// deleteMcpClient removes one client; its token stops working.
// deleteMcpClient 删除一个客户端；其令牌失效。
export async function deleteMcpClient(clientId: string): Promise<McpServiceSnapshot> {
  return serviceClient.request<McpServiceSnapshot>("service_delete_mcp_client", { clientId });
}
