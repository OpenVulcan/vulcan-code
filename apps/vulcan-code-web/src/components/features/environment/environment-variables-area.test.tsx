// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { EnvironmentVariablesSnapshot, SessionEnvironmentSnapshot } from "@/types";
import {
  checkEnvironmentVariableName,
  loadEnvironmentVariables,
  loadSessionEnvironment,
  saveEnvironmentVariable,
} from "@/service/environment-variables-service";
import { EnvironmentVariablesArea } from "./environment-variables-area";

vi.mock("@/service/environment-variables-service", () => ({
  checkEnvironmentVariableName: vi.fn(),
  deleteEnvironmentVariable: vi.fn(),
  deleteSessionEnvironmentVariable: vi.fn(),
  loadEnvironmentVariables: vi.fn(),
  loadSessionEnvironment: vi.fn(),
  saveEnvironmentVariable: vi.fn(),
}));

// snapshot holds one secret and one plain variable; the service never sends the secret value.
// snapshot 含一个密钥变量和一个普通变量；服务从不发送密钥值。
const snapshot: EnvironmentVariablesSnapshot = {
  variables: [
    { id: "v1", name: "VULCAN_ENV_TOKEN", kind: "secret", value: "", description: "Deploy token", global: true, projectIds: [], createdAtMs: 1, updatedAtMs: 1 },
    { id: "v2", name: "VULCAN_ENV_REGION", kind: "plain", value: "eu-west", description: "", global: false, projectIds: ["p1"], createdAtMs: 1, updatedAtMs: 1 },
  ],
  rules: { recommendedPrefix: "VULCAN_ENV_", reservedNames: ["VULCAN_HOME"], nameMaxChars: 128, valueMaxBytes: 32768, descriptionMaxChars: 500 },
};
const session: SessionEnvironmentSnapshot = {
  sessionId: "s1",
  variables: [{ name: "SESSION_ONLY", kind: "plain", value: "on", description: "", updatedAtMs: 1 }],
};

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(loadEnvironmentVariables).mockResolvedValue(snapshot);
  vi.mocked(loadSessionEnvironment).mockResolvedValue(session);
  vi.mocked(checkEnvironmentVariableName).mockImplementation(async (name) => ({ name, definedBySystem: false, hasRecommendedPrefix: true }));
  vi.mocked(saveEnvironmentVariable).mockResolvedValue(snapshot);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.clearAllMocks(); });

async function mount() {
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <EnvironmentVariablesArea
        conversations={[{ id: "s1", title: "First", status: "idle" }]}
        projects={[{ id: "p1", name: "Alpha", path: "/alpha" } as never]}
        selectedConversationId="s1"
      />
    </I18nProvider>,
  ));
  await act(async () => {});
}

it("masks secret values and lists the chosen session's own variables", async () => {
  await mount();
  const secret = host.querySelector('[data-environment-variable="VULCAN_ENV_TOKEN"] .vk-env-value');
  const plain = host.querySelector('[data-environment-variable="VULCAN_ENV_REGION"] .vk-env-value');
  expect(secret?.hasAttribute("data-secret")).toBe(true);
  expect(secret?.textContent).not.toContain("eu-west");
  expect(plain?.textContent).toBe("eu-west");
  expect(loadSessionEnvironment).toHaveBeenCalledWith("s1");
  expect(host.querySelector('[data-session-environment-variable="SESSION_ONLY"]')).not.toBeNull();
});

it("keeps a secret's stored value when its edit leaves the value empty", async () => {
  await mount();
  const edit = host.querySelector<HTMLButtonElement>('[data-environment-variable="VULCAN_ENV_TOKEN"] button[aria-label="Edit"]')!;
  await act(async () => edit.click());
  const dialog = document.querySelector("[data-environment-dialog]")!;
  const value = dialog.querySelector<HTMLInputElement>("#environment-variable-value")!;
  expect(value.value).toBe("");
  expect(value.type).toBe("password");
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 300)); });
  await act(async () => document.querySelector<HTMLButtonElement>("[data-environment-save]")!.click());
  expect(saveEnvironmentVariable).toHaveBeenCalledExactlyOnceWith({
    id: "v1", name: "VULCAN_ENV_TOKEN", value: undefined, kind: "secret", description: "Deploy token", global: true, projectIds: [],
  });
});
