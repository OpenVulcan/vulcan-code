import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  CommandExecutionPresentation,
  FileAccessPresentation,
  FileChangePresentation,
  GenericToolMarkdownPresentation,
  MarkdownToolPresentation,
  NativeSemanticToolPresentation,
  ProviderToolTextPresentation,
  SubagentOrchestrationPresentation,
} from "@/components/conversation/conversation-tool-presentations";
import { TooltipProvider } from "@/components/ui/tooltip";
import { I18nProvider } from "@/i18n";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";
import { ToolRawInvocationButton } from "./conversation-tool-execution";

// LIST_PRESENTATION represents one directory-list result whose target has no file extension.
// LIST_PRESENTATION 表示目标没有文件扩展名的目录列表结果。
const LIST_PRESENTATION: FileAccessPresentation = {
  action: "list",
  errorDetail: null,
  items: [{
    diffFile: null,
    displayPath: "D:/openvulcan",
    expandable: false,
    icon: { fallbackName: "openvulcan", iconName: null },
    id: "directory-list-target",
    lineRanges: [],
    path: "D:/openvulcan",
    pathKind: "external",
    resourceId: null,
    rows: [],
    textFormat: null,
  }],
  id: "directory-list",
  interactive: false,
  kind: "file_list",
  state: "completed",
};

// MARKDOWN_MENU_PRESENTATION represents a recognized Markdown menu that must start collapsed.
// MARKDOWN_MENU_PRESENTATION 表示一张必须默认折叠的已识别 Markdown 目录卡片。
const MARKDOWN_MENU_PRESENTATION: MarkdownToolPresentation = {
  contentKind: "markdown",
  icon: "folder_tree",
  id: "markdown-menu",
  kind: "markdown_tool_output",
  markdown: "# MARKDOWN MENU",
  sourceSections: [],
  state: "completed",
  summaryTargets: [{ lineRanges: [], numbered: null, path: "D:/openvulcan/docs" }],
  titleKey: "toolPresentation.codekit.markdownMenu.title",
  toolId: "vulcan_codekit_markdown_menu",
};

// PROVIDER_TEXT_PRESENTATION represents one completed non-native card with expandable content.
// PROVIDER_TEXT_PRESENTATION 表示一张带可展开内容的已完成非原生卡片。
const PROVIDER_TEXT_PRESENTATION: ProviderToolTextPresentation = {
  id: "provider-text",
  kind: "provider_tool_text",
  state: "completed",
  text: "Provider result",
};

// GET_GOAL_PRESENTATION represents one read-like native semantic result that starts folded.
// GET_GOAL_PRESENTATION 表示一条默认折叠的读取类原生语义结果。
const GET_GOAL_PRESENTATION: NativeSemanticToolPresentation = {
  arguments: {},
  family: "goal",
  id: "get-goal-result",
  kind: "native_semantic",
  result: { goal: null },
  state: "completed",
  titleKey: "toolPresentation.native.getGoal",
  toolId: "get_goal",
};

// SKILL_PRESENTATION represents one prompt-skill detail rendered as Markdown rather than JSON.
// SKILL_PRESENTATION 表示一份以 Markdown 而不是 JSON 渲染的提示词技能详情。
const SKILL_PRESENTATION: NativeSemanticToolPresentation = {
  arguments: { name: "terminal-session" },
  family: "skill",
  id: "terminal-session-skill",
  kind: "native_semantic",
  result: "<skill_content name=\"terminal-session\">\n# Terminal Session\n\nUse the persistent terminal safely.\n</skill_content>",
  state: "completed",
  titleKey: "toolPresentation.native.skill",
  toolId: "skill",
};

// TODO_PRESENTATION represents one native checklist update that must stay inside a folded card.
// TODO_PRESENTATION 表示一条必须保留在折叠卡片内的原生待办更新。
const TODO_PRESENTATION: NativeSemanticToolPresentation = {
  arguments: {
    explanation: "推进测试",
    items: [{ step: "补回动态面板", status: "in_progress" }],
  },
  family: "todo",
  id: "todo-update",
  kind: "native_semantic",
  result: null,
  state: "completed",
  titleKey: "toolPresentation.native.todolist",
  toolId: "todolist",
};

