import type { PluginUiModuleSource } from "@/types";
import { invalidatePluginQueries } from "@/service/plugin-ui-query-runtime";
import { BROWSER_SERVICE_CONNECTION_EVENT, type BrowserServiceConnectionDetail } from "@/service/browser-service-discovery";

// PluginUiModuleLoader reads one authenticated module source from the service boundary.
// PluginUiModuleLoader 从服务边界读取一个经过认证的模块源码。
export type PluginUiModuleLoader = (pluginId: string, mountId: string, signal: AbortSignal) => Promise<PluginUiModuleSource>;

// LoadedPluginUiModule retains authenticated metadata and the imported ESM namespace.
// LoadedPluginUiModule 保留认证元数据与已导入的 ESM 命名空间。
export interface LoadedPluginUiModule {
  // Authenticated source metadata returned by the service.
  // 服务返回的已认证源码元数据。
  source: PluginUiModuleSource;
  // Imported namespace created from the untouched module source.
  // 从原样模块源码创建的已导入命名空间。
  namespace: Record<string, unknown>;
}

// PluginUiModuleCacheEntry owns one enabled-plugin module promise and its revocable URL.
// PluginUiModuleCacheEntry 持有单个已启用插件模块 Promise 及其可撤销 URL。
interface PluginUiModuleCacheEntry {
  // The catalog projection invalidates the cache; authenticated source freezes execution identity.
  // 目录投影负责缓存失效；已认证源码冻结执行身份。
  catalogIdentity: string;
  // Retirement stops startup waits and module HTTP reads, never submitted business mutations.
  // 退役停止启动等待和模块 HTTP 读取，绝不停止已提交业务变更。
  cancellation: AbortController;
  // Whether plugin disable or uninstall invalidated this exact entry.
  // 插件停用或卸载是否已使当前精确条目失效。
  invalidated: boolean;
  // In-flight or completed authenticated module import.
  // 正在执行或已完成的认证模块导入。
  promise: Promise<LoadedPluginUiModule>;
  // Revocable object URL retained for the whole enabled-plugin lifetime.
  // 在插件整个启用期内保留的可撤销对象 URL。
  moduleUrl: string | null;
}

// pluginUiModuleCache keeps enabled plugin modules loaded across application navigation.
// pluginUiModuleCache 让已启用插件模块在应用导航期间保持加载。
const pluginUiModuleCache = new Map<string, PluginUiModuleCacheEntry>();

// Reconnecting drops service-bound module identity before any page may reuse old queries.
// 重连时先清除服务绑定模块身份，避免页面复用旧查询。
if (typeof window !== "undefined") window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, event => {
  const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
  if (detail.status === "connected") return;
  for (const key of [...pluginUiModuleCache.keys()]) invalidatePersistentPluginUiModules(key.split("\u0000")[0]);
});

// pluginUiModuleCacheKey creates one collision-free key for an exact plugin mount.
// pluginUiModuleCacheKey 为精确插件挂载点创建无冲突键。
function pluginUiModuleCacheKey(pluginId: string, mountId: string): string {
  return `${pluginId}\u0000${mountId}`;
}

