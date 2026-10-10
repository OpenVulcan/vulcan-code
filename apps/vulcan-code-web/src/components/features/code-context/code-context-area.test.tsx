import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { VceJobSummary, VceProjectOverviewResponse } from "@/types";
import { CodeContextArea, loadAuthoritativeVceOverview } from "./code-context-area";
import { VceJobObserverProvider } from "./vce-job-observer-context";

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
      updateMode: "manual",
      autoUpdateMinIntervalSeconds: 300,
      revision: 4,
      maxFileBytes: 1_048_576,
      defaultIgnorePatterns: ["*.lock", "*.map", "generated/"],
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
      <VceJobObserverProvider serviceEpoch={0}>
      <CodeContextArea
        busy={busy}
        onOverviewChanged={vi.fn()}
        onRefresh={vi.fn<() => Promise<VceJobSummary>>()}
        onSave={vi.fn(async () => overview.settings)}
        activityError={null}
        onReloadActivity={vi.fn(async () => undefined)}
        onOpenEngineSettings={vi.fn()}
        overview={overview}
        projectId="project-a"
      />
      </VceJobObserverProvider>
    </I18nProvider>,
  );
}

describe("CodeContextArea", () => {
  // Global model failures leave project intent editable while disabling index creation and management reads.
  // 全局模型故障保留项目意图编辑，同时禁用索引创建及管理读取。
  it("keeps project settings and model navigation available while index actions are closed", () => {
    const overview = overviewFixture(true);
    overview.configuration.ready = false;
    overview.status.lifecycle = "blocked_configuration";
    overview.status.checkpointVersion = null;
    const markup = renderArea(overview);
    expect(markup).toContain("您可以保存项目设置");
    expect(markup).toContain("配置语义引擎模型");
    const createButton = markup.match(/<button[^>]*>[\s\S]*?<\/button>/g)?.find((button) => button.includes("创建语义索引"));
    expect(createButton).toContain('disabled=""');
    const enabledSwitch = markup.match(/<button[^>]*>/g)?.find((button) => button.includes('aria-label="启用代码语义索引"'));
    expect(enabledSwitch).toBeDefined(); expect(enabledSwitch).not.toContain('disabled=""');
    expect(markup).not.toContain("文件与 Blob");
  });
  // Initialization requires a separately labelled human action, with no fabricated management data.
  // 初始化要求单独标记的人为操作，不虚构管理数据。
  it("separates project enablement from explicit index creation", () => {
    const overview = overviewFixture(true);
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "uninitialized";
    overview.counts = null;
    const markup = renderArea(overview);
    expect(markup).toContain("创建语义索引");
    expect(markup).toContain("只有点击创建");
    expect(markup).toContain("索引更新模式");
    expect(markup).not.toContain("手动更新索引");
    expect(markup).not.toContain("文件与 Blob");
  });

  // A failed first publication exposes recovery planning even without a published checkpoint.
  // 首次发布失败时，即使没有已发布检查点，也要显示恢复计划入口。
  it("exposes rebuild recovery for an uninitialized failed index", () => {
    const overview = overviewFixture(true);
    overview.status.checkpointVersion = null;
    overview.status.lifecycle = "rebuild_required";
    overview.status.lastError = "initial publication failed";
    overview.counts = null;
    overview.integrity = null;
    const markup = renderArea(overview);
    expect(markup).toContain("索引需要重建");
    expect(markup).toContain("生成重建计划");
    const createButton = markup.match(/<button[^>]*>[\s\S]*?<\/button>/g)?.find((button) => button.includes("创建语义索引"));
    expect(createButton).toBeUndefined();
    expect(markup).not.toContain("文件与 Blob");
  });

  // Expanded management reads obtain a fresh host snapshot instead of reusing stale component props.
  // 展开的管理读取取得全新宿主快照，而不是复用陈旧组件属性。
  it("loads the authoritative project revision for management details", async () => {
    const authoritative = overviewFixture(true);
    authoritative.settings.revision = 5;
    authoritative.status.settingsRevision = 5;
    const loader = vi.fn(async () => authoritative);

    await expect(loadAuthoritativeVceOverview("project-a", loader)).resolves.toBe(authoritative);
    expect(loader).toHaveBeenCalledExactlyOnceWith("project-a");
  });

  // Management overview reads receive the caller-owned abort signal used by the panel lifetime.
  // 管理概览读取必须接收面板生命周期持有的中止信号。
  it("forwards the management overview abort signal to its loader", async () => {
    const authoritative = overviewFixture(true);
    // controller supplies the caller-owned signal expected to reach the loader.
    // controller 提供预期传递给加载器的调用方中止信号。
    const controller = new AbortController();
    // loader captures the exact project/options contract while returning an authoritative response.
    // loader 在返回权威响应的同时捕获精确的项目与选项契约。
    const loader = vi.fn(async (_projectId: string, options?: { signal?: AbortSignal }) => {
      expect(options?.signal).toBe(controller.signal);
      return authoritative;
    });

    await expect(loadAuthoritativeVceOverview("project-a", loader, { signal: controller.signal }))
      .resolves.toBe(authoritative);
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
    expect(markup).toContain("默认过滤规则");
    expect(markup).toContain("*.lock");
    expect(markup).toContain("自定义忽略规则");
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