// GENERIC_PRESENTATION represents an unknown V1 tool whose persisted result has no hidden-detail RPC.
// GENERIC_PRESENTATION 表示一个没有隐藏详情 RPC 的未知 V1 工具持久化结果。
const GENERIC_PRESENTATION: GenericToolMarkdownPresentation = {
  id: "generic-v1-tool",
  kind: "generic_tool_markdown",
  markdown: "# Persisted result\n\nVisible only after expansion.",
  state: "completed",
  toolName: "custom_tool",
};

// MULTI_FILE_CHANGE represents one authoritative completed ChangeSet with two actual files.
// MULTI_FILE_CHANGE 表示一份包含两个实际文件的权威已完成 ChangeSet。
const MULTI_FILE_CHANGE: FileChangePresentation = {
  errorDetail: null,
  files: ["src/a.ts", "src/b.ts"].map((path, index) => ({
    added: index === 0 ? 2 : 1,
    diffFile: null,
    displayPath: path,
    expandable: false,
    icon: { fallbackName: path, iconName: null },
    id: `changed-${index}`,
    operation: "modify" as const,
    operationIds: [],
    path,
    pathKind: "project" as const,
    previousPath: null,
    readOnly: false,
    removed: index === 0 ? 1 : 0,
    resourceId: null,
    rows: [],
    textFormat: null,
  })),
  hasEffectiveChanges: true,
  id: "multi-file-change",
  interactive: false,
  kind: "file_change",
  mode: "applied",
  operation: "modify",
  source: "native_file_edit",
  state: "completed",
  totalAdded: 3,
  totalRemoved: 1,
};

// COMPLETED_COMMAND represents one native shell command after terminal completion.
// COMPLETED_COMMAND 表示一条进入终态后的原生 Shell 命令。
const COMPLETED_COMMAND: CommandExecutionPresentation = {
  command: "git status --short",
  exitCode: 0,
  id: "completed-command",
  kind: "command_execution",
  state: "completed",
  stderr: "",
  stdout: "",
};

// WAIT_AGENT_PRESENTATION represents one completed wait with authoritative child usage.
// WAIT_AGENT_PRESENTATION 表示一次携带权威子智能体用量的已完成等待。
const WAIT_AGENT_PRESENTATION: SubagentOrchestrationPresentation = {
  agents: [{
    id: "child-agent-1",
    name: "agentA",
    status: "completed",
    usageSummary: {
      cachedInputTokens: 30_000,
      providerCallCount: 2,
      toolCallCount: 0,
      totalTokens: 38_348,
    },
  }],
  agentsKnown: true,
  closedAgents: [],
  closedCount: 0,
  id: "wait-agent-usage",
  kind: "subagent_orchestration",
  operation: "wait",
  state: "completed",
  stoppedCount: 0,
  stoppedTaskIds: [],
  terminalEventReceived: true,
  title: "wait_agent",
};