// loadPersistentPluginUiModule loads one authenticated ESM module once per enabled plugin mount.
// loadPersistentPluginUiModule 在每个已启用插件挂载点上仅加载一次认证 ESM 模块。
//
// Parameters: pluginId and mountId select the manifest contribution; loader reads authenticated source.
// 参数：pluginId 与 mountId 选择清单贡献；loader 读取认证源码。
// Returns: a shared promise containing source metadata and the imported namespace.
// 返回值：包含源码元数据与已导入命名空间的共享 Promise。
export function loadPersistentPluginUiModule(
  pluginId: string,
  mountId: string,
  runtimeIdentity: string,
  loader: PluginUiModuleLoader,
): Promise<LoadedPluginUiModule> {
  // Reject incomplete catalog projections explicitly instead of treating absence as an identity.
  // 明确拒绝不完整目录投影，不把缺失值作为运行实例身份。
  if (typeof runtimeIdentity !== "string" || runtimeIdentity.length === 0) {
    return Promise.reject(new Error("Plugin catalog has no runtime identity; refresh the plugin catalog."));
  }
  // key is the exact cache identity for this plugin contribution.
  // key 是当前插件贡献的精确缓存身份。
  const key = pluginUiModuleCacheKey(pluginId, mountId);
  // cached reuses both in-flight and completed imports during the enabled lifetime.
  // cached 在启用生命周期内复用进行中及已完成的导入。
  const cached = pluginUiModuleCache.get(key);
  if (cached && cached.catalogIdentity === runtimeIdentity) {
    return cached.promise;
  }
  if (cached) invalidatePersistentPluginUiModules(pluginId);
  // entry is assigned synchronously before the loader promise can enter its continuation.
  // entry 会在加载 Promise 进入后续回调前同步完成赋值。
  let entry: PluginUiModuleCacheEntry;
  // promise owns the authenticated read, import, and failed-entry eviction flow.
  // promise 持有认证读取、导入与失败条目驱逐流程。
  const cancellation = new AbortController();
  const promise = loader(pluginId, mountId, cancellation.signal)
    .then(async (source) => {
      // A sidebar catalog can lag a service restart. Only the freshly authenticated module
      // freezes the action identity, and the service revalidates it before every action.
      // 侧栏目录可能落后于服务重启；只有重新认证的模块冻结 Action 身份，
      // 服务在每次 Action 前再次校验该身份。
      if (!source.runtimeIdentity) throw new Error("Authenticated plugin module has no runtime identity.");
      if (entry.invalidated) {
        throw new Error(`Plugin UI module \`${pluginId}/${mountId}\` was disabled while loading.`);
      }
      entry.moduleUrl = URL.createObjectURL(new Blob([source.source], { type: "text/javascript" }));
      const namespace = await import(/* @vite-ignore */ entry.moduleUrl) as Record<string, unknown>;
      if (entry.invalidated) {
        throw new Error(`Plugin UI module \`${pluginId}/${mountId}\` was disabled while importing.`);
      }
      return { namespace, source };
    })
    .catch((reason: unknown) => {
      if (pluginUiModuleCache.get(key) === entry) {
        pluginUiModuleCache.delete(key);
      }
      revokePluginUiModuleUrl(entry);
      throw reason;
    });
  entry = {
    catalogIdentity: runtimeIdentity,
    cancellation,
    invalidated: false,
    moduleUrl: null,
    promise,
  };
  pluginUiModuleCache.set(key, entry);
  return promise;
}

// invalidatePersistentPluginUiModules unloads every cached mount owned by one disabled plugin.
// invalidatePersistentPluginUiModules 卸载属于单个已停用插件的全部缓存挂载模块。
//
// Parameters: pluginId identifies the plugin whose enabled lifetime has ended.
// 参数：pluginId 标识启用生命周期已经结束的插件。
// Returns: no value.
// 返回值：无。
export function invalidatePersistentPluginUiModules(pluginId: string): void {
  invalidatePluginQueries(pluginId);
  // keyPrefix selects only mounts owned by the exact plugin identity.
  // keyPrefix 仅选择精确插件身份持有的挂载点。
  const keyPrefix = `${pluginId}\u0000`;
  for (const [key, entry] of pluginUiModuleCache) {
    if (!key.startsWith(keyPrefix)) {
      continue;
    }
    entry.invalidated = true;
    entry.cancellation.abort();
    pluginUiModuleCache.delete(key);
    revokePluginUiModuleUrl(entry);
  }
}

// revokePluginUiModuleUrl releases one object URL exactly once.
// revokePluginUiModuleUrl 仅释放一次对象 URL。
//
// Parameters: entry owns the optional URL.
// 参数：entry 持有可选 URL。
// Returns: no value.
// 返回值：无。
function revokePluginUiModuleUrl(entry: PluginUiModuleCacheEntry): void {
  if (entry.moduleUrl === null) {
    return;
  }
  URL.revokeObjectURL(entry.moduleUrl);
  entry.moduleUrl = null;
}
