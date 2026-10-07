import { describe, expect, it } from "vitest";

import { contextCompactionProgressKey } from "./conversation-blocks";

// DeclaredCompactionStage lists every closed stage emitted directly by prompt_compaction.rs.
// DeclaredCompactionStage 列出 prompt_compaction.rs 直接发送的全部封闭阶段。
const declaredCompactionStages = [
  "prepared",
  "request_ready",
  "provider_lowering_repair",
  "wire_request_ready",
  "request_started",
  "thinking_delta",
  "output_delta",
  "prepare_remote_tooling",
  "remote_compaction_check",
  "remote_compaction_complete",
  "remote_compaction_local_fallback",
  "prepare_input",
  "build_request",
  "prepare_runtime",
  "runtime_ready",
  "prepare_execution",
  "execution_ready",
  "bootstrap_runtime",
] as const;

describe("context compaction progress localization", () => {
  // Every declared backend stage must resolve to a concrete user-facing key.
  // 每一个已声明后端阶段都必须解析为具体的用户可见文案键。
  it("registers every backend compaction stage", () => {
    for (const stage of declaredCompactionStages) {
      expect(contextCompactionProgressKey(stage)).not.toBe("contextCompaction.progress.unknown");
    }
  });

  // Unknown future stages retain the explicit forward-compatible fallback.
  // 未知未来阶段继续使用显式的前向兼容回退文案。
  it("keeps an explicit fallback for future stages", () => {
    expect(contextCompactionProgressKey("future_stage")).toBe("contextCompaction.progress.unknown");
  });
});