describe("ToolPresentationCard file access icons", () => {
  // Goal operations share Target, TODO uses the Plan icon, and unrelated native tools stay unchanged.
  // 目标操作共用靶心，TODO 使用方案图标，其他原生工具保持不变。
  it.each([
    [GET_GOAL_PRESENTATION, "lucide-target"],
    [{ ...GET_GOAL_PRESENTATION, toolId: "create_goal" as const, titleKey: "toolPresentation.native.createGoal" as const }, "lucide-target"],
    [{ ...GET_GOAL_PRESENTATION, toolId: "update_goal" as const, titleKey: "toolPresentation.native.updateGoal" as const }, "lucide-target"],
    [TODO_PRESENTATION, "lucide-list-checks"],
    [SKILL_PRESENTATION, "lucide-braces"],
  ] as const)("renders %o with its semantic icon %s", (presentation, iconClass) => {
    // Both active and finished calls retain the same business icon.
    // 执行中和已完成调用保持同一个业务图标。
    for (const state of ["running", "completed"] as const) {
      const markup = renderToStaticMarkup(
        <I18nProvider initialLocale="zh-CN">
          <ToolPresentationCard
            expanded={false}
            expandedToolPresentationIds={new Set()}
            presentation={{ ...presentation, state }}
            requestProgress={null}
            detailLoadEnabled={false}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </I18nProvider>,
      );
      expect(markup).toContain(iconClass);
      if (presentation.family === "goal" || presentation.family === "todo") {
        expect(markup).not.toContain("lucide-braces");
      }
    }
  });

  // Raw invocation belongs only to expanded non-native content.
  // 原始调用只属于已展开的非原生内容。
  it("renders raw invocation only inside expanded non-native cards", () => {
    const collapsedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ToolPresentationCard
            detailLoadEnabled={true}
            expanded={false}
            expandedFooter={<ToolRawInvocationButton onOpen={() => undefined} />}
            expandedToolPresentationIds={new Set()}
            presentation={PROVIDER_TEXT_PRESENTATION}
            requestProgress={null}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </TooltipProvider>
      </I18nProvider>,
    );
    const expandedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ToolPresentationCard
            detailLoadEnabled={true}
            expanded
            expandedFooter={<ToolRawInvocationButton onOpen={() => undefined} />}
            expandedToolPresentationIds={new Set()}
            presentation={PROVIDER_TEXT_PRESENTATION}
            requestProgress={null}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(collapsedMarkup).not.toContain("显示原始工具调用输出");
    expect(expandedMarkup).toContain("显示原始工具调用输出");
  });

  // Native file cards reject raw invocation content even if a caller supplies it.
  // 即使调用方传入原始调用内容，原生文件卡片也必须拒绝展示。
  it("never renders raw invocation inside native file cards", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ToolPresentationCard
            detailLoadEnabled={true}
            expanded
            expandedFooter={<ToolRawInvocationButton onOpen={() => undefined} />}
            expandedToolPresentationIds={new Set()}
            presentation={{ ...LIST_PRESENTATION, interactive: true }}
            requestProgress={null}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(markup).not.toContain("显示原始工具调用输出");
  });

  // Directory-list summaries reserve no operation or file-type icon slot.
  // 目录列表摘要不预留操作图标或文件类型图标位置。
  it("renders directory-list summaries without an icon placeholder", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={LIST_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("查询文件列表");
    expect(markup).toContain("D:/openvulcan");
    expect(markup).not.toContain("file-access-operation-icon");
    expect(markup).not.toContain("file-change-icon-stack");
    expect(markup).not.toContain("file-change-fallback-icon");
  });

  // Read-like native semantic results expose a fold control and remain closed by default.
  // 读取类原生语义结果提供折叠控制，并默认保持关闭。
  it("keeps native semantic read results folded by default", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={GET_GOAL_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("读取目标");
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).not.toContain('&quot;goal&quot;');
  });

  // Prompt-skill cards retain the skill name in the folded row and render exact Markdown when opened.
  // 提示词技能卡在折叠行保留技能名称，并在展开时渲染精确 Markdown。
  it("renders prompt-skill identity and Markdown through the restored card", () => {
    const collapsedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={SKILL_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );
    const expandedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded
          expandedToolPresentationIds={new Set([SKILL_PRESENTATION.id])}
          presentation={SKILL_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(collapsedMarkup).toContain("查看技能 · terminal-session");
    expect(collapsedMarkup).toContain('aria-expanded="false"');
    expect(collapsedMarkup).not.toContain("Use the persistent terminal safely.");
    expect(expandedMarkup).toContain('aria-expanded="true"');
    expect(expandedMarkup).toContain("Terminal Session");
    expect(expandedMarkup).toContain("Use the persistent terminal safely.");
    expect(expandedMarkup).not.toContain("language-json");
  });

  // TODO and unknown tools must use the same default-folded interaction instead of leaking bodies inline.
  // TODO 与未知工具必须使用同一默认折叠交互，不能把正文直接泄露到行内。
  it.each([
    ["todo", TODO_PRESENTATION, "补回动态面板"],
    ["generic", GENERIC_PRESENTATION, "Visible only after expansion."],
  ] as const)("keeps %s tool content inside its folded card", (_kind, presentation, bodyText) => {
    const collapsedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={presentation}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );
    const expandedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded
          expandedToolPresentationIds={new Set([presentation.id])}
          presentation={presentation}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(collapsedMarkup).toContain('aria-expanded="false"');
    expect(collapsedMarkup).not.toContain(bodyText);
    expect(expandedMarkup).toContain('aria-expanded="true"');
    expect(expandedMarkup).toContain(bodyText);
  });

  // Recognized Markdown-menu cards expose a localized title and directory while remaining folded.
  // 已识别 Markdown 目录卡片在保持折叠时展示本地化标题与目录。
  it("renders the localized Markdown-menu directory in its folded summary", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={MARKDOWN_MENU_PRESENTATION}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("查询 Markdown 目录 · D:/openvulcan/docs");
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).not.toContain("vulcan_codekit_markdown_menu");
  });

  // Read cards use their own read-only icon and never inherit mutation icon slots.
  // 读取卡片使用独立只读图标，绝不继承变更图标槽位。
  it("uses an independent read-only icon for file reads", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={{
            ...LIST_PRESENTATION,
            action: "read",
            id: "file-read",
            kind: "file_read",
            items: [{
              ...LIST_PRESENTATION.items[0],
              displayPath: "README.md",
              icon: { fallbackName: "README.md", iconName: null },
              id: "file-read-target",
              lineRanges: [{ startLine: 10, endLine: 24 }],
              path: "D:/openvulcan/README.md",
            }],
          }}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("file-access-operation-icon");
    expect(markup).toContain("读取文件 · README.md · 第 10-24 行");
    expect(markup).not.toContain("file-change-operation-icon");
    expect(markup).not.toContain("file-change-counts");
  });

  // Completed edits expose actual files and line counts without expansion or generic terminal status text.
  // 已完成编辑无需展开即可展示实际文件与行数，且不显示通用终态状态文案。
  it("shows actual edit files and line counts directly", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={MULTI_FILE_CHANGE}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("src/a.ts");
    expect(markup).toContain("src/b.ts");
    expect(markup).toContain("+2");
    expect(markup).toContain("-1");
    expect(markup).not.toContain("执行成功");
    expect(markup).not.toContain("已编辑");
    expect(markup).not.toContain("已执行");
  });

  // Completed shell cards retain the command itself without a generic terminal-status prefix.
  // 已完成 Shell 卡片保留命令本身，且不附加通用终态状态前缀。
  it("does not prefix completed shell cards with executed status", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ToolPresentationCard
          detailLoadEnabled={false}
          expanded={false}
          expandedToolPresentationIds={new Set()}
          presentation={COMPLETED_COMMAND}
          requestProgress={null}
          onExpandedChange={() => undefined}
          onLoadToolDetail={async () => null}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("git status --short");
    expect(markup).not.toContain("已执行");
    expect(markup).not.toContain("执行失败");
  });

  // Completed child usage keeps its compact chip and exposes the dedicated Tooltip trigger.
  // 已完成子智能体用量保留紧凑胶囊，并暴露专用 Tooltip 触发器。
  it("binds completed subagent usage to a focusable Tooltip trigger", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ToolPresentationCard
            detailLoadEnabled={false}
            expanded={false}
            expandedToolPresentationIds={new Set()}
            presentation={WAIT_AGENT_PRESENTATION}
            requestProgress={null}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(markup).toContain("2 次调用 · 0 个工具 · 38348 tokens · 缓存 30000 tokens");
    expect(markup).toContain('data-subagent-usage-tooltip="true"');
    expect(markup).toContain('tabindex="0"');
    // The dark tooltip uses the shared label/value contrast, not the page's muted text color.
    // 深色弹窗使用共用标签/数值对比样式，不沿用页面的弱化文字色。
    expect(markup).toContain('class="context-usage-card-header turn-stats-card-header"');
    expect(markup).toContain('class="turn-stats-metric-grid"');
    expect(markup.match(/class="turn-stats-metric"/g)).toHaveLength(4);
    expect(markup).toContain('<span>总请求次数</span><strong>2</strong>');
    expect(markup).toContain('<span>工具调用次数</span><strong>0</strong>');
    expect(markup).toContain('<span>缓存 Tokens</span><strong>30,000</strong>');
    expect(markup).toContain('<span>合计 Tokens</span><strong>38,348</strong>');
    expect(markup).not.toContain("subagent-orchestration-usage-tooltip-grid");
  });

  // Every folded orchestration card keeps the operation name before its concrete agent or outcome.
  // 每个折叠调度卡都在具体智能体或结果前保留操作名称。
  it("keeps create, message, wait, list, and close operations visible in folded rows", () => {
    // cases cover the operation labels that were previously replaced by only an agent name.
    // cases 覆盖此前被单独智能体名称替代的操作文案。
    const cases: Array<{
      presentation: SubagentOrchestrationPresentation;
      expected: string;
    }> = [
      {
        presentation: {
          ...WAIT_AGENT_PRESENTATION,
          agents: [],
          id: "spawn-agent-summary",
          operation: "create",
          terminalEventReceived: false,
          title: "long-task",
        },
        expected: "创建智能体 · long-task",
      },
      {
        presentation: {
          ...WAIT_AGENT_PRESENTATION,
          agents: [],
          id: "message-agent-summary",
          operation: "message",
          target: "long-task",
          terminalEventReceived: false,
          title: "long-task",
        },
        expected: "向智能体发送消息 · long-task",
      },
      {
        presentation: WAIT_AGENT_PRESENTATION,
        expected: "等待智能体消息 · 已收到智能体消息",
      },
      {
        presentation: {
          ...WAIT_AGENT_PRESENTATION,
          id: "list-agent-summary",
          operation: "list_agents",
          terminalEventReceived: false,
          title: "list_agents",
        },
        expected: "查询智能体状态列表 · 1 智能体",
      },
      {
        presentation: {
          ...WAIT_AGENT_PRESENTATION,
          agents: [],
          id: "close-agent-summary",
          operation: "close",
          target: "long-task",
          terminalEventReceived: false,
          title: "long-task",
        },
        expected: "关闭智能体 · long-task",
      },
    ];

    for (const testCase of cases) {
      // markup captures the exact folded copy without browser-only interaction state.
      // markup 捕获不依赖浏览器交互状态的精确折叠文案。
      const markup = renderToStaticMarkup(
        <I18nProvider initialLocale="zh-CN">
          <TooltipProvider>
            <ToolPresentationCard
              detailLoadEnabled={false}
              expanded={false}
              expandedToolPresentationIds={new Set()}
              presentation={testCase.presentation}
              requestProgress={null}
              onExpandedChange={() => undefined}
              onLoadToolDetail={async () => null}
            />
          </TooltipProvider>
        </I18nProvider>,
      );
      expect(markup).toContain(testCase.expected);
    }
  });

  // A no-active-agents wait is terminal UI state, never a pending spinner label.
  // 无活动子代理等待属于终态 UI 状态，绝不能显示为等待中的旋转状态。
  it("renders no-active-agent waits as completed zero-work results", () => {
    // presentation models the authoritative immediate backend result.
    // presentation 模拟后端权威的即时返回结果。
    const presentation: SubagentOrchestrationPresentation = {
      ...WAIT_AGENT_PRESENTATION,
      agents: [],
      id: "wait-agent-no-active",
      noActiveAgents: true,
      terminalEventReceived: false,
    };
    // markup captures both the operation and terminal reason in the folded row.
    // markup 捕获折叠行中的操作名称和终态原因。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ToolPresentationCard
            detailLoadEnabled={false}
            expanded={false}
            expandedToolPresentationIds={new Set()}
            presentation={presentation}
            requestProgress={null}
            onExpandedChange={() => undefined}
            onLoadToolDetail={async () => null}
          />
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(markup).toContain("等待智能体消息 · 当前无未关闭的智能体。");
    expect(markup).not.toContain("等待智能体消息 · 等待中");
  });
});
