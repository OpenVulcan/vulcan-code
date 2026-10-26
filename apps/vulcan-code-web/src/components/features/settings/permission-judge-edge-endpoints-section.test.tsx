// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type {
  PermissionJudgeEdgeEndpoint,
  PermissionJudgeEdgeEndpoints,
  PermissionJudgeEdgeRequestAttempt,
  PermissionJudgeEdgeRequestTest,
} from "@/types";
import { formatMessage, type Translate } from "@/i18n";

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadPermissionJudgeEdgeEndpoints: vi.fn(),
  probePermissionJudgeEdgeEndpoints: vi.fn(),
  lockPermissionJudgeEdgeEndpoint: vi.fn(),
  testPermissionJudgeEdgeRequest: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);

const { PermissionJudgeEdgeEndpointsSection, endpointStatus } = await import("./permission-judge-edge-endpoints-section");

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
const t = ((key: string) => key) as Translate;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// endpoint builds one probed endpoint; overrides adjust the probe.
// endpoint 构建一个已探测端点；overrides 调整探测结果。
function endpoint(endpointId: string, overrides: Partial<PermissionJudgeEdgeEndpoint> = {}): PermissionJudgeEdgeEndpoint {
  return {
    endpointId, endpoint: `https://${endpointId}.example.com`, priority: 10, online: true, region: "us-west",
    ready: true, acceptsSite: true, rttMs: 40, computeP50Ms: 64, scoreMs: 104, error: null, ...overrides,
  };
}

// table builds an endpoint table of the given mode and selection.
// table 构建给定方式与选择的端点表。
function table(overrides: Partial<PermissionJudgeEdgeEndpoints> = {}): PermissionJudgeEdgeEndpoints {
  return {
    modelId: "m", site: "global", mode: "auto", lockedEndpointId: null, selectedEndpointId: "us-mini",
    probedAtMs: 1_790_000_000_000, nextProbeAtMs: 1_790_000_600_000, error: null,
    endpoints: [endpoint("us-mini"), endpoint("cn-mini", { rttMs: 180, scoreMs: 244 })],
    ...overrides,
  };
}

async function mount(snapshot: PermissionJudgeEdgeEndpoints) {
  service.loadPermissionJudgeEdgeEndpoints.mockResolvedValue(snapshot);
  await act(async () => root.render(<PermissionJudgeEdgeEndpointsSection pollIntervalMs={undefined} disabled={false} t={t} locale="en-US" />));
}

// The endpoint in use carries the current badge and every probe column is shown.
// 使用中的端点带有“使用中”标记，并显示每一列探测数据。
it("marks the endpoint in use and shows its probe", async () => {
  await mount(table());
  const current = host.querySelector("[data-judge-edge-current]");
  expect(current?.closest("[data-judge-edge-endpoint]")?.getAttribute("data-judge-edge-endpoint")).toBe("us-mini");
  expect(current?.textContent).toBe("settings.permissionJudge.edge.endpoints.current");
  const cells = [...host.querySelectorAll("tbody tr")[0].querySelectorAll("td")].map((cell) => cell.textContent);
  const ms = (value: number) => formatMessage("settings.permissionJudge.edge.endpoints.ms", { value });
  expect(cells.slice(1, 5)).toEqual(["us-west", ms(40), ms(64), ms(104)]);
});

// A locked endpoint is marked locked; a locked endpoint no longer offered raises a notice.
// 锁定的端点标为已锁定；不再提供的锁定端点给出提示。
it("marks a locked endpoint and warns when it is gone", async () => {
  await mount(table({ mode: "locked", lockedEndpointId: "cn-mini", selectedEndpointId: "cn-mini" }));
  expect(host.querySelector("[data-judge-edge-current]")?.textContent).toBe("settings.permissionJudge.edge.endpoints.locked");
  expect(host.querySelector("[data-judge-edge-locked-missing]")).toBeNull();
  await act(async () => root.unmount());
  root = createRoot(host);
  await mount(table({ mode: "locked", lockedEndpointId: "gone", selectedEndpointId: null }));
  expect(host.querySelector("[data-judge-edge-current]")).toBeNull();
  expect(host.querySelector("[data-judge-edge-locked-missing]")).not.toBeNull();
});

