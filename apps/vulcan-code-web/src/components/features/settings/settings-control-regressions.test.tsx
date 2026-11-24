// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { PublicAiModelCapabilityDialog } from "@/components/features/settings/public-ai-model-capability-dialog";
import { PublicAiConnectionDialog } from "@/components/features/settings/provider-list-settings";
import { ModelServicesArea } from "@/components/features/settings/model-services/model-services-area";
import { ConfigurationArchiveDialog } from "@/components/features/settings/model-services/configuration-archive-dialog";
import { ProviderConnectPanel } from "@/components/features/settings/model-services/provider-connect";
import { ProviderManifestStatusPanel } from "@/components/features/settings/provider-manifest-status";
import { publicAiSettingsWithInvalidDefaultFixture } from "@/test-fixtures/public-ai-settings";

// Every mutation and read below is a detached mock, never a provider or local configuration write.
// 下方所有变更和读取均为独立模拟，不写供应商或本地配置。
const service = vi.hoisted(() => ({
  load: vi.fn(), modelSave: vi.fn(), connectionSave: vi.fn(), providerSave: vi.fn(), prioritySave: vi.fn(),
  manifestGet: vi.fn(), manifestSync: vi.fn(), oauthStart: vi.fn(), oauthStatus: vi.fn(), oauthCommit: vi.fn(),
  oauthCancel: vi.fn(), connectionDelete: vi.fn(), archiveExport: vi.fn(), archivePreview: vi.fn(), archiveImport: vi.fn(),
}));
vi.mock("@/service/settings-actions-service", async (importOriginal) => ({
  ...await importOriginal<object>(), loadPublicAiSettings: service.load,
  savePublicAiModelCapabilityOverrides: service.modelSave, addPublicAiConnection: service.connectionSave,
  savePublicAiCustomProvider: service.providerSave, getProviderManifestStatus: service.manifestGet,
  savePublicAiProviderPriority: service.prioritySave,
  syncProviderManifest: service.manifestSync,
  startPublicAiOAuth: service.oauthStart, getPublicAiOAuthStatus: service.oauthStatus,
  commitPublicAiOAuthConnection: service.oauthCommit, cancelPublicAiOAuth: service.oauthCancel,
  removePublicAiConnection: service.connectionDelete,
  exportPublicAiConfiguration: service.archiveExport,
  previewPublicAiConfigurationImport: service.archivePreview,
  importPublicAiConfiguration: service.archiveImport,
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
// Locate one visible action by its exact text or accessible name; absence must fail the test.
// 按精确文本或无障碍名称定位可见动作，缺失必须使测试失败。
function button(label: string): HTMLButtonElement {
  const result = [...document.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent === label || entry.getAttribute("aria-label") === label);
  expect(result, label).toBeTruthy(); return result!;
}
// Open a menu from its trigger by keyboard, as Radix does, and return the item whose text starts with the label.
// 像 Radix 一样通过键盘从触发按钮打开菜单，并返回文本以该标签开头的菜单项。
async function menuItem(trigger: HTMLElement | null, label: string): Promise<HTMLElement> {
  expect(trigger, "menu trigger").toBeTruthy();
  await act(async () => { trigger!.focus(); trigger!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
  const item = [...document.querySelectorAll<HTMLElement>("[role=\"menuitem\"]")].find((entry) => entry.textContent?.startsWith(label));
  expect(item, label).toBeTruthy(); return item!;
}

it("locks the model draft and close action until a rejected save restores editing", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
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
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
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
  await act(async () => button("Sync").click());
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic rejection");
  expect(service.manifestGet).toHaveBeenCalledTimes(2);
  let resolve!: (value: unknown) => void;
  service.manifestGet.mockImplementation(() => new Promise((done) => { resolve = done; }));
  await act(async () => { button("Refresh").click(); button("Refresh").click(); });
  expect(service.manifestGet).toHaveBeenCalledTimes(3);
  expect(button("Sync").disabled).toBe(true);
  await act(async () => resolve([status]));
  expect(button("Sync").disabled).toBe(false);
});

it("allows typing unfinished custom header rows without silently persisting the old map", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  snapshot.connections = [];
  snapshot.providers[0].service_entries[0].protocol_routes = [
    { protocol_profile_id: "openai-responses.v1", endpoint_path: "/responses", verified: true },
    { protocol_profile_id: "openai-chat-completions.v1", endpoint_path: "/chat/completions", verified: true },
  ];
  service.load.mockResolvedValue(snapshot);
  await render(<ModelServicesArea />);
  const edit = await menuItem(document.querySelector<HTMLElement>('button[aria-label="More actions"]'), "Edit ");
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
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
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

it("marks an unstable configured provider in the rail and workspace", async () => {
  // Use a saved account to exercise the configured provider row and its selected workspace.
  // 使用已保存账号覆盖已配置供应商行及其选中的工作区。
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  const provider = snapshot.providers.find((item) =>
    snapshot.connections.some((connection) => connection.provider_id === item.id)
  )!;
  provider.credential_directory_visibility = "existing_accounts_only";
  snapshot.providers = [provider];
  service.load.mockResolvedValue(snapshot);
  await render(<ModelServicesArea />);
  // Compare against the catalog entry so copy edits can evolve independently of the behavior.
  // 通过目录项核对标记，让文案可独立于行为调整。
  const warning = readLocaleMessage("en-US", "settings.modelServices.providerUnstable");
  expect(host.querySelector('.vk-ms-head-title [data-variant="warning"]')?.textContent).toBe(warning);
  expect(host.querySelector('.vk-ms-rail-name [role="img"]')?.getAttribute("aria-label")).toBe(warning);
});

it("opens account editing and model management from the icon-only footer shortcuts", async () => {
  service.load.mockResolvedValue(publicAiSettingsWithInvalidDefaultFixture());
  await render(<ModelServicesArea />);
  // The visible shortcuts reuse catalog labels for keyboard access and tooltips.
  // 可见的快捷按钮复用目录文案作为键盘访问名称与提示。
  const editLabel = readLocaleMessage("en-US", "settings.credentials.edit");
  const manageLabel = readLocaleMessage("en-US", "settings.credentialModels.manage");
  // Both icon controls belong to the same rendered account footer.
  // 两个图标控件属于同一张已渲染账号卡片的底部。
  const edit = button(editLabel);
  const manage = button(manageLabel);
  expect(edit.closest(".vk-ms-account-foot")).not.toBeNull();
  expect(manage.closest(".vk-ms-account-foot")).toBe(edit.closest(".vk-ms-account-foot"));
  expect(edit.textContent).toBe("");
  expect(manage.textContent).toBe("");
  vi.useFakeTimers();
  try {
    await act(async () => {
      edit.dispatchEvent(new MouseEvent("pointermove", { bubbles: true }));
      await vi.runOnlyPendingTimersAsync();
    });
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe(editLabel);
  } finally {
    vi.useRealTimers();
  }
  await act(async () => edit.click());
  expect(document.querySelector('[role="dialog"] [data-slot="dialog-title"]')?.textContent).toBe(
    readLocaleMessage("en-US", "settings.credentials.editModelTitle"),
  );
  await act(async () => document.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]')!.click());
  await act(async () => manage.focus());
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe(manageLabel);
  await act(async () => manage.click());
  expect(document.querySelector('[role="dialog"] [data-slot="dialog-title"]')?.textContent).toContain(
    readLocaleMessage("en-US", "settings.credentialModels.title"),
  );
});

it("keeps a deleting account dialog open and editable after a rejected deletion", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  service.load.mockResolvedValue(snapshot);
  let reject!: (error: Error) => void;
  service.connectionDelete.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  await render(<ModelServicesArea />);
  const remove = await menuItem(document.querySelector<HTMLElement>('button[aria-label^="Actions for "]'), readLocaleMessage("en-US", "settings.credentials.remove"));
  expect(document.querySelectorAll('[role="menuitem"]')).toHaveLength(1);
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
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
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

it("locks reordering and account creation during a priority write and reloads after a failure", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  service.load.mockResolvedValue(snapshot);
  let reject!: (error: Error) => void;
  service.prioritySave.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
  await render(<ModelServicesArea />);
  const moves = [...document.querySelectorAll<HTMLButtonElement>('button[aria-label^="Move "]')];
  expect(moves.length).toBeGreaterThan(1);
  const down = moves.find((entry) => entry.getAttribute("aria-label")?.endsWith(" down") && !entry.disabled)!;
  expect(down).toBeTruthy();
  await act(async () => { down.click(); down.click(); });
  // Moving the first configured provider down writes the new top provider first; the second write waits.
  // 将第一个已配置供应商下移时先写入新的首位供应商；第二次写入等待前一次完成。
  expect(service.prioritySave).toHaveBeenCalledExactlyOnceWith("custom:audit-2", 2);
  expect(moves.every((entry) => entry.disabled)).toBe(true);
  const add = button("Add credential");
  expect(add.disabled).toBe(true);
  const loads = service.load.mock.calls.length;
  await act(async () => reject(new Error("Synthetic priority write failed")));
  expect(service.load.mock.calls.length).toBe(loads + 1);
  expect(add.disabled).toBe(false);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Synthetic priority write failed");
});

// Existing Codex records explain the migration, while the plan is the new selectable entry.
// 已有 Codex 记录说明迁移方式，套餐作为可选择的新入口。
it("shows the retirement notice and prevents adding a legacy Codex connection", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  const [provider] = snapshot.providers;
  const [connection] = snapshot.connections;
  const [entry] = provider.service_entries;
  provider.id = "openai";
  provider.display_name = "OpenAI";
  provider.service_entries = [
    { ...entry, id: "codex-subscription", display_name: "Legacy Codex" },
    { ...entry, id: "chatgpt-plan", display_name: "ChatGPT Plan" },
  ];
  connection.provider_id = "openai";
  connection.service_entry_id = "codex-subscription";
  connection.enabled = false;
  snapshot.providers = [provider];
  snapshot.connections = [connection];
  service.load.mockResolvedValue(snapshot);
  await render(<ModelServicesArea />);
  const account = document.querySelector(".vk-ms-account")!;
  expect(account.querySelector('[role="status"]')?.textContent)
    .toBe(readLocaleMessage("en-US", "settings.credentials.retiredCodexMigration"));
  expect(account.querySelector('[role="switch"]')).toBeNull();
  const connect = vi.fn();
  await render(<ProviderConnectPanel busy={false} onConnect={connect} provider={provider} />);
  expect(document.querySelectorAll('[role="radio"]')).toHaveLength(1);
  await act(async () => button(readLocaleMessage("en-US", "settings.modelServices.connectPanel.connect")).click());
  expect(connect).toHaveBeenCalledExactlyOnceWith("chatgpt-plan");
});

// Locates a checkbox by its visible credential name or group action.
// 按可见凭据名或分组操作定位复选框。
function archiveCheckbox(label: string): HTMLButtonElement {
  const checkbox = [...document.querySelectorAll<HTMLButtonElement>('button[role="checkbox"]')]
    .find((entry) => entry.closest("label")?.textContent?.startsWith(label));
  expect(checkbox, label).toBeTruthy();
  return checkbox!;
}

// Locates a migration password field through the current locale catalog.
// 通过当前语言目录定位配置迁移密码输入框。
function archivePassword(key: "password" | "confirmPassword"): HTMLInputElement {
  const label = readLocaleMessage("en-US", `settings.configurationArchive.${key}`);
  const input = [...document.querySelectorAll<HTMLInputElement>('input[type="password"]')]
    .find((entry) => entry.closest("label")?.textContent === label);
  expect(input, label).toBeTruthy();
  return input!;
}

// Exercises the page entry point and verifies only the single checked ID is exported.
// 通过设置页面入口验证导出仅提交唯一勾选的 ID。
it("exports only the credentials selected on the model services page", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  const chosen = snapshot.connections.find((entry) => entry.id === "audit-account-2")!;
  const exportLabel = readLocaleMessage("en-US", "settings.configurationArchive.export");
  const selectAll = readLocaleMessage("en-US", "settings.misc.selectAll");
  service.load.mockResolvedValue(snapshot);
  service.archiveExport.mockRejectedValueOnce(new Error("Fixture does not write a download"));
  await render(<ModelServicesArea />);
  await act(async () => button(exportLabel).click());
  await type(archivePassword("password"), "fixture password");
  await type(archivePassword("confirmPassword"), "fixture password");
  const submit = document.querySelector<HTMLButtonElement>('[role="dialog"] button[type="submit"]')!;
  await act(async () => archiveCheckbox(selectAll).click());
  expect(submit.disabled).toBe(true);
  await act(async () => archiveCheckbox(chosen.display_name).click());
  await act(async () => submit.click());
  expect(service.archiveExport).toHaveBeenCalledExactlyOnceWith("fixture password", [chosen.id], false);
});

// Reviews an encrypted file, skips credentials already present and imports the confirmed subset.
// 预览加密文件，跳过已存在的凭据并导入已确认的子集。
it("binds the import selection to the previewed file and password", async () => {
  const snapshot = publicAiSettingsWithInvalidDefaultFixture();
  const existing = snapshot.connections.find((entry) => entry.id === "audit-account-1")!;
  const preview = {
    providerCount: 1,
    accounts: [
      { connectionId: existing.id, displayName: "Installed account" },
      { connectionId: "imported-first", displayName: "First available credential" },
      { connectionId: "imported-second", displayName: "Second available credential" },
    ].map((account, index) => ({ ...account, index, providerId: existing.provider_id, serviceEntryId: existing.service_entry_id })),
  };
  const onImported = vi.fn();
  const reviewLabel = readLocaleMessage("en-US", "settings.configurationArchive.review");
  const importLabel = readLocaleMessage("en-US", "settings.configurationArchive.import");
  service.archivePreview.mockResolvedValue(preview);
  service.archiveImport.mockResolvedValue({ snapshot, providerCount: 0, accountCount: 1, signInRequired: [] });
  await render(<ConfigurationArchiveDialog mode="import" settings={snapshot} onClose={vi.fn()} onImported={onImported} />);
  const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = new File(["synthetic ciphertext"], "fixture.vc_mcfg");
  const encoded = btoa("synthetic ciphertext");
  await act(async () => {
    Object.defineProperty(input, "files", { configurable: true, value: [file] });
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await type(archivePassword("password"), "first password");
  await act(async () => {
    button(reviewLabel).click();
    await vi.waitFor(() => expect(service.archivePreview).toHaveBeenCalledExactlyOnceWith(encoded, "first password"));
  });
  expect(archiveCheckbox("Installed account").disabled).toBe(true);
  expect(service.archiveImport).not.toHaveBeenCalled();
  await type(archivePassword("password"), "changed password");
  expect(document.querySelector('[role="dialog"] fieldset')).toBeNull();
  await act(async () => {
    button(reviewLabel).click();
    await vi.waitFor(() => expect(service.archivePreview).toHaveBeenLastCalledWith(encoded, "changed password"));
  });
  const omitted = preview.accounts.find((account) => account.connectionId === "imported-first")!;
  const selected = preview.accounts.find((account) => account.connectionId === "imported-second")!;
  await act(async () => archiveCheckbox(omitted.displayName).click());
  await act(async () => button(importLabel).click());
  expect(service.archiveImport).toHaveBeenCalledExactlyOnceWith(encoded, "changed password", [selected.index], false);
  expect(onImported).toHaveBeenCalledExactlyOnceWith(snapshot);
  expect(document.querySelector('input[type="password"]')).toBeNull();
});
