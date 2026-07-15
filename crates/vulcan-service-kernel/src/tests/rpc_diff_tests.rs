//! rpc diff tests.
//! rpc diff tests。

use super::*;

/// Ensures every frontend business RPC method has a kernel dispatcher arm.
/// 确保每个前端业务 RPC 方法都有服务内核分发分支。
#[test]
fn frontend_service_rpc_methods_are_dispatched_by_kernel() {
    let frontend_sources = [
        include_str!(
            "../../../../apps/vulcan-code-web/src/service/conversation-actions-service.ts"
        ),
        include_str!(
            "../../../../apps/vulcan-code-web/src/service/conversation-tool-detail-service.ts"
        ),
        include_str!("../../../../apps/vulcan-code-web/src/service/memory-actions-service.ts"),
        include_str!("../../../../apps/vulcan-code-web/src/service/project-actions-service.ts"),
        include_str!("../../../../apps/vulcan-code-web/src/service/settings-actions-service.ts"),
        include_str!("../../../../apps/vulcan-code-web/src/service/skills-actions-service.ts"),
        include_str!("../../../../apps/vulcan-code-web/src/service/workbench-snapshot-service.ts"),
    ];
    let methods = frontend_service_rpc_methods(&frontend_sources);
    assert!(
        !methods.is_empty(),
        "frontend service RPC method extraction should not be empty"
    );
    for method in methods {
        assert!(
            crate::rpc_dispatch::service_rpc_method_names().contains(method.as_str()),
            "frontend RPC method `{method}` is not present in kernel dispatcher"
        );
    }
}

/// Extracts literal serviceClient.request method names from frontend source.
/// 从前端源码中提取字面量 serviceClient.request 方法名。
fn frontend_service_rpc_methods(sources: &[&str]) -> BTreeSet<String> {
    let mut methods = BTreeSet::new();
    for source in sources {
        let mut remaining = *source;
        while let Some(index) = remaining.find("serviceClient.request") {
            remaining = &remaining[index + "serviceClient.request".len()..];
            if let Some(method) = first_double_quoted_literal(remaining) {
                methods.insert(method);
            }
        }
    }
    methods
}

/// Returns the first double-quoted literal in one source fragment.
/// 返回单个源码片段中的第一个双引号字面量。
fn first_double_quoted_literal(source: &str) -> Option<String> {
    let start = source.find('"')?;
    let rest = &source[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Verifies backend diff summary scanning handles CRLF and mode tokens without line arrays.
/// 验证后端 diff 摘要扫描无需行数组即可处理 CRLF 与 mode token。
#[test]
fn webui_diff_summary_scanner_handles_crlf_and_mode_tokens() {
    let unified_diff = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\r\n",
        "old mode 100644\r\n",
        "new mode 100755\r\n",
        "index 1111111..2222222 100755\r\n",
        "--- a/src/lib.rs\r\n",
        "+++ b/src/lib.rs\r\n",
        "@@ -1,2 +1,2 @@\r\n",
        "-old value\r\n",
        "+new value\r\n",
        " context value\r\n",
    );
    let summaries = webui_unified_diff_file_summaries(unified_diff);
    let stats = webui_unified_diff_stats(unified_diff);

    assert_eq!(webui_unified_diff_line_count(unified_diff), 10);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].file_mode.as_deref(), Some("100755"));
    assert_eq!(summaries[0].row_count, 4);
    assert_eq!(summaries[0].hunk_count, 1);
    assert_eq!(stats.files, 1);
    assert_eq!(stats.additions, 1);
    assert_eq!(stats.deletions, 1);
}

/// Verifies Git quoted UTF-8 path escapes are decoded for WebUI file summaries.
/// 验证 WebUI 文件摘要会解码 Git 引号内的 UTF-8 路径转义。
#[test]
fn webui_diff_summary_scanner_decodes_git_quoted_utf8_paths() {
    let unified_diff = concat!(
        "diff --git \"a/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\" \"b/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
        "similarity index 80%\n",
        "rename from \"\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\"\n",
        "rename to \"\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
        "--- \"a/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\"\n",
        "+++ \"b/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
        "@@ -1 +1 @@\n",
        "-old value\n",
        "+new value\n",
    );
    let summaries = webui_unified_diff_file_summaries(unified_diff);

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].old_path, "中文 目录/old.txt");
    assert_eq!(summaries[0].new_path, "中文 目录/new.txt");
    assert_eq!(summaries[0].status, "renamed");
}