// Re-testing replaces the table with the probe's answer.
// 重新测试以探测的回答替换端点表。
it("replaces the table after a re-test", async () => {
  await mount(table());
  service.probePermissionJudgeEdgeEndpoints.mockResolvedValue(table({ selectedEndpointId: "cn-mini" }));
  const button = host.querySelector<HTMLButtonElement>("[data-judge-edge-probe]");
  await act(async () => button?.click());
  expect(service.probePermissionJudgeEdgeEndpoints).toHaveBeenCalledOnce();
  expect(host.querySelector("[data-judge-edge-current]")?.closest("[data-judge-edge-endpoint]")?.getAttribute("data-judge-edge-endpoint")).toBe("cn-mini");
});

// Probe outcomes map to distinct statuses, failure first.
// 探测结果映射到不同状态，失败优先。
it("maps probe outcomes to statuses", () => {
  const keys = [
    endpoint("a"),
    endpoint("a", { error: "unreachable", rttMs: null, scoreMs: null }),
    endpoint("a", { rttMs: null, region: null, ready: null, acceptsSite: null, scoreMs: null }),
    endpoint("a", { acceptsSite: false, scoreMs: null }),
    endpoint("a", { ready: false, scoreMs: null }),
  ].map((probe) => endpointStatus(probe).key);
  expect(new Set(keys).size).toBe(keys.length);
  expect(keys[1]).toBe("settings.permissionJudge.edge.endpoints.status.failed");
});

// attempt builds one request of a request test; overrides turn it into a failure.
// attempt 构建请求测试中的一个请求；overrides 将其变为失败。
function attempt(requestId: string, overrides: Partial<PermissionJudgeEdgeRequestAttempt> = {}): PermissionJudgeEdgeRequestAttempt {
  return {
    requestId, totalMs: 120, networkMs: 75, endpointMs: 45, nodeMs: 40, queueMs: 2, computeMs: 31, httpStatus: 200,
    tokenRenewed: false, errorCode: null, error: null,
    answer: {
      region: "us-west", model: "m", checkpointSha256: "a".repeat(64), policyVersion: "p",
      results: [{ label: "allow", logits: [2, 0.5, -1], inputTokens: 41, modelEvaluated: true, reasonCode: null }],
    },
    ...overrides,
  };
}

// A request test opens for the row's endpoint, sends the chosen sample there and lists every
// request with its stage times, the summary and the failures.
// 请求测试针对所在行的端点打开，向其发送所选示例，并列出每个请求的各阶段耗时、汇总与失败。
it("sends a request test to the row's endpoint and lists every request", async () => {
  await mount(table());
  const result: PermissionJudgeEdgeRequestTest = {
    modelId: "m", endpointId: "cn-mini", endpoint: "https://cn-mini.example.com", site: "global", renderedChars: 52,
    attempts: [
      attempt("r1"),
      attempt("r2", { httpStatus: 503, answer: null, errorCode: "region_saturated", error: "refused", networkMs: null, endpointMs: 3 }),
    ],
    succeeded: 1, minMs: 120, medianMs: 120, maxMs: 120,
  };
  service.testPermissionJudgeEdgeRequest.mockResolvedValue(result);
  await act(async () => host.querySelector<HTMLButtonElement>('[data-judge-edge-request-test-open="cn-mini"]')!.click());
  const dialog = document.querySelector('[data-judge-edge-request-test="cn-mini"]')!;
  expect(dialog).not.toBeNull();
  await act(async () => dialog.querySelector<HTMLButtonElement>("[data-judge-edge-test-send]")!.click());
  expect(service.testPermissionJudgeEdgeRequest).toHaveBeenCalledExactlyOnceWith("cn-mini", { kind: "preset", preset: "ordinary" });
  expect(dialog.querySelectorAll("tbody tr")).toHaveLength(2);
  expect(dialog.querySelector("[data-judge-edge-test-summary]")?.textContent).toContain("settings.permissionJudge.edge.requestTest.summary");
  expect(dialog.querySelector("[data-judge-edge-test-answer]")).not.toBeNull();
  expect(dialog.querySelector("[data-judge-edge-test-failures]")).not.toBeNull();
  expect(service.lockPermissionJudgeEdgeEndpoint).not.toHaveBeenCalled();
});
