import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import { PublicAiModelCapabilityDialog } from "@/components/features/settings/public-ai-model-capability-dialog";
import { CredentialManagementArea, PublicAiConnectionDialog } from "@/components/features/settings/provider-list-settings";
import { ProviderManagementArea } from "@/components/features/settings/provider-management-settings";
import { ProviderManifestStatusPanel } from "@/components/features/settings/provider-manifest-status";
import { createAuditPublicAi } from "./fixtures";

// Every mutation and read below is a detached mock, never a provider or local configuration write.
// 下方所有变更和读取均为独立模拟，不写供应商或本地配置。
const service = vi.hoisted(() => ({ load: vi.fn(), modelSave: vi.fn(), connectionSave: vi.fn(), providerSave: vi.fn(), prioritySave: vi.fn(), manifestGet: vi.fn(), manifestSync: vi.fn(), oauthStart: vi.fn(), oauthStatus: vi.fn(), oauthCommit: vi.fn(), oauthCancel: vi.fn(), connectionDelete: vi.fn() }));
vi.mock("@/service/settings-actions-service", async (importOriginal) => ({
  ...await importOriginal<object>(), loadPublicAiSettings: service.load,
  savePublicAiModelCapabilityOverrides: service.modelSave, addPublicAiConnection: service.connectionSave,
  savePublicAiCustomProvider: service.providerSave, getProviderManifestStatus: service.manifestGet,
  savePublicAiProviderPriority: service.prioritySave,
  syncProviderManifest: service.manifestSync,
  startPublicAiOAuth: service.oauthStart, getPublicAiOAuthStatus: service.oauthStatus,
  commitPublicAiOAuthConnection: service.oauthCommit, cancelPublicAiOAuth: service.oauthCancel,
  removePublicAiConnection: service.connectionDelete,
}));

// Shared test ownership guarantees portal cleanup even when an assertion fails.
// 共享测试所有权保证断言失败时也清理 Portal。
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

// Render actual production controls with English labels and the real shared primitives.
// 使用英文文案与真实共享原语渲染生产控件。
async function render(content: React.ReactNode) {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><TooltipProvider>{content}</TooltipProvider></I18nProvider>));
}
// Change through React's native input event rather than invoking component internals.
// 通过 React 原生输入事件修改，不调用组件内部函数。
async function type(input: HTMLInputElement | HTMLTextAreaElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
// Locate one visible action by exact text; absence must fail the test.
// 按精确文本定位可见动作，缺失必须使测试失败。
function button(label: string): HTMLButtonElement {
  const result = [...document.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent === label);
  expect(result, label).toBeTruthy(); return result!;
}

it("locks the model draft and close action until a rejected save restores editing", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  const model = snapshot.catalogs["audit-account-1"].models[0];
  let reject!: (error: Error) => void;
  service.modelSave.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  const close = vi.fn();
  await render(<PublicAiModelCapabilityDialog model={model} onOpenChange={close} onSaved={vi.fn()} open providerId="custom:audit-1" providerDisplayName="Audit" />);
  const input = document.querySelector<HTMLInputElement>('input:not([type="hidden"])')!;
  await type(input, "64000");
  await act(async () => { button("Save").click(); button("Save").click(); });
  expect(service.modelSave).toHaveBeenCalledTimes(1);
  expect(service.modelSave.mock.calls[0][0].overrides).toEqual({ context_limit: 64000 });
  expect(input.matches(":disabled")).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')?.click());
  expect(close).not.toHaveBeenCalled();
  await act(async () => reject(new Error("Synthetic model write failed")));
  expect(input.matches(":disabled")).toBe(false);
  expect(input.value).toBe("64000");
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic model write failed");
  expect(document.querySelector('[role="dialog"]')?.getAttribute("data-scroll-mode")).toBe("fixed-footer");
});

it("retains a credential draft across equivalent snapshots and blocks closing while saving", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  const close = vi.fn();
  let reject!: (error: Error) => void;
  service.connectionSave.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  const content = (providers = snapshot.providers) => <PublicAiConnectionDialog connection={null} initialProviderId="custom:audit-1" lockedProviderId="custom:audit-1" onOpenChange={close} onSaved={vi.fn()} open providers={providers} />;
  await render(content());
  const name = document.querySelector<HTMLInputElement>('input:not([type="hidden"])')!;
  await type(name, "Draft account");
  await type(document.querySelector<HTMLInputElement>('input[type="password"]')!, "synthetic-test-value");
  await render(content(structuredClone(snapshot.providers)));
  expect(name.value).toBe("Draft account");
  const save = document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!;
  await act(async () => { save.click(); save.click(); });
  expect(service.connectionSave).toHaveBeenCalledTimes(1);
  expect(name.matches(":disabled")).toBe(true);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')?.click());
  expect(close).not.toHaveBeenCalled();
  await act(async () => reject(new Error("Synthetic credential write failed")));
  expect(name.matches(":disabled")).toBe(false);
  expect(name.value).toBe("Draft account");
});

