import { expect, it, vi } from "vitest"
import { browserFallbackSnapshot } from "./browser-fallback-workbench"
import { executeProjectAttachmentCleanup, previewProjectAttachmentCleanup, runVmmMemoryAction, saveVmmMemoryConfig } from "./memory-actions-service"

// transport observes only the isolated service boundary; no real runtime is installed.
// transport 仅观察隔离服务边界，不安装实际运行时。
const transport = vi.hoisted(() => ({ available: false, request: vi.fn() }))
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => transport.available, serviceClient: { request: transport.request } }))

// Missing runtime must fail writes without changing preview state or fabricating a zero cleanup report.
// 缺少运行时必须使写入失败，不改变预览状态或伪造零清理报告。
it("rejects memory and cleanup mutations while preserving explicit read-only previews", async () => {
  // before is the exact browser snapshot whose immutability is asserted after failed writes.
  // before 是准确浏览器快照，用于断言失败写入后保持不变。
  const before = structuredClone(browserFallbackSnapshot())
  // cleanup is a fictional project command, never a filesystem path to operate on.
  // cleanup 是虚构项目命令，绝非要操作的文件系统路径。
  const cleanup = { projectId: "isolated-project", cutoffDate: "2026-09-01", selection: { image: true, audio: false, video: false, pdf: false } }
  await expect(runVmmMemoryAction({ action: "native_memory_write", scope: "global", projectId: null, commandId: "isolated-command", payload: {} })).rejects.toThrow("runtime is required")
  await expect(saveVmmMemoryConfig({ scope: "global", projectId: null, draft: { ...before.vmmMemory.globalDraft, enabled: !before.vmmMemory.globalDraft.enabled } })).rejects.toThrow("runtime is required")
  await expect(executeProjectAttachmentCleanup(cleanup)).rejects.toThrow("runtime is required")
  expect(browserFallbackSnapshot()).toEqual(before)
  expect(transport.request).not.toHaveBeenCalled()
  expect((await previewProjectAttachmentCleanup(cleanup)).summary).toContain("Preview mode")
})
