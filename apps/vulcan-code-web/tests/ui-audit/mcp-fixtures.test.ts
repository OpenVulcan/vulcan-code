import { expect, it } from "vitest";
import { auditMcpCapabilities, auditMcpInspections } from "./mcp-fixtures";
import { serviceClient } from "@/service/webui-service-runtime";

// Verifies schema-rich fixtures preserve exact metadata and reject unavailable server details.
// 验证完整 Schema 夹具保留准确元数据，并拒绝不可用服务器详情。
it("keeps connected metadata distinct from failed, connecting, disabled and empty states", async () => {
  const servers = auditMcpInspections("long-list");
  expect(servers.map(server => server.state.status)).toEqual(["connected", "failed", "connecting", "disabled"]);
  expect(servers[0].tools[0].inputSchema).toHaveProperty("x-audit-extension", { preserved: true });
  expect(servers[0].tools[0].outputSchema).toHaveProperty("oneOf");
  expect(auditMcpCapabilities(servers[0].state.id, "long-list").prompts?.[0].arguments?.[0].required).toBe(true);
  expect(auditMcpCapabilities(servers[0].state.id, "long-list").resources?.[0].uri).toBe("audit://handbook");
  for (const server of servers.slice(1)) expect(() => auditMcpCapabilities(server.state.id, "long-list")).toThrow("NOT_CONNECTED");
  expect(auditMcpInspections("empty")).toEqual([]);
  // The transport denies tool execution and persistence even while fixtures present connected metadata.
  // 即使夹具展示已连接元数据，传输仍拒绝工具执行与持久化。
  await expect(serviceClient.request("service_execute_mcp_tool", {})).rejects.toThrow("UNIMPLEMENTED_RPC");
  await expect(serviceClient.request("service_save_mcp_server", {})).rejects.toThrow("UNIMPLEMENTED_RPC");
});