it("preserves a rejected manifest reason after the status refresh and serializes reads", async () => {
  const status = { provider_id: "audit", sync_configured: true, quarantine: [], model_rules: [], model_bindings: [], active_model_binding_changes: [], operations: [], tool_target_template_instances: [], model_binding_total_count: 0 };
  service.manifestGet.mockResolvedValue([status]);
  service.manifestSync.mockResolvedValue({ outcome: "rejected", failure_kind: "signature_invalid", failure_message: "Synthetic rejection" });
  await render(<ProviderManifestStatusPanel />);
  await act(async () => button("同步").click());
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic rejection");
  expect(service.manifestGet).toHaveBeenCalledTimes(2);
  let resolve!: (value: unknown) => void;
  service.manifestGet.mockImplementation(() => new Promise((done) => { resolve = done; }));
  await act(async () => { button("刷新").click(); button("刷新").click(); });
  expect(service.manifestGet).toHaveBeenCalledTimes(3);
  expect(button("同步").disabled).toBe(true);
  await act(async () => resolve([status]));
  expect(button("同步").disabled).toBe(false);
});

it("allows typing unfinished custom header rows without silently persisting the old map", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  snapshot.connections = [];
  snapshot.providers[0].service_entries[0].protocol_routes = [
    { protocol_profile_id: "openai-responses.v1", endpoint_path: "/responses", verified: true },
    { protocol_profile_id: "openai-chat-completions.v1", endpoint_path: "/chat/completions", verified: true },
  ];
  service.load.mockResolvedValue(snapshot);
  await render(<ProviderManagementArea />);
  const category = [...document.querySelectorAll<HTMLButtonElement>("nav button")].find((entry) => entry.textContent?.startsWith("Custom"))!;
  expect(category).toBeTruthy();
  await act(async () => category.click());
  const edit = [...document.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.getAttribute("aria-label")?.startsWith("Edit "))!;
  expect(edit).toBeTruthy();
  await act(async () => edit.click());
  const textarea = document.querySelector<HTMLTextAreaElement>("textarea")!;
  expect(textarea).toBeTruthy();
  await type(textarea, "x");
  expect(textarea.value).toBe("x");
  expect(textarea.getAttribute("aria-invalid")).toBe("true");
  expect(document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!.disabled).toBe(true);
  expect(service.providerSave).not.toHaveBeenCalled();
  await type(textarea, "x-audit: value:with:colons");
  expect(textarea.value).toBe("x-audit: value:with:colons");
  expect(textarea.getAttribute("aria-invalid")).toBe("false");
  service.providerSave.mockRejectedValue(new Error("Synthetic provider write failed"));
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!.click());
  expect(service.providerSave).toHaveBeenCalledTimes(1);
  expect(service.providerSave.mock.calls[0][0].protocolRoutes[0].request_headers).toEqual({ "x-audit": "value:with:colons" });
  expect(textarea.value).toBe("x-audit: value:with:colons");
});

it("does not restart OAuth commit when the parent callback changes and locks close during commit", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  const profile = snapshot.providers[0].service_entries[0].credential_profiles[0];
  profile.credential_mode = "auth";
  profile.authorization = { kind: "registered_oauth", provider: "open_ai_codex" };
  const transaction = { transactionId: "synthetic-oauth", authorizationBinding: "audit", phase: "waiting_for_callback", expiresAtUnixMs: 1 };
  service.oauthStart.mockResolvedValue({ transaction, authorizationUrl: "https://audit.invalid", flowKind: "loopback_callback" });
  service.oauthStatus.mockResolvedValue({ ...transaction, phase: "succeeded" });
  let resolve!: (value: unknown) => void;
  service.oauthCommit.mockImplementation(() => new Promise((done) => { resolve = done; }));
  const close = vi.fn();
  const firstSaved = vi.fn(); const latestSaved = vi.fn();
  const content = (onSaved: typeof firstSaved) => <PublicAiConnectionDialog connection={null} initialProviderId="custom:audit-1" lockedProviderId="custom:audit-1" onOpenChange={close} onSaved={onSaved} open providers={snapshot.providers} />;
  await render(content(firstSaved));
  await type(document.querySelector<HTMLInputElement>('input:not([type="hidden"])')!, "OAuth draft");
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!.click());
  expect(service.oauthCommit).toHaveBeenCalledTimes(1);
  await render(content(latestSaved));
  const closeButton = document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')!;
  expect(closeButton).toBeTruthy();
  await act(async () => closeButton.click());
  expect(close).not.toHaveBeenCalled();
  expect(service.oauthCancel).not.toHaveBeenCalled();
  expect(service.oauthStatus).toHaveBeenCalledTimes(1);
  await act(async () => resolve(snapshot));
  expect(firstSaved).not.toHaveBeenCalled();
  expect(latestSaved).toHaveBeenCalledExactlyOnceWith(snapshot);
});

