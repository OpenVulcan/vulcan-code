//! Plugin process-spec resolution, environment allowlisting, and path containment.
//! 插件进程规格解析、环境变量白名单与路径包含性。

use super::{build_plugin_process_spec, canonical_plugin_directory, validate_plugin_environment};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Verifies plugin process specifications expose standard Windows paths to the Lua bridge.
/// 验证插件进程规格向 Lua 桥暴露标准 Windows 路径。
#[cfg(windows)]
#[test]
fn plugin_process_spec_normalizes_windows_verbatim_paths() {
    let spec = build_plugin_process_spec(
        PathBuf::from(r"\\?\D:\openvulcan\runtime\node.exe"),
        PathBuf::from(r"\\?\D:\openvulcan\plugins\context7"),
        Vec::new(),
        "exec",
        &Map::new(),
        None,
        BTreeMap::new(),
    )
    .expect("build plugin process specification");

    assert_eq!(
        spec.get("program").and_then(Value::as_str),
        Some(r"D:\openvulcan\runtime\node.exe")
    );
    assert_eq!(
        spec.get("cwd").and_then(Value::as_str),
        Some(r"D:\openvulcan\plugins\context7")
    );
}

/// One uniquely named directory that removes itself on drop.
/// 一个唯一命名并在析构时自我删除的目录。
///
/// The crate has no `tempfile` dependency, so uniqueness comes from the process id, a
/// monotonic counter, and the current time, which keeps parallel test threads apart.
/// 该 crate 没有 `tempfile` 依赖，因此唯一性来自进程 ID、单调计数器与当前时间，从而让并行
/// 测试线程互不干扰。
struct TempRoot {
    /// Exact temporary directory owned by this fixture.
    /// 本夹具拥有的精确临时目录。
    path: PathBuf,
}

impl TempRoot {
    /// Creates one empty unique directory beneath the system temporary directory.
    /// 在系统临时目录下创建一个唯一的空目录。
    ///
    /// `label` identifies the scenario in diagnostics and the generated directory name.
    /// `label` 在诊断与生成的目录名中标识测试场景。
    /// Returns the owner of the newly created directory.
    /// 返回新建目录的所有者。
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let unique = format!(
            "vulcan-plugin-process-{label}-{}-{}-{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("create temporary plugin process fixture");
        Self { path }
    }

