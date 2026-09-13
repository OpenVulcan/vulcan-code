import { describe, expect, it, vi } from "vitest"
import { renameProject, renameConversation, setProjectIcon, resetProjectIcon, toggleProjectPinned, archiveProjectConversations, removeProject, archiveConversation, reorderProjects, setExpandedTaskProjects } from "./project-actions-service"

// request is a readout spy; preview mode never has an authorized persistence transport.
// request 是调用观察器；预览模式没有可执行持久化的传输。
const request = vi.hoisted(() => vi.fn())
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => false, serviceClient: { request } }))

describe("project editor preview failures", () => {
  it("rejects rename and icon writes without a service instead of reporting success", async () => {
    await expect(renameProject("p", "name")).rejects.toThrow("not renamed")
    await expect(renameConversation("c", "name")).rejects.toThrow("not renamed")
    await expect(setProjectIcon("p", { pack: "lucide", key: "Folder" })).rejects.toThrow("not saved")
    await expect(resetProjectIcon("p")).rejects.toThrow("not reset")
    await expect(toggleProjectPinned("p")).rejects.toThrow("not changed")
    await expect(archiveProjectConversations("p")).rejects.toThrow("not archived")
    await expect(removeProject("p")).rejects.toThrow("not removed")
    await expect(archiveConversation("c")).rejects.toThrow("not archived")
    await expect(reorderProjects(["p"])).rejects.toThrow("not saved")
    await expect(setExpandedTaskProjects(["p"])).rejects.toThrow("not saved")
    expect(request).not.toHaveBeenCalled()
  })
})
