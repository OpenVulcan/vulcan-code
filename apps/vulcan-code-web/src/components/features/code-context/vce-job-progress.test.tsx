import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { VceJobSummary } from "@/types";
import { isVceJobActive, mergeVceJobReceipt } from "./use-vce-project-job";
import { VceJobProgress, vceSynchronizationControl } from "./vce-job-progress";

// Create one complete receipt for deterministic progress and event-ordering assertions.
// 创建完整回执用于确定性进度与事件排序断言。
// Takes no parameters. Returns an isolated queued initialization receipt.
// 无参数。返回独立的排队初始化回执。
function receipt(): VceJobSummary {
  return {
    projectId: "project-a", jobId: "job-a", kind: "initialize", state: "queued", sequence: 1,
    createdAtUnixMs: 1000, updatedAtUnixMs: 1000, sourceCheckpointVersion: null,
    resultCheckpointVersion: null, progress: null, followupRequested: false, error: null,
  };
}

describe("durable VCE job projections", () => {
  // Both entry points offer one successor only during synchronization; initialization never queues a second build.
  // 两个入口仅在同步期间提供一个后续批次；初始化绝不排入第二次建库。
  it("offers one visible coalesced synchronization and keeps its cancellation explicit", () => {
    const initial = receipt();
    expect(vceSynchronizationControl(initial, true).disabled).toBe(true);
    const running: VceJobSummary = { ...initial, kind: "synchronize", state: "running", sourceCheckpointVersion: 1 };
    expect(vceSynchronizationControl(running, true)).toEqual({ disabled: false, label: "本次完成后再同步" });
    const pending = { ...running, followupRequested: true };
    expect(vceSynchronizationControl(pending, true)).toEqual({ disabled: true, label: "已安排后续更新" });
    const markup = renderToStaticMarkup(<VceJobProgress job={pending} chinese onCancel={() => undefined} />);
    expect(markup).toContain("已合并一次后续手动同步");
    expect(markup).toContain("取消更新及后续同步");
    expect(vceSynchronizationControl({ ...running, state: "cancelling" }, false).disabled).toBe(true);
    expect(vceSynchronizationControl({ ...running, state: "succeeded" }, true).disabled).toBe(false);
  });

  // A terminal persistence diagnostic keeps the recovery cancellation action enabled even after the last phase.
  // 终态持久化诊断即使出现在最后阶段，也必须保持恢复取消入口可用。
  it("keeps recovery cancellation available for an active receipt error", () => {
    const job: VceJobSummary = {
      ...receipt(), state: "running", error: "Persist VCE terminal job receipt failed: transport error",
      progress: { phase: "complete", operationId: "job-a", unit: "batches", completed: 1, total: 1, currentPath: null },
    };
    const markup = renderToStaticMarkup(<VceJobProgress job={job} chinese={false} onCancel={() => undefined} />);
    expect(markup).toContain("Recover and cancel");
    expect(markup).not.toContain('disabled=""');
  });
  // Slow reads and reordered events cannot roll back a terminal result or erase an observed job.
  // 缓慢读取与乱序事件不能回退终态结果或擦除已观察作业。
  it("keeps the newest acknowledged sequence", () => {
    const queued = receipt();
    const succeeded: VceJobSummary = { ...queued, state: "succeeded", sequence: 6, resultCheckpointVersion: 1 };
    expect(mergeVceJobReceipt(succeeded, queued, "project-a")).toBe(succeeded);
    expect(mergeVceJobReceipt(succeeded, null, "project-a")).toBe(succeeded);
    expect(mergeVceJobReceipt(queued, succeeded, "project-a")).toBe(succeeded);
    expect(isVceJobActive(succeeded)).toBe(false);
    expect(isVceJobActive(queued)).toBe(true);
  });

  // Job identity ordering is project-scoped; older histories never replace a newer job.
  // 作业身份排序限于项目；旧历史绝不替换较新作业。
  it("rejects cross-project data and stale previous jobs", () => {
    const first = receipt();
    const newer = { ...first, jobId: "job-b", createdAtUnixMs: 2000 };
    expect(mergeVceJobReceipt(first, newer, "project-a")).toBe(newer);
    expect(mergeVceJobReceipt(newer, first, "project-a")).toBe(newer);
    expect(() => mergeVceJobReceipt(first, { ...newer, projectId: "project-b" }, "project-a")).toThrow();
  });

  // Queued jobs must not claim any scans, model calls, percentages, or initialized search tools.
  // 排队作业不得宣称已扫描、调用模型、产生百分比或初始化检索工具。
  it("renders queued initialization as genuinely indeterminate", () => {
    const markup = renderToStaticMarkup(<VceJobProgress job={receipt()} chinese />);
    expect(markup).toContain("已排队");
    expect(markup).toContain("尚未开始扫描或调用模型");
    expect(markup).toContain("首次索引完成后");
    expect(markup).not.toContain('value="');
    expect(markup).not.toContain("100%");
  });

  // Known batch denominators use actual completed response counts while retaining old-query messaging.
  // 已知批次分母使用实际已完成响应数，同时保留旧查询可用提示。
  it("renders measured progress and preserves ongoing conversations", () => {
    const job: VceJobSummary = { ...receipt(), kind: "synchronize", state: "running", sourceCheckpointVersion: 2,
      progress: { phase: "embedding", operationId: "job-a:document:1", unit: "batches", completed: 3, total: 7, overallCompleted: 4, overallTotal: 12, currentPath: "src/main.rs" } };
    const markup = renderToStaticMarkup(<VceJobProgress job={job} chinese />);
    expect(markup).toContain('value="3"');
    expect(markup).toContain('max="7"');
    expect(markup).toContain("整体进度");
    expect(markup).toContain('value="4"');
    expect(markup).toContain('max="12"');
    expect(markup).toContain("会话和已有索引查询仍可正常使用");
    job.progress!.total = null;
    job.progress!.overallTotal = null;
    const unknown = renderToStaticMarkup(<VceJobProgress job={job} chinese />);
    expect(unknown).not.toContain('value="');
  });

  // A failed update does not turn the previously committed checkpoint into an uninitialized index.
  // 更新失败不把先前已提交检查点变成未初始化索引。
  it("distinguishes failed jobs from first initialization", () => {
    const job: VceJobSummary = { ...receipt(), state: "failed", sourceCheckpointVersion: 0, error: "模型调用失败" };
    const markup = renderToStaticMarkup(<VceJobProgress job={job} chinese />);
    expect(markup).toContain("本次索引更新失败");
    expect(markup).toContain("模型调用失败");
    expect(markup).not.toContain("<progress");
    expect(isVceJobActive(job)).toBe(false);
  });
});
