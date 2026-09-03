import { describe, expect, it } from "vitest";

import type { VmmConfigDraftSummary } from "@/types";

import { externalProjectBindingDraft } from "./project-settings-sections";

// PROJECT_DRAFT is a complete project VMM draft used to verify exact binding projection.
// PROJECT_DRAFT 是用于验证准确绑定投影的完整项目 VMM 草稿。
const PROJECT_DRAFT: VmmConfigDraftSummary = {
  hasOverride: false,
  backendMode: "external_grpc",
  nativeProjectBindingMode: "fixed",
  externalProjectBindingMode: "fixed",
  nativeFixedProjectId: "1",
  externalProjectPrefix: "vulcan/space",
  enabled: true,
  grpcTarget: "http://127.0.0.1:50051",
  grpcApiKey: "",
  managementBaseUrl: "http://127.0.0.1:8080",
  managementBearerToken: "",
  managementTimeoutMs: "30000",
  userId: "2",
  projectId: "1",
  receiveTimeoutMs: "30000",
  handshakeTimeoutMs: "10000",
  maxReceiveMessageBytes: "16777216",
  grpcKeepaliveTimeMs: "30000",
  grpcKeepaliveTimeoutMs: "10000",
  grpcKeepalivePermitWithoutCalls: false,
  aiToolsEnabled: true,
  precheckEnabled: true,
  memoryWritebackEnabled: true,
  precheckMode: "inherit",
  memoryWritebackMode: "inherit",
  implicitMemoryTurns: "8",
  profileRefreshTurns: "24",
  profileNodeLimit: "64",
  injectionTag: "memory",
  injectionDescription: "description",
};

describe("project VMM external binding draft", () => {
  it("freezes the returned project id without changing unrelated settings", () => {
    expect(externalProjectBindingDraft(PROJECT_DRAFT, " 42 ")).toEqual({
      ...PROJECT_DRAFT,
      hasOverride: true,
      projectId: "42",
    });
  });

  it("rejects missing or non-positive remote project ids", () => {
    expect(() => externalProjectBindingDraft(PROJECT_DRAFT, "0")).toThrow(
      "External VMM project binding requires a positive project id.",
    );
    expect(() => externalProjectBindingDraft(PROJECT_DRAFT, "unknown")).toThrow(
      "External VMM project binding requires a positive project id.",
    );
  });
});
