// @vitest-environment jsdom
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { load } from "js-yaml";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { SUPPORTED_LOCALES } from "@/i18n-locales";
import { loadPersistentPluginUiModule } from "@/service/plugin-ui-module-runtime";
import { DynamicPluginMountPanel } from "./dynamic-plugin-mount-panel";

// selection supplies the actual host language to the mounted SDK on each rerender.
// selection 在每次重新渲染时向挂载 SDK 提供真实宿主语言。
const selection = vi.hoisted(() => ({ locale: "en-US" }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual retains host message formatting while only the selected language is controlled.
  // actual 保留宿主文案格式化，仅控制选中的语言。
  const actual = await importOriginal();
  // t keeps a stable callback identity while resolving the currently selected locale.
  // t 保持稳定的回调身份，同时解析当前选择的语言。
  const t = (key) => actual.readLocaleMessage(selection.locale, key);
  return { ...actual, useI18n: () => ({
    locale: selection.locale,
    setLocale: () => undefined,
    t,
  }) };
});
vi.mock("@/service/plugin-ui-module-runtime", () => ({
  loadPersistentPluginUiModule: vi.fn(),
  invalidatePersistentPluginUiModules: vi.fn(),
}));

// pluginRoot is the repository source directory, independent of generated output resources.
// pluginRoot 是仓库源码目录，独立于生成的输出资源。
const pluginRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../../../plugins");
// packages are discovered from their authoritative manifests so new bundled plugins are covered too.
// packages 从权威清单发现，使以后新增的内置插件也接受检查。
const packages = readdirSync(pluginRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => {
    // directory and manifest bind every locale read to the same installed package shape.
    // directory 和 manifest 把每份语言资源读取绑定到同一插件包结构。
    const directory = path.join(pluginRoot, entry.name);
    const manifest = load(readFileSync(path.join(directory, "plugin.yaml"), "utf8"));
    return { directory, manifest };
  });

// readCatalog parses one explicitly declared locale with duplicate-key rejection from the YAML parser.
// readCatalog 使用 YAML 解析器的重复键拒绝机制解析一份明确声明的语言资源。
// pkg and locale select an existing package declaration; returns the exact parsed mapping.
// pkg 和 locale 选择既有包声明，返回精确解析的映射。
function readCatalog(pkg, locale) {
  // relative is declared by this manifest, never guessed from a language file name.
  // relative 来自此清单声明，绝不根据语言名称猜测文件路径。
  const relative = pkg.manifest.locales[locale];
  expect(typeof relative, `${pkg.manifest.id}: missing locale ${locale}; update bundled translations when host languages change`).toBe("string");
  // absolute is resolved from the exact declaration and checked against the package boundary.
  // absolute 从精确声明解析，并检查是否位于插件包边界内。
  const absolute = path.resolve(pkg.directory, relative);
  expect(absolute.startsWith(`${pkg.directory}${path.sep}`), "Locale must remain inside its package").toBe(true);
  return load(readFileSync(absolute, "utf8"));
}

// placeholders preserves repeated interpolation names; changing or dropping one breaks the UI contract.
// placeholders 保留重复出现的插值名称，改动或遗漏任何一项都会破坏界面契约。
// message is a catalog string; returns sorted placeholder names including duplicates.
// message 是语言资源字符串，返回包含重复项的已排序占位符名称。
function placeholders(message) {
  return [...message.matchAll(/\{\{([^{}]+)\}\}/gu)].map((match) => match[1]).sort();
}

it("discovers bundled packages before checking language completeness", () => {
  expect(packages.length, "The source plugin directory must contain packages").toBeGreaterThan(0);
});

