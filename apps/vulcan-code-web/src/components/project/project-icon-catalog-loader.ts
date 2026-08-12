import type { ProjectIconOption, ProjectIconPack } from "./project-icon-model";

// ProjectIconCatalog exposes one deferred icon source through a stable contract.
// ProjectIconCatalog 通过稳定契约暴露一个延迟加载的图标来源。
interface ProjectIconCatalog {
  // PROJECT_ICON_OPTIONS is the ordered picker list.
  // PROJECT_ICON_OPTIONS 是选择器使用的有序列表。
  PROJECT_ICON_OPTIONS: ProjectIconOption[];
  // PROJECT_ICON_OPTION_MAP resolves persisted keys, including declared legacy aliases.
  // PROJECT_ICON_OPTION_MAP 解析持久化图标键，包括明确声明的旧版别名。
  PROJECT_ICON_OPTION_MAP: Map<string, ProjectIconOption>;
}

// loadProjectIconCatalog imports only the icon source selected by the user or persisted project.
// loadProjectIconCatalog 只导入用户选择或项目持久化记录对应的图标来源。
export async function loadProjectIconCatalog(pack: ProjectIconPack): Promise<ProjectIconCatalog> {
  if (pack === "skill-icons") {
    return import("./project-icon-catalog-skill");
  }
  if (pack === "material-icon-theme") {
    return import("./project-icon-catalog-material");
  }
  return import("./project-icon-catalog-lucide");
}

// loadProjectIconOption resolves one persisted icon without loading unrelated catalogs.
// loadProjectIconOption 在不加载无关目录的情况下解析一个持久化图标。
export async function loadProjectIconOption(pack: ProjectIconPack, key: string): Promise<ProjectIconOption | null> {
  const catalog = await loadProjectIconCatalog(pack);
  return catalog.PROJECT_ICON_OPTION_MAP.get(key) ?? null;
}
