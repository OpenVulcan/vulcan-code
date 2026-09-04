import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { VceProjectOverviewResponse } from "@/types";
import { CodeContextArea, loadAuthoritativeVceOverview } from "./code-context-area";

// overviewFixture builds one complete management-safe project projection.
// overviewFixture 构建一份完整且管理安全的项目投影。
function overviewFixture(enabled: boolean): VceProjectOverviewResponse {
  return {
    configuration: {
      ready: true,
      consumerProfileId: "vce",
      queryRewritePurposeId: "code_retrieval.query_rewrite",
      reasoningPolicy: "fixed_off",
      embedding: { configured: true, available: true },
      queryRewrite: { configured: true, available: true },
      rerank: { configured: true, available: true },
      issues: [],
    },
    settings: {
      enabled,
      revision: 4,
      migratedFromCodebaseMemory: true,
      maxFileBytes: 1_048_576,
      extraIgnorePatterns: [],
      allowQueryRewrite: false,
      allowRerank: true,
      embeddingMaxConcurrency: 4,
      queryMaxConcurrency: 3,
      maxResults: 10,
      maxContextChars: 32_000,
    },
    status: {
      projectId: "project-a",
      enabled,
      settingsRevision: 4,
      lifecycle: enabled ? "ready" : "disabled",
      engineState: enabled ? "ready" : null,
      workspaceGeneration: enabled ? 3 : null,
      checkpointVersion: enabled ? 2 : null,
      presentFiles: enabled ? 7 : 0,
      checkpointBlobs: enabled ? 6 : 0,
      lastError: null,
    },
    ai: enabled ? {
      registered: true,
      embeddingAvailable: true,
      queryRewriteAvailable: true,
      rerankAvailable: true,
    } : null,
    counts: enabled ? {
      files: 7,
      searchableFiles: 6,
      skippedFiles: 1,
      deletedFiles: 0,
      blobs: 6,
      chunks: 18,
      vectors: 18,
      pathDocuments: 6,
      symbols: 11,
    } : null,
    integrity: enabled ? {
      indexExists: true,
      searchable: true,
      checkpointBlobCount: 6,
      indexedBlobCount: 6,
      missingBlobCount: 0,
      orphanBlobCount: 0,
      detail: null,
    } : null,
  };
}

// renderArea renders one locale-specific code-context surface without running browser effects.
// renderArea 在不运行浏览器副作用的情况下渲染指定语言的代码上下文界面。
function renderArea(overview: VceProjectOverviewResponse, busy = false, locale = "zh-CN") {
  return renderToStaticMarkup(
    <I18nProvider initialLocale={locale}>
      <CodeContextArea
        busy={busy}
        onOverviewChanged={vi.fn()}
        onRefresh={vi.fn(async () => undefined)}
        onSave={vi.fn(async () => undefined)}
        overview={overview}
        projectId="project-a"
      />
    </I18nProvider>,
  );
}

describe("CodeContextArea", () => {
  // Rejected saves must recover from a fresh host snapshot instead of reusing stale component props.
  // 被拒绝的保存必须从新的宿主快照恢复，而不是复用陈旧组件 props。
  it("loads the authoritative project revision after a rejected save", async () => {
    const authoritative = overviewFixture(true);
    authoritative.settings.revision = 5;
    authoritative.status.settingsRevision = 5;
    const loader = vi.fn(async () => authoritative);

    await expect(loadAuthoritativeVceOverview("project-a", loader)).resolves.toBe(authoritative);
    expect(loader).toHaveBeenCalledExactlyOnceWith("project-a");
  });

  // Recovery rejects a cross-project response before it can replace the mounted project's draft.
  // 恢复流程会在跨项目响应替换当前项目草稿前拒绝该响应。
  it("rejects a mismatched project during authoritative recovery", async () => {
    const mismatched = overviewFixture(true);
    mismatched.status.projectId = "project-b";

    await expect(loadAuthoritativeVceOverview("project-a", async () => mismatched))
      .rejects.toThrow("mismatched project identity");
  });

  // Ready projections render bounded semantic statistics without exposing provider internals.
  // 就绪投影会渲染有界语义统计，且不暴露 Provider 内部信息。
  it("renders the ready Chinese management surface", () => {
    const markup = renderArea(overviewFixture(true));

    expect(markup).toContain("代码语义索引");
    expect(markup).toContain("ready");
    expect(markup).toContain("代码块");
    expect(markup).toContain(">18<");
    expect(markup).not.toContain("test-provider");
  });

  // Disabled projections hide management lists while preserving the explicit enable control.
  // 禁用投影会隐藏管理列表，同时保留明确的启用控件。
  it("renders disabled state without fabricated index data", () => {
    const markup = renderArea(overviewFixture(false), false, "en-US");

    expect(markup).toContain("Enable code semantic index");
    expect(markup).toContain("disabled");
    expect(markup).not.toContain("Files and Blobs");
    expect(markup).not.toContain("Operation history");
  });

  // Busy projections disable every editable advanced setting until the host mutation settles.
  // 忙碌投影会在宿主修改收敛前禁用每项可编辑高级设置。
  it("disables advanced settings while a host mutation is active", () => {
    const markup = renderArea(overviewFixture(true), true);

    expect(markup.match(/disabled=""/g)?.length ?? 0).toBeGreaterThanOrEqual(9);
  });
});