/// Verifies the backend WebUI diff service paths work against a real temporary Git repository.
/// 验证后端 WebUI diff 服务路径可在真实临时 Git 仓库中工作。
#[test]
fn webui_diff_service_paths_cover_real_git_modes() {
    let repository = WebuiDiffTestRepository::new();
    write_webui_diff_test_file(
        repository.path(),
        "src/lib.rs",
        concat!(
            "pub fn marker() -> &'static str {\n",
            "    \"main\"\n",
            "}\n",
            "\n",
            "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
            "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
            "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
            "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
            "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
            "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
            "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
            "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
            "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
            "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
            "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
            "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
            "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
            "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
            "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
            "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
            "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
            "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
            "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
            "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
            "\n",
            "pub fn distant() -> &'static str {\n",
            "    \"stable\"\n",
            "}\n"
        ),
    );
    run_webui_diff_test_git(repository.path(), &["init"]);
    run_webui_diff_test_git(
        repository.path(),
        &["config", "user.email", "webui@example.test"],
    );
    run_webui_diff_test_git(
        repository.path(),
        &["config", "user.name", "WebUI Diff Test"],
    );
    run_webui_diff_test_git(repository.path(), &["add", "src/lib.rs"]);
    run_webui_diff_test_git(repository.path(), &["commit", "-m", "initial"]);
    run_webui_diff_test_git(repository.path(), &["branch", "-M", "main"]);
    run_webui_diff_test_git(repository.path(), &["checkout", "-b", "feature/webui-diff"]);
    write_webui_diff_test_file(
        repository.path(),
        "src/lib.rs",
        concat!(
            "pub fn marker() -> &'static str {\n",
            "    \"branch target\"\n",
            "}\n",
            "\n",
            "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
            "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
            "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
            "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
            "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
            "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
            "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
            "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
            "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
            "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
            "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
            "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
            "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
            "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
            "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
            "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
            "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
            "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
            "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
            "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
            "\n",
            "pub fn distant() -> &'static str {\n",
            "    \"stable\"\n",
            "}\n"
        ),
    );
    run_webui_diff_test_git(repository.path(), &["add", "src/lib.rs"]);
    run_webui_diff_test_git(
        repository.path(),
        &["commit", "-m", "feature branch marker"],
    );
    let feature_commit = webui_diff_test_git_stdout(repository.path(), &["rev-parse", "HEAD"]);
    write_webui_diff_test_file(
        repository.path(),
        "src/lib.rs",
        concat!(
            "pub fn marker() -> &'static str {\n",
            "    \"unstaged top marker\"\n",
            "}\n",
            "\n",
            "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
            "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
            "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
            "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
            "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
            "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
            "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
            "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
            "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
            "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
            "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
            "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
            "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
            "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
            "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
            "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
            "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
            "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
            "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
            "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
            "\n",
            "pub fn distant() -> &'static str {\n",
            "    \"unstaged marker\"\n",
            "}\n"
        ),
    );
    write_webui_diff_test_file(
        repository.path(),
        "src/staged.rs",
        "pub const STAGED_MARKER: &str = \"staged marker\";\n",
    );
    run_webui_diff_test_git(repository.path(), &["add", "src/staged.rs"]);

    let repository_text = repository.path().to_string_lossy().to_string();
    let unstaged = load_webui_workspace_diff_snapshot(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Unstaged,
            base_branch: None,
            compare_branch: None,
            commit_hash: None,
        }),
    )
    .expect("unstaged snapshot should load");
    let staged = load_webui_workspace_diff_snapshot(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Staged,
            base_branch: None,
            compare_branch: None,
            commit_hash: None,
        }),
    )
    .expect("staged snapshot should load");
    let commit = load_webui_workspace_diff_snapshot(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Commit,
            base_branch: None,
            compare_branch: None,
            commit_hash: Some(feature_commit.clone()),
        }),
    )
    .expect("commit snapshot should load");
    let branch = load_webui_workspace_diff_snapshot(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Branch,
            base_branch: Some(String::from("feature/webui-diff")),
            compare_branch: Some(String::from("main")),
            commit_hash: None,
        }),
    )
    .expect("branch snapshot should load");

    assert!(unstaged.unified_diff.contains("unstaged marker"));
    assert!(unstaged.unified_diff.contains("unstaged top marker"));
    assert!(staged.unified_diff.contains("staged marker"));
    assert!(commit.unified_diff.contains("branch target"));
    assert!(branch.unified_diff.contains("branch target"));
    assert_eq!(staged.files.len(), 1);
    assert_eq!(staged.files[0].new_path, "src/staged.rs");
    assert_eq!(staged.files[0].old_file_line_count, Some(0));
    assert_eq!(staged.files[0].new_file_line_count, Some(1));

    let detail = load_webui_workspace_diff_file_detail(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Staged,
            base_branch: None,
            compare_branch: None,
            commit_hash: None,
        }),
        WebuiWorkspaceDiffFileDetailRequest {
            key: staged.files[0].key.clone(),
            old_path: staged.files[0].old_path.clone(),
            new_path: staged.files[0].new_path.clone(),
            context_lines: Some(WEBUI_DIFF_DEFAULT_CONTEXT_LINES),
            hunk_index: None,
            hunk_expansion_direction: None,
        },
    )
    .expect("staged file detail should load");
    assert!(detail.unified_diff.contains("staged marker"));
    assert_eq!(
        detail.file.as_ref().map(|file| file.new_path.as_str()),
        Some("src/staged.rs")
    );

    let expanded_hunk_detail = load_webui_workspace_diff_file_detail(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Unstaged,
            base_branch: None,
            compare_branch: None,
            commit_hash: None,
        }),
        WebuiWorkspaceDiffFileDetailRequest {
            key: unstaged.files[0].key.clone(),
            old_path: unstaged.files[0].old_path.clone(),
            new_path: unstaged.files[0].new_path.clone(),
            context_lines: Some(8),
            hunk_index: Some(0),
            hunk_expansion_direction: Some(String::from("down")),
        },
    )
    .expect("unstaged hunk-scoped file detail should load");
    assert_eq!(expanded_hunk_detail.hunk_index, Some(0));
    assert_eq!(
        expanded_hunk_detail.hunk_expansion_direction.as_deref(),
        Some("down")
    );
    assert_eq!(expanded_hunk_detail.context_lines, 8);
    assert!(
        expanded_hunk_detail
            .unified_diff
            .contains("unstaged top marker")
    );
    assert!(
        !expanded_hunk_detail
            .unified_diff
            .contains("unstaged marker")
    );
    assert_eq!(
        expanded_hunk_detail
            .file
            .as_ref()
            .map(|file| file.hunk_count),
        Some(1)
    );

    let search = load_webui_workspace_diff_search(
        &repository_text,
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Branch,
            base_branch: Some(String::from("feature/webui-diff")),
            compare_branch: Some(String::from("main")),
            commit_hash: None,
        }),
        WebuiWorkspaceDiffSearchRequest {
            query: String::from("branch target"),
            limit: 10,
        },
    )
    .expect("branch search should load");
    assert_eq!(search.matches.len(), 1);
    assert_eq!(search.matches[0].new_path, "src/lib.rs");
    assert!(search.matches[0].first_preview.contains("branch target"));
}

