import { ensureLocaleCatalog } from "@/i18n-catalogs";
// General fixtures read English and Chinese synchronously; dedicated locale tests load their own extra catalogs to avoid eleven large imports per isolated file.
// 普通行为夹具同步读取中英；其他语言由使用它们的专项测试加载，避免每个隔离测试文件都导入十一份大目录。
await Promise.all([ensureLocaleCatalog("en-US"), ensureLocaleCatalog("zh-CN")]);
