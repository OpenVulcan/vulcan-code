import { icons as materialIconThemeIconSet } from "@iconify-json/material-icon-theme";
import { addCollection } from "@iconify/react";
import type { ProjectIconOption } from "./project-icon-model";

addCollection(materialIconThemeIconSet);

// PROJECT_ICON_OPTIONS contains the Material Icon Theme picker entries owned by this deferred chunk.
// PROJECT_ICON_OPTIONS 包含由当前延迟分块独立承载的 Material Icon Theme 选择项。
export const PROJECT_ICON_OPTIONS: ProjectIconOption[] = Object.keys(materialIconThemeIconSet.icons)
  .map((key) => ({
    pack: "material-icon-theme" as const,
    key,
    label: key,
    iconName: `material-icon-theme:${key}`,
  }))
  .sort((left, right) => left.label.localeCompare(right.label));

// PROJECT_ICON_OPTION_MAP resolves persisted Material Icon Theme keys after the catalog is loaded.
// PROJECT_ICON_OPTION_MAP 在目录加载后解析持久化的 Material Icon Theme 图标键。
export const PROJECT_ICON_OPTION_MAP = new Map(PROJECT_ICON_OPTIONS.map((option) => [option.key, option]));
