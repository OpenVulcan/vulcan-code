import { beforeEach, describe, expect, it, vi } from "vitest";
import type { VceJobSummary } from "@/types";
import { executeVceMaintenance, listVceFiles, listVceIntegrityIssues, loadVceBlobDetail, planVceMaintenance } from "./vce-service";

// Replace transport only; the maintenance adapter must preserve exact human confirmation authority.
// 仅替换传输；维护适配器必须保留精确的人为确认权威。
const transport = vi.hoisted(() => ({ available: vi.fn(), request: vi.fn() }));
vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: transport.available,
  serviceClient: { request: transport.request },
}));

// Reset transport state before each isolated adapter test; no host or model is started.
// 每项隔离适配器测试前重置传输状态；不启动宿主或模型。
beforeEach(() => { vi.resetAllMocks(); transport.available.mockReturnValue(true); });

describe("confirmed VCE maintenance admission", () => {
  // Preserve the overview's opaque identity for every content management RPC.
  // 为每个内容管理调用保留概览的不透明身份。
  it("forwards the exact publication identity to files, integrity and details", async () => {
    transport.request.mockResolvedValue({ items: [] });
    await listVceFiles("project-a", "physical-space:7", "100", 100);
    expect(transport.request).toHaveBeenLastCalledWith("service_list_vce_files", {
      request: { projectId: "project-a", expectedReadIdentity: "physical-space:7", pathPrefix: null,
        status: null, auxiliaryFailuresOnly: false, page: { cursor: "100", limit: 100 } },
    });
    await listVceIntegrityIssues("project-a", "physical-space:7", null, 100);
    expect(transport.request).toHaveBeenLastCalledWith("service_list_vce_integrity_issues", {
      request: { projectId: "project-a", expectedReadIdentity: "physical-space:7", page: { cursor: null, limit: 100 } },
    });
    await loadVceBlobDetail("project-a", "blob-a", "physical-space:7");
    expect(transport.request).toHaveBeenLastCalledWith("service_get_vce_blob_detail", {
      request: { projectId: "project-a", blobName: "blob-a", expectedReadIdentity: "physical-space:7" },
    });
  });

  // Verify a queued rebuild returns immediately as a job, preserving all plan and replay fields.
  // 验证排队重建立即以作业返回，保留全部计划及重放字段。
  it("returns the durable receipt rather than treating admission as a completed overview", async () => {
    // job is a complete queued receipt with an existing query checkpoint and no fabricated progress.
    // job 为完整排队回执，具有已有查询检查点且无伪造进度。
    const job: VceJobSummary = {
      projectId: "project-a", jobId: "rebuild-a", kind: "rebuild", state: "queued", sequence: 1,
      createdAtUnixMs: 1000, updatedAtUnixMs: 1000, sourceCheckpointVersion: 7,
      resultCheckpointVersion: null, progress: null, followupRequested: false, error: null,
    };
    transport.request.mockResolvedValue({ kind: "job", job });
    expect(await executeVceMaintenance("project-a", "rebuild", 4, 7, ["confirmed-action"], "click-a", "sha256:preview-target"))
      .toEqual({ kind: "job", job });
    expect(transport.request).toHaveBeenCalledExactlyOnceWith("service_execute_vce_maintenance", {
      request: { projectId: "project-a", mode: "rebuild", expectedSettingsRevision: 4,
        expectedCheckpointVersion: 7, actionIds: ["confirmed-action"], idempotencyKey: "click-a", expectedEmbeddingFingerprint: "sha256:preview-target" },
    });
  });

  // An uninitialized rebuild carries no checkpoint and must preserve that explicit null in the RPC.
  // 未初始化重建没有检查点，必须在 RPC 中保留这个明确的空值。
  it("admits a rebuild without a published checkpoint", async () => {
    const job: VceJobSummary = {
      projectId: "project-a", jobId: "rebuild-uninitialized", kind: "initialize", state: "queued", sequence: 1,
      createdAtUnixMs: 1000, updatedAtUnixMs: 1000, sourceCheckpointVersion: null,
      resultCheckpointVersion: null, progress: null, followupRequested: false, error: null,
    };
    transport.request.mockResolvedValue({ kind: "job", job });
    await expect(executeVceMaintenance("project-a", "rebuild", 4, null, ["recovery-action"], "click-rebuild", "sha256:current-model"))
      .resolves.toEqual({ kind: "job", job });
    expect(transport.request).toHaveBeenCalledExactlyOnceWith("service_execute_vce_maintenance", {
      request: { projectId: "project-a", mode: "rebuild", expectedSettingsRevision: 4,
        expectedCheckpointVersion: null, actionIds: ["recovery-action"], idempotencyKey: "click-rebuild", expectedEmbeddingFingerprint: "sha256:current-model" },
    });
  });

  // Repair and staging cleanup remain tied to an existing published checkpoint.
  // 修复和暂存清理仍然要求已有的已发布检查点。
  it("rejects checkpoint-less non-rebuild maintenance before transport", async () => {
    await expect(executeVceMaintenance("project-a", "repair", 4, null, ["repair-action"], "click-repair", "sha256:current-model"))
      .rejects.toThrow("要求一个已发布检查点");
    expect(transport.request).not.toHaveBeenCalled();
  });

  // Preserve explicit host rejection so failed admission cannot look like a started or completed job.
  // 保留明确宿主拒绝，避免准入失败伪装为已开始或已完成作业。
  it("propagates an obsolete-plan rejection without inventing a receipt", async () => {
    transport.request.mockRejectedValue(new Error("maintenance plan changed"));
    await expect(executeVceMaintenance("project-a", "repair", 4, 7, ["old-action"], "click-b", "sha256:preview-target"))
      .rejects.toThrow("maintenance plan changed");
  });

  // A disconnected browser must not emulate native indexing or call transport.
  // 未连接浏览器不能模拟原生索引或调用传输。
  it("requires a connected service", async () => {
    transport.available.mockReturnValue(false);
    await expect(executeVceMaintenance("project-a", "rebuild", 4, 7, ["action"], "click-c", "sha256:preview-target"))
      .rejects.toThrow("桌面服务连接");
    expect(transport.request).not.toHaveBeenCalled();
  });

  // Preserve the preview's exact model identity and dimension without reading a model picker.
  // 保留预览的精确模型身份与维度，不读取模型选择器。
  it("preserves the frozen model target in the maintenance preview", async () => {
    // plan represents the server's immutable target at preview time.
    // plan 表示服务端在预览时刻的不可变目标。
    const plan = { mode: "rebuild", actions: [], affectedBlobCount: 7, embeddingFingerprint: "sha256:preview-target", embeddingDimension: 1024 };
    transport.request.mockResolvedValue(plan);
    expect(await planVceMaintenance("project-a", "rebuild")).toEqual(plan);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith("service_plan_vce_maintenance", { request: { projectId: "project-a", mode: "rebuild" } });
  });

  // Offline mode cannot invent a target fingerprint or default dimension for human confirmation.
  // 离线模式不能为人为确认编造目标指纹或默认维度。
  it("rejects offline planning instead of fabricating an empty plan", async () => {
    transport.available.mockReturnValue(false);
    await expect(planVceMaintenance("project-a", "rebuild")).rejects.toThrow("需要连接桌面服务");
    expect(transport.request).not.toHaveBeenCalled();
  });
});
