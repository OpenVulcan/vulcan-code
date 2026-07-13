import type { PluginUiModuleSource } from "@/types";

// PluginUiModuleLoader reads one authenticated module source from the service boundary.
// PluginUiModuleLoader 从服务边界读取一个经过认证的模块源码。
export type PluginUiModuleLoader = (pluginId: string, mountId: string) => Promise<PluginUiModuleSource>;

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
  loader: PluginUiModuleLoader,
): Promise<LoadedPluginUiModule> {
  // key is the exact cache identity for this plugin contribution.
  // key 是当前插件贡献的精确缓存身份。
  const key = pluginUiModuleCacheKey(pluginId, mountId);
  // cached reuses both in-flight and completed imports during the enabled lifetime.
  // cached 在启用生命周期内复用进行中及已完成的导入。
  const cached = pluginUiModuleCache.get(key);
  if (cached) {
    return cached.promise;
  }
  // entry is assigned synchronously before the loader promise can enter its continuation.
  // entry 会在加载 Promise 进入后续回调前同步完成赋值。
  let entry: PluginUiModuleCacheEntry;
  // promise owns the authenticated read, import, and failed-entry eviction flow.
  // promise 持有认证读取、导入与失败条目驱逐流程。
  const promise = loader(pluginId, mountId)
    .then(async (source) => {
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
  // keyPrefix selects only mounts owned by the exact plugin identity.
  // keyPrefix 仅选择精确插件身份持有的挂载点。
  const keyPrefix = `${pluginId}\u0000`;
  for (const [key, entry] of pluginUiModuleCache) {
    if (!key.startsWith(keyPrefix)) {
      continue;
    }
    entry.invalidated = true;
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