/// Verifies the WebUI Git command layer preserves UTF-8 paths from real Git output.
/// 验证 WebUI Git 命令层会保留真实 Git 输出中的 UTF-8 路径。
#[test]
fn webui_diff_service_preserves_utf8_paths_from_git_output() {
    let repository = WebuiDiffTestRepository::new();
    run_webui_diff_test_git(repository.path(), &["init"]);
    run_webui_diff_test_git(
        repository.path(),
        &["config", "user.email", "webui@example.test"],
    );
    run_webui_diff_test_git(
        repository.path(),
        &["config", "user.name", "WebUI Diff Test"],
    );
    write_webui_diff_test_file(
        repository.path(),
        "中文目录/中文文件.txt",
        "initial value\n",
    );
    run_webui_diff_test_git(repository.path(), &["add", "中文目录/中文文件.txt"]);
    run_webui_diff_test_git(repository.path(), &["commit", "-m", "initial utf8 path"]);
    write_webui_diff_test_file(
        repository.path(),
        "中文目录/中文文件.txt",
        "updated value\n",
    );

    let snapshot = load_webui_workspace_diff_snapshot(
        &repository.path().to_string_lossy(),
        Some(WebuiWorkspaceDiffRequest {
            mode: WebuiWorkspaceDiffMode::Unstaged,
            base_branch: None,
            compare_branch: None,
            commit_hash: None,
        }),
    )
    .expect("utf8 path snapshot should load");

    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].new_path, "中文目录/中文文件.txt");
    assert!(
        snapshot
            .unified_diff
            .contains("diff --git a/中文目录/中文文件.txt b/中文目录/中文文件.txt")
    );
}

