//! Permission classification for the LuaSkills packages Vulcan Code installs by default.
//! Vulcan Code 默认安装的 LuaSkills 包的权限分类。
//!
//! These entries are known by their complete canonical names, so the gate can classify them
//! exactly instead of guessing from name keywords. Entries not listed keep the generic rules.
//! 这些入口按完整规范名称已知，闸门可以精确分类，而不是依据名称关键字猜测。未列出的入口沿用通用规则。

use serde_json::{Map, Value};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;

use super::PermissionSubject;
use crate::luaskills_runtime::PWD_PARAMETER_NAME;

/// Built-in entries that only read the workspace; they run without approval in every mode.
/// 仅读取工作区的内置入口；在任何模式下都无需审批即可运行。
pub(crate) const READ_ONLY_ENTRIES: &[&str] = &[
    "vulcan-codekit-repo-map",
    "vulcan-codekit-ast-detail",
    "vulcan-codekit-ast-tree",
    "vulcan-codekit-rg",
    "vulcan-codekit-markdown-menu",
    "vulcan-codekit-node-source",
];

/// CodeKit whole-function patch; writes the files named by `file` and `patches[].file`.
/// CodeKit 整函数补丁；写入 `file` 与 `patches[].file` 指定的文件。
const CODEKIT_PATCH_ENTRY: &str = "vulcan-codekit-patch";

/// curl GET; a network read unless `download_to` saves the body to a local file.
/// curl GET；除非 `download_to` 将响应体保存到本地文件，否则属于网络读取。
const CURL_GET_ENTRY: &str = "vulcan-curl-get";

/// Returns whether `canonical_name` is a built-in entry that only reads.
/// 返回 `canonical_name` 是否为仅读取的内置入口。
pub(crate) fn is_read_only_entry(canonical_name: &str) -> bool {
    READ_ONLY_ENTRIES.contains(&canonical_name)
}

/// Classifies one call of the built-in entry `canonical_name` with object `args`.
/// 对内置入口 `canonical_name` 以对象参数 `args` 的一次调用分类。
/// Returns `None` for entries this module does not know; callers then apply the generic rules.
/// 本模块不认识的入口返回 `None`，调用方随后应用通用规则。
pub(super) fn classify_entry(canonical_name: &str, args: &Value) -> Option<PermissionSubject> {
    if is_read_only_entry(canonical_name) {
        return Some(PermissionSubject::Ungoverned);
    }
    match canonical_name {
        CODEKIT_PATCH_ENTRY => Some(file_write(codekit_patch_paths(args))),
        CURL_GET_ENTRY => Some(match args.get("download_to") {
            None | Some(Value::Null) => PermissionSubject::Ungoverned,
            // curl resolves a relative download path against the service process directory,
            // not the workspace, so only an absolute path can be checked against the workspace.
            // curl 将相对下载路径相对服务进程目录解析，而非工作区，因此只有绝对路径能与工作区比对。
            Some(Value::String(path)) if std::path::Path::new(path).is_absolute() => {
                PermissionSubject::FileWrite {
                    paths: vec![path.clone()],
                }
            }
            Some(_) => PermissionSubject::Opaque,
        }),
        _ => None,
    }
}

/// Classifies one `call_luaskill` dispatch by the entry it targets.
/// 按目标入口对一次 `call_luaskill` 调度分类。
/// Unknown entries use `generic` on the target name; malformed calls are `Opaque`.
/// 未知入口对目标名称使用 `generic`；格式错误的调用归为 `Opaque`。
pub(super) fn classify_dispatch(
    args: &Value,
    generic: impl FnOnce(&str) -> PermissionSubject,
) -> PermissionSubject {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return PermissionSubject::Opaque;
    };
    let Some(target_args) = dispatch_arguments(args) else {
        return PermissionSubject::Opaque;
    };
    classify_entry(name, &target_args).unwrap_or_else(|| generic(name))
}

/// Returns whether `tool_id` is the implicit LuaSkills dispatcher.
/// 返回 `tool_id` 是否为隐式 LuaSkills 调度器。
pub(super) fn is_dispatcher(tool_id: &str) -> bool {
    tool_id == CALL_LUASKILL_TOOL_NAME
}

/// Rebuilds the target entry's argument object from `call_luaskill` name/value text pairs.
/// 根据 `call_luaskill` 的名称/文本值对重建目标入口的参数对象。
/// JSON-looking values are parsed as the host does for array and object parameters.
/// 形如 JSON 的值会像宿主处理数组与对象参数时那样解析。
fn dispatch_arguments(args: &Value) -> Option<Value> {
    let mut object = Map::new();
    let Some(pairs) = args.get("args") else {
        return Some(Value::Object(object));
    };
    for pair in pairs.as_array()? {
        let name = pair.get("name")?.as_str()?;
        let text = pair.get("value")?.as_str()?;
        let value = match text.trim_start().chars().next() {
            Some('[' | '{') => serde_json::from_str(text).unwrap_or_else(|_| Value::from(text)),
            _ => Value::from(text),
        };
        object.insert(name.to_string(), value);
    }
    Some(Value::Object(object))
}

/// Collects CodeKit patch targets from single mode `file` and batch mode `patches[].file`.
/// 从单项模式的 `file` 与批量模式的 `patches[].file` 收集 CodeKit 补丁目标。
/// Returns `None` when any listed target is not a string.
/// 任一目标不是字符串时返回 `None`。
/// CodeKit resolves relative targets against `PWD`. The host injects the workspace root only
/// when the call omits `PWD`, so relative targets stay workspace-relative in that case; an
/// explicit absolute `PWD` is joined here, and any other explicit `PWD` returns `None`.
/// CodeKit 相对 `PWD` 解析相对目标。宿主仅在调用省略 `PWD` 时注入工作区根目录，此时相对目标保持
/// 相对工作区；显式的绝对 `PWD` 在此拼接，其他显式 `PWD` 返回 `None`。
fn codekit_patch_paths(args: &Value) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    if let Some(file) = args.get("file") {
        paths.push(file.as_str()?.to_string());
    }
    if let Some(patches) = args.get("patches") {
        for patch in patches.as_array()? {
            paths.push(patch.get("file")?.as_str()?.to_string());
        }
    }
    let Some(pwd) = args.get(PWD_PARAMETER_NAME) else {
        return Some(paths);
    };
    let pwd = std::path::Path::new(pwd.as_str()?);
    if !pwd.is_absolute() {
        return None;
    }
    paths
        .into_iter()
        .map(|path| {
            if std::path::Path::new(&path).is_absolute() {
                Some(path)
            } else {
                pwd.join(&path).to_str().map(str::to_string)
            }
        })
        .collect()
}

/// Builds a file-write subject, or `Opaque` when the targets are unknown or empty.
/// 构建文件写入对象；目标未知或为空时返回 `Opaque`。
fn file_write(paths: Option<Vec<String>>) -> PermissionSubject {
    match paths {
        Some(paths) if !paths.is_empty() => PermissionSubject::FileWrite { paths },
        _ => PermissionSubject::Opaque,
    }
}
