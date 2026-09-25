import { ensureLocaleCatalog } from "@/i18n-catalogs";
import { SUPPORTED_LOCALES } from "@/i18n-locales";

// Tests read any language synchronously, so every catalog is downloaded before a test file runs; the app
// itself loads only the active language on demand.
// 测试会同步读取任意语言，因此在测试文件运行前下载全部目录；应用本身只按需加载当前语言。
await Promise.all(SUPPORTED_LOCALES.map((entry) => ensureLocaleCatalog(entry.code)));