it("keeps a deleting account dialog open and editable after a rejected deletion", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  service.load.mockResolvedValue(snapshot);
  let reject!: (error: Error) => void;
  service.connectionDelete.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  await render(<CredentialManagementArea />);
  const remove = document.querySelector<HTMLButtonElement>('button[aria-label="Delete credential"]')!;
  expect(remove).toBeTruthy();
  await act(async () => remove.click());
  const confirm = document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!;
  await act(async () => { confirm.click(); confirm.click(); });
  expect(service.connectionDelete).toHaveBeenCalledTimes(1);
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')!.click());
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  await act(async () => reject(new Error("Synthetic deletion failure")));
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic deletion failure");
  expect(confirm.disabled).toBe(false);
});

it("does not close an OAuth dialog until cancellation is acknowledged", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  const profile = snapshot.providers[0].service_entries[0].credential_profiles[0];
  profile.credential_mode = "auth";
  profile.authorization = { kind: "registered_oauth", provider: "open_ai_codex" };
  const transaction = { transactionId: "synthetic-cancel", authorizationBinding: "audit", phase: "waiting_for_callback", expiresAtUnixMs: 1 };
  service.oauthStart.mockResolvedValue({ transaction, authorizationUrl: "https://audit.invalid", flowKind: "loopback_callback" });
  service.oauthStatus.mockImplementation(() => new Promise(() => {}));
  service.oauthCancel.mockRejectedValueOnce(new Error("Synthetic cancellation failure")).mockResolvedValue({ ...transaction, phase: "cancelled" });
  const close = vi.fn();
  await render(<PublicAiConnectionDialog connection={null} initialProviderId="custom:audit-1" lockedProviderId="custom:audit-1" onOpenChange={close} onSaved={vi.fn()} open providers={snapshot.providers} />);
  await type(document.querySelector<HTMLInputElement>('input:not([type="hidden"])')!, "OAuth draft");
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button:last-child')!.click());
  const closeButton = document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')!;
  await act(async () => closeButton.click());
  expect(close).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic cancellation failure");
  await act(async () => closeButton.click());
  expect(service.oauthCancel).toHaveBeenCalledTimes(2);
  expect(close).toHaveBeenCalledExactlyOnceWith(false);
});

it("locks all provider rows and account creation during a priority write", async () => {
  const snapshot = createAuditPublicAi("invalid-default");
  service.load.mockResolvedValue(snapshot);
  let reject!: (error: Error) => void;
  service.prioritySave.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  await render(<ProviderManagementArea />);
  const category = [...document.querySelectorAll<HTMLButtonElement>("nav button")].find((entry) => entry.textContent?.startsWith("Custom"))!;
  await act(async () => category.click());
  const priorities = [...document.querySelectorAll<HTMLInputElement>('input[type="number"]')];
  expect(priorities).toHaveLength(2);
  await type(priorities[0], "5");
  const save = priorities[0].parentElement!.querySelector<HTMLButtonElement>("button")!;
  await act(async () => { save.click(); save.click(); });
  expect(service.prioritySave).toHaveBeenCalledExactlyOnceWith("custom:audit-1", 5);
  expect(priorities.every((input) => input.disabled)).toBe(true);
  const add = document.querySelector<HTMLButtonElement>('button[aria-label^="Add credential"]')!;
  expect(add).toBeTruthy(); expect(add.disabled).toBe(true);
  await act(async () => reject(new Error("Synthetic priority write failed")));
  expect(priorities.every((input) => !input.disabled)).toBe(true);
  expect(add.disabled).toBe(false);
});