for (const pkg of packages) {
  it(`${pkg.manifest.id} declares every host language with complete translated messages`, () => {
    // English remains the existing compatibility catalog and exact en-US maps to the same bytes.
    // 英语仍使用既有兼容目录，精确 en-US 映射到同一份字节。
    const english = readCatalog(pkg, "en");
    expect(pkg.manifest.locales["en-US"]).toBe(pkg.manifest.locales.en);
    // englishMessages and keys define the exact optional UI message contract for this package.
    // englishMessages 和 keys 定义此包可选界面消息的精确契约。
    const englishMessages = english.messages ?? {};
    const keys = Object.keys(englishMessages).sort();
    for (const { code } of SUPPORTED_LOCALES) {
      // catalog is one exact locale; fallback must never hide missing bundled translations.
      // catalog 是精确语言资源，回退不得隐藏内置翻译缺失。
      const catalog = readCatalog(pkg, code);
      expect(typeof catalog.name).toBe("string");
      expect(catalog.name.trim()).not.toBe("");
      expect(typeof catalog.capability).toBe("string");
      expect(catalog.capability.trim()).not.toBe("");
      // messages must preserve all keys even when the manifest has no UI contribution.
      // messages 必须保留所有键，包括清单没有界面贡献的情况。
      const messages = catalog.messages ?? {};
      expect(Object.keys(messages).sort(), `${pkg.manifest.id}/${code}: message keys drifted`).toEqual(keys);
      for (const key of keys) {
        expect(typeof messages[key], `${pkg.manifest.id}/${code}/${key}`).toBe("string");
        expect(messages[key].trim(), `${pkg.manifest.id}/${code}/${key}`).not.toBe("");
        expect(placeholders(messages[key]), `${pkg.manifest.id}/${code}/${key}: interpolation changed`).toEqual(placeholders(englishMessages[key]));
      }
      if (code !== "en-US") {
        expect(catalog.capability, `${pkg.manifest.id}/${code}: translate the user-facing description`).not.toBe(english.capability);
        if (keys.length > 0) {
          expect(keys.some((key) => messages[key] !== englishMessages[key]), `${pkg.manifest.id}/${code}: catalog is an English copy`).toBe(true);
        }
      }
    }
  });
}

// Mounted resources are released after each test, including assertion failures.
// 每个测试结束时释放挂载资源，包括断言失败的情况。
let activeRoot;
// activeHost owns the DOM container corresponding to the optional mounted root.
// activeHost 拥有与可选挂载根对应的 DOM 容器。
let activeHost;
afterEach(async () => {
  if (activeRoot) await act(async () => activeRoot.unmount());
  activeHost?.remove();
  activeRoot = undefined;
  activeHost = undefined;
  selection.locale = "en-US";
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

for (const pkg of packages) {
  // Only packages with a declared UI mount participate in the real SDK rendering boundary.
  // 仅明确声明界面挂载点的插件参与真实 SDK 渲染边界验证。
  const mounts = pkg.manifest.ui?.mounts ?? [];
  for (const mount of mounts) {
    it(`${pkg.manifest.id}/${mount.id} switches every host language without an English fallback`, async () => {
      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
      activeHost = document.createElement("div");
      document.body.append(activeHost);
      activeRoot = createRoot(activeHost);
      // Localized module metadata mirrors the authenticated service projection from the real files.
      // 本地化模块元数据使用真实文件，对应认证服务投影。
      const messagesByLocale = Object.fromEntries(Object.keys(pkg.manifest.locales)
        .map((locale) => [locale, readCatalog(pkg, locale).messages ?? {}]));
      // uiMount and plugin provide the exact identities consumed by the host module loader.
      // uiMount 和 plugin 提供宿主模块加载器使用的精确身份。
      const uiMount = { id: mount.id };
      const plugin = { id: pkg.manifest.id, extension: { runtimeIdentity: "locale-test:1", uiMounts: [uiMount] } };
      vi.mocked(loadPersistentPluginUiModule).mockResolvedValue({
        source: { pluginId: pkg.manifest.id, mountId: mount.id, exportName: "create", source: "",
          messagesByLocale, runtimeIdentity: "locale-test:1", actionPolicies: [] },
        namespace: { create: (sdk) => () => React.createElement("span", null,
          `${sdk.locale}:${sdk.t(mount.label_message_key, "MISSING_TRANSLATION")}`) },
      });
      for (const { code } of SUPPORTED_LOCALES) {
        selection.locale = code;
        await act(async () => activeRoot.render(React.createElement(DynamicPluginMountPanel, { plugin, mount: uiMount })));
        await vi.waitFor(async () => {
          await act(async () => undefined);
          expect(activeHost.textContent).toBe(`${code}:${messagesByLocale[code][mount.label_message_key]}`);
        });
      }
    });
  }
}
