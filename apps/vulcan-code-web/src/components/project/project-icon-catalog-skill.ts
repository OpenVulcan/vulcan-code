import { icons as skillIconSet } from "@iconify-json/skill-icons";
import { addCollection } from "@iconify/react";
import type { ProjectIconOption } from "./project-icon-model";

addCollection(skillIconSet);

// PROJECT_ICON_OPTIONS contains the Skill Icons picker entries owned by this deferred chunk.
// PROJECT_ICON_OPTIONS 包含由当前延迟分块独立承载的 Skill Icons 选择项。
export const PROJECT_ICON_OPTIONS: ProjectIconOption[] = Object.keys(skillIconSet.icons)
  .map((key) => ({
    pack: "skill-icons" as const,
    key,
    label: key,
    iconName: `skill-icons:${key}`,
  }))
  .sort((left, right) => left.label.localeCompare(right.label));

// PROJECT_ICON_OPTION_MAP resolves persisted Skill Icons keys after the catalog is loaded.
// PROJECT_ICON_OPTION_MAP 在目录加载后解析持久化的 Skill Icons 图标键。
export const PROJECT_ICON_OPTION_MAP = new Map(PROJECT_ICON_OPTIONS.map((option) => [option.key, option]));