    /// Returns the fixture root.
    /// 返回夹具根。
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    /// Removes the exact fixture-owned directory without masking an earlier test failure.
    /// 删除夹具拥有的精确目录，且不掩盖先前的测试失败。
    fn drop(&mut self) {
        // Cleanup is best-effort: a failed removal must not mask a test failure.
        // 清理是尽力而为：删除失败不得掩盖测试失败。
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Verifies an undeclared environment variable is refused, not silently dropped.
/// 验证未声明的环境变量被拒绝，而不是被静默丢弃。
///
/// Silently dropping it would let a plugin believe it configured a variable that the
/// child process never received.
/// 静默丢弃会让插件以为它配置了某个变量，而子进程从未收到该变量。
#[test]
fn undeclared_environment_variable_is_refused() {
    let error = validate_plugin_environment(
        &json!({ "DECLARED": "ok", "SECRET": "leak" }),
        &[String::from("DECLARED")],
    )
    .expect_err("an undeclared variable must be refused");
    // The message names the offending variable so the author can fix the declaration.
    // 消息点名冒犯的变量，使作者能够修正声明。
    assert!(error.contains("SECRET"), "{error}");
    assert!(error.contains("not declared"), "{error}");
}

/// Verifies only declared string variables survive validation.
/// 验证只有已声明的字符串变量能通过校验。
#[test]
fn declared_string_environment_variables_are_preserved() {
    let validated = validate_plugin_environment(
        &json!({ "DECLARED": "value", "OTHER": "second" }),
        &[String::from("DECLARED"), String::from("OTHER")],
    )
    .expect("declared string variables are valid");

    assert_eq!(
        validated.get("DECLARED").and_then(Value::as_str),
        Some("value")
    );
    assert_eq!(
        validated.get("OTHER").and_then(Value::as_str),
        Some("second")
    );
    // An absent optional variable stays absent rather than becoming an empty string.
    // 缺失的可选变量保持缺失，而不是变成空字符串。
    assert_eq!(validated.as_object().map(Map::len), Some(2));
}

/// Verifies a non-object environment and non-string values are refused.
/// 验证非对象环境与非字符串取值均被拒绝。
#[test]
fn non_object_environment_and_non_string_values_are_refused() {
    assert!(
        validate_plugin_environment(&json!(["DECLARED"]), &[String::from("DECLARED")]).is_err(),
        "an array is not an environment object"
    );
    let error = validate_plugin_environment(&json!({ "DECLARED": 7 }), &[String::from("DECLARED")])
        .expect_err("a non-string value must be refused");
    assert!(error.contains("DECLARED"), "{error}");
    assert!(error.contains("string"), "{error}");
}

/// Verifies a directory inside the package root resolves.
/// 验证包根内的目录可解析。
#[test]
fn contained_plugin_directory_resolves() {
    let root = TempRoot::new("contained");
    let child = root.path().join("nested");
    std::fs::create_dir_all(&child).expect("create nested fixture directory");

    let resolved = canonical_plugin_directory(root.path(), "nested", "asset root")
        .expect("a contained directory resolves");
    assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
    assert!(resolved.is_dir());
}

/// Verifies a traversal out of the package root is refused.
/// 验证越出包根的路径穿越被拒绝。
///
/// The check runs on the canonicalized path, so `..` cannot escape even when the
/// lexical form still looks contained.
/// 该校验作用于规范化后的路径，因此即使字面形式看似仍在包根内，`..` 也无法逃逸。
#[test]
fn plugin_directory_escape_is_refused() {
    let root = TempRoot::new("escape-root");
    let outside = TempRoot::new("escape-outside");

    // `..` from the root lands in the shared temporary directory, which exists but is
    // not inside the package root.
    // 从包根出发的 `..` 落在共享临时目录，它存在但不在包根内。
    let error = canonical_plugin_directory(root.path(), "..", "asset root")
        .expect_err("a traversal must be refused");
    assert!(error.contains("escapes its package root"), "{error}");

    // A lexical sibling that is genuinely outside is refused for the same reason.
    // 字面上确实在外的兄弟路径因同一原因被拒绝。
    let sibling = format!(
        "../{}",
        outside
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("fixture directory has a name")
    );
    assert!(canonical_plugin_directory(root.path(), &sibling, "asset root").is_err());
}

/// Verifies a regular file is refused where a directory is required.
/// 验证在需要目录之处拒绝普通文件。
#[test]
fn plugin_directory_rejects_a_regular_file() {
    let root = TempRoot::new("regular-file");
    std::fs::write(root.path().join("asset.txt"), b"content").expect("write file fixture");

    let error = canonical_plugin_directory(root.path(), "asset.txt", "asset root")
        .expect_err("a regular file is not a directory");
    assert!(error.contains("not a directory"), "{error}");
}

/// Verifies a missing path and an unresolvable root are distinct failures.
/// 验证缺失路径与不可解析的根是两种不同的失败。
#[test]
fn plugin_directory_reports_missing_paths_separately() {
    let root = TempRoot::new("missing");
    let missing = canonical_plugin_directory(root.path(), "absent", "asset root")
        .expect_err("a missing directory fails");
    // The label distinguishes which resolution step failed, so the message stays
    // actionable instead of being a single generic containment error.
    // label 区分是哪个解析步骤失败，使消息保持可操作，而不是笼统的一条包含性错误。
    assert!(missing.contains("asset root"), "{missing}");

    let absent_root = root.path().join("does-not-exist");
    let root_error = canonical_plugin_directory(&absent_root, "nested", "asset root")
        .expect_err("a missing package root fails");
    assert!(root_error.contains("package root"), "{root_error}");
}