/// Verifies hunk-scoped detail keeps only the expanded target hunk.
/// 验证 hunk 级详情裁剪只保留目标扩展 hunk。
#[test]
fn webui_diff_hunk_scoped_unified_diff_keeps_only_target_hunk() {
    let base_unified_diff = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "index 1111111..2222222 100644\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -10,2 +10,2 @@\n",
        " context\n",
        "-old target\n",
        "+new target\n",
        "@@ -30,1 +30,1 @@\n",
        "-old other\n",
        "+new other\n",
    );
    let expanded_unified_diff = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "index 1111111..2222222 100644\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -5,12 +5,12 @@\n",
        " pre context\n",
        " context\n",
        "-old target\n",
        "+new target\n",
        " post context\n",
        "@@ -28,5 +28,5 @@\n",
        " other pre\n",
        "-old other\n",
        "+new other\n",
        " other post\n",
    );

    let scoped = webui_diff_hunk_scoped_unified_diff(base_unified_diff, expanded_unified_diff, 0)
        .expect("target hunk should be found");

    assert!(scoped.contains("@@ -5,12 +5,12 @@"));
    assert!(scoped.contains("new target"));
    assert!(!scoped.contains("@@ -28,5 +28,5 @@"));
    assert!(!scoped.contains("new other"));
}

/// Temporary Git repository removed when a WebUI diff test finishes.
/// WebUI diff 测试结束后自动移除的临时 Git 仓库。
struct WebuiDiffTestRepository {
    /// Absolute repository path under the system temporary directory.
    /// 系统临时目录下的绝对仓库路径。
    path: PathBuf,
}

impl WebuiDiffTestRepository {
    /// Creates one empty temporary repository directory.
    /// 创建一个空的临时仓库目录。
    fn new() -> Self {
        static NEXT_WEBUI_DIFF_TEST_REPOSITORY_ID: AtomicU64 = AtomicU64::new(1);
        let repository_id = NEXT_WEBUI_DIFF_TEST_REPOSITORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "vulcan-webui-diff-test-{}-{}-{}",
            std::process::id(),
            current_unix_millis_value(),
            repository_id
        ));
        fs::create_dir_all(&path).expect("temporary WebUI diff repository should be created");
        Self { path }
    }

    /// Returns the temporary repository path.
    /// 返回临时仓库路径。
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WebuiDiffTestRepository {
    /// Removes the temporary repository directory.
    /// 移除临时仓库目录。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Writes one file inside the temporary WebUI diff repository.
/// 在临时 WebUI diff 仓库中写入单个文件。
fn write_webui_diff_test_file(repository_root: &Path, relative_path: &str, body: &str) {
    let path = repository_root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("test file parent should be created");
    }
    fs::write(path, body).expect("test file should be written");
}

/// Runs one Git command for a temporary WebUI diff repository and requires success.
/// 为临时 WebUI diff 仓库运行单个 Git 命令并要求成功。
fn run_webui_diff_test_git(repository_root: &Path, args: &[&str]) {
    let output = run_webui_git_command(repository_root, args).expect("test git command should run");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs one Git command for a temporary WebUI diff repository and returns stdout.
/// 为临时 WebUI diff 仓库运行单个 Git 命令并返回 stdout。
fn webui_diff_test_git_stdout(repository_root: &Path, args: &[&str]) -> String {
    let output = run_webui_git_command(repository_root, args).expect("test git command should run");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
