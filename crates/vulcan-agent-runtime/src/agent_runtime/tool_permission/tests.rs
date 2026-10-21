//! Behaviour of tool classification, deterministic rules, and the permission gate.
//! 工具分类、确定性规则与权限闸门的行为。

use super::command_rules::{high_risk_category, is_known_read_only};
use super::*;

/// Knowledge mutations and host ingest reads bypass approval under their declared access contract.
/// 知识变更及宿主搬运读取按已声明访问契约无需审批。
#[test]
fn knowledge_and_ingest_file_reads_are_ungoverned() {
    for tool in [
        "knowledge_list",
        "knowledge_read",
        "knowledge_search",
        "knowledge_write",
        "file_read",
    ] {
        for args in [
            serde_json::json!({}),
            serde_json::json!({"action":"ingest","sources":["C:/external/file.md"]}),
        ] {
            assert!(matches!(
                classify_native(tool, &args),
                PermissionSubject::Ungoverned
            ));
        }
    }
}
use crate::tooling::native::FILE_READ_TOOL_ID;
use serde_json::json;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolDescriptor, VulcanToolInvocationResult, VulcanToolOrigin,
};

/// Builds one resolved definition with a no-op body for classification tests.
/// 为分类测试构建一个主体为空操作的已解析定义。
fn definition(
    source: VulcanToolSourceKind,
    id: &str,
    plan_policy: VulcanPlanToolPolicy,
) -> ResolvedToolDefinition {
    ResolvedToolDefinition {
        descriptor: VulcanToolDescriptor::new(
            id,
            id,
            "test tool",
            None,
            VulcanToolOrigin::new(source, "test", id),
        ),
        parameters_schema: json!({"type": "object"}),
        plan_policy,
        request_streaming: None,
        invoke: Arc::new(|_, _| {
            Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
        }),
    }
}

/// Native definition shortcut.
/// 原生定义的简写。
fn native(id: &str) -> ResolvedToolDefinition {
    definition(
        VulcanToolSourceKind::Native,
        id,
        VulcanPlanToolPolicy::Hidden,
    )
}

/// Fixed host used to drive the gate: a mode, an approval answer, and the prompts it received.
/// 用于驱动闸门的固定宿主：一个模式、一个审批回答以及它收到的提示。
struct FixedHost {
    mode: Result<PermissionMode, String>,
    approval: Result<bool, String>,
    judge: Result<Option<JudgeLabel>, String>,
    review: Result<Option<LlmReview>, String>,
    prompts: std::sync::Mutex<Vec<(String, ToolApprovalPrompt)>>,
    judged: std::sync::Mutex<Vec<JudgeCall>>,
    reviewed: std::sync::Mutex<Vec<ReviewCall>>,
    records: std::sync::Mutex<Vec<PermissionRecord>>,
    unattended: std::sync::Mutex<Option<String>>,
    /// Lets every approval run out unanswered instead of answering it.
    /// 让每次审批无人回答直至等待时间耗尽，而不是回答它。
    expire: std::sync::Mutex<bool>,
    memory: PermissionMemory,
}

impl FixedHost {
    /// Builds a host with `mode` that answers every approval with `approval`.
    /// 构建一个模式为 `mode`、对每次审批都回答 `approval` 的宿主。
    fn new(mode: Result<PermissionMode, String>, approval: Result<bool, String>) -> Arc<Self> {
        Self::judging(mode, approval, Ok(None))
    }

    /// Builds a host whose judge answers every call with `judge`.
    /// 构建一个判定器对每次调用都回答 `judge` 的宿主。
    fn judging(
        mode: Result<PermissionMode, String>,
        approval: Result<bool, String>,
        judge: Result<Option<JudgeLabel>, String>,
    ) -> Arc<Self> {
        Self::reviewing(mode, approval, judge, Ok(None))
    }

    /// Builds a host whose judge and reviewer answer every call with fixed results.
    /// 构建一个判定器与复核器对每次调用给出固定结果的宿主。
    fn reviewing(
        mode: Result<PermissionMode, String>,
        approval: Result<bool, String>,
        judge: Result<Option<JudgeLabel>, String>,
        review: Result<Option<LlmReview>, String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            mode,
            approval,
            judge,
            review,
            prompts: std::sync::Mutex::new(Vec::new()),
            judged: std::sync::Mutex::new(Vec::new()),
            reviewed: std::sync::Mutex::new(Vec::new()),
            records: std::sync::Mutex::new(Vec::new()),
            unattended: std::sync::Mutex::new(None),
            expire: std::sync::Mutex::new(false),
            memory: PermissionMemory::default(),
        })
    }
}

#[async_trait::async_trait]
impl ToolPermissionHost for FixedHost {
    fn access(&self, _session_id: SessionId) -> Result<AccessConfig, String> {
        Ok(AccessConfig {
            permission: self.mode.clone()?,
            unattended: self.unattended.lock().unwrap().clone().map(|deny_policy| {
                vulcan_session_core::v1::config::UnattendedConfig { deny_policy }
            }),
        })
    }

    fn memory(&self) -> Option<&PermissionMemory> {
        Some(&self.memory)
    }

    async fn request_approval(
        &self,
        _session_id: SessionId,
        _context: VulcanToolInvocationContext,
        tool_name: String,
        _arguments: Value,
        prompt: ToolApprovalPrompt,
    ) -> Result<ApprovalAnswer, String> {
        self.prompts.lock().unwrap().push((tool_name, prompt));
        if *self.expire.lock().unwrap() {
            return Ok(ApprovalAnswer::Expired);
        }
        self.approval.clone().map(|allowed| {
            if allowed {
                ApprovalAnswer::Allowed
            } else {
                ApprovalAnswer::Denied
            }
        })
    }

    async fn judge(&self, call: JudgeCall) -> Result<Option<JudgeVerdict>, JudgeFailure> {
        self.judged.lock().unwrap().push(call);
        self.judge
            .clone()
            .map(|label| {
                label.map(|label| JudgeVerdict {
                    label,
                    trace: trace(),
                })
            })
            .map_err(|reason| JudgeFailure {
                reason,
                trace: None,
            })
    }

    async fn review(&self, call: ReviewCall) -> Result<Option<LlmReview>, String> {
        self.reviewed.lock().unwrap().push(call);
        self.review.clone()
    }

    fn record(&self, record: PermissionRecord) {
        self.records.lock().unwrap().push(record);
    }
}

/// Judge trace the fixed host reports with every verdict.
/// 固定宿主随每个判定一并报告的判定轨迹。
fn trace() -> JudgeTrace {
    JudgeTrace {
        backend: String::from("local"),
        model: String::from("fixed"),
        policy_version: String::from("1"),
        settings: BTreeMap::new(),
        rendered: String::from("Tool: shell_command"),
        logits: Some([1.0, 0.0, 0.0]),
        allow_probability: None,
        input_tokens: Some(8),
    }
}

/// Exact coordinates of one synthetic tool call.
/// 一次合成工具调用的精确坐标。
fn context() -> VulcanToolInvocationContext {
    use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};
    VulcanToolInvocationContext {
        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
        session_id: SessionId::nil().to_string(),
        request_stream_id: RequestStreamId::try_new("gate-stream").unwrap(),
        api_call_id: ApiCallId::try_new(1).unwrap(),
        tool_call_id: ToolCallId::try_new(1).unwrap(),
        tool_seq: Seq::try_new(3).unwrap(),
        controller_tool_call_id: String::from("gate-call"),
        provider_tool_call_id: None,
    }
}

#[test]
fn known_read_only_commands_are_recognised() {
    for command in [
        "git status",
        "git -C sub log --oneline -5",
        "git diff HEAD~1 -- src",
        "git branch -a",
        "git --no-pager log -1",
        "git grep -n needle",
        "git grep -in --only-matching needle -- src",
        "git ls-remote origin",
        "git ls-remote -q --tags origin",
        "rg -n \"fn main\" src",
        "ls -la | head -20",
        "Get-ChildItem -Recurse src | Select-String foo",
        "cat a.txt 2>&1",
        "git log > /dev/null",
        "find . -name '*.rs'",
        "echo 'a > b; rm -rf /'",
        "(Get-ChildItem -LiteralPath 'D:\\work' -Directory -Force).Count",
        "(gci src).Count | Write-Output",
        "((Get-Content a.txt).Length)",
        "Select-Object -First (Get-Date).Day",
        "(ls)",
        "$dirs = Get-ChildItem -Directory -Force; $dirs.Count",
        "$n = (gci).Count",
        "date +%Y-%m-%d",
        "date -u -d yesterday +%s",
        "hostname",
        "hostname -f",
        "sort -u names.txt",
        "uniq -c sorted.txt",
        "tree -L 2 src",
        "file -b a.bin",
        "ipconfig /all",
        "docker ps -a",
        "ps aux | head",
        "Get-ChildItem src | Group-Object Extension | Sort-Object Count",
        "Get-Content a.json | ConvertFrom-Json | Format-List",
        "[ -f Cargo.toml ]",
    ] {
        assert!(is_known_read_only(command), "{command}");
    }
}

#[test]
fn writes_and_unknown_commands_are_not_read_only() {
    for command in [
        "",
        "cargo build",
        "git commit -m x",
        "git branch feature",
        "git branch -D old",
        "git diff --output=patch.diff",
        "git -c core.fsmonitor=./hook status",
        "git --config-env=core.pager=PAGER log",
        "git --git-dir=other status",
        "git --work-tree=other status",
        "git --bare log",
        "git --exec-path=bin status",
        "git grep -Ohook needle",
        "git grep --open-files-in-pager=hook needle",
        // git's option parser bundles short options and accepts abbreviated long ones.
        // git 选项解析器会合并短选项并接受长选项缩写。
        "git grep -nOhook needle",
        "git grep --open-f=hook needle",
        "git ls-remote --upload-pack=hook .",
        "git ls-remote -u hook .",
        "git ls-remote -quhook .",
        "git ls-remote --upl=hook .",
        "echo hi > out.txt",
        "ls >> out.txt",
        "find . -delete",
        "rg --pre ./script foo",
        "ls && rm a",
        "echo $(rm a)",
        "echo `rm a`",
        "Write-Output (Remove-Item a)",
        "(Remove-Item a).Count",
        "(Get-ChildItem a).Delete()",
        "(Get-ChildItem a)[0].Delete()",
        "(Get-ChildItem > out.txt).Count",
        "(Get-ChildItem a) Remove-Item",
        "(Get-ChildItem a",
        "$(Get-ChildItem a).Count",
        "@(Get-ChildItem a).Count",
        "(Get-ChildItem a).Count; rm a",
        "$env:GIT_DIR = 'x'; git status",
        "$env:GIT_DIR='x'; git status",
        "$ErrorActionPreference = 'Stop'",
        "$PSDefaultParameterValues = @{}",
        "$x = Remove-Item a",
        "$x foo",
        "date -s 2020-01-01",
        "date 010100002020",
        "hostname newname",
        "hostname -F names",
        "sort -o out.txt in.txt",
        "sort -uo out.txt in.txt",
        "sort --out=out.txt in.txt",
        "sort --compress-program=prog in.txt",
        "uniq in.txt out.txt",
        "tree -o out.txt",
        "file -C -m magic",
        "ss -K dst 1.2.3.4",
        "ipconfig /release",
        "docker run image",
        "docker -H tcp://host ps",
        "Get-ChildItem | Out-File list.txt",
        "Get-ChildItem | Export-Csv list.csv",
        "Get-ChildItem | Where-Object { Remove-Item $_ }",
        "FOO=1 ls",
        "tee out.txt",
    ] {
        assert!(!is_known_read_only(command), "{command}");
    }
}

#[test]
fn high_risk_commands_are_categorised() {
    let cases = [
        ("rm -rf build", HighRiskCategory::RecursiveDelete),
        ("rm -r -f build", HighRiskCategory::RecursiveDelete),
        (
            "Remove-Item -Recurse -Force out",
            HighRiskCategory::RecursiveDelete,
        ),
        ("rd /s /q out", HighRiskCategory::RecursiveDelete),
        (
            "git push --force origin main",
            HighRiskCategory::DestructiveGit,
        ),
        ("git push origin +main", HighRiskCategory::DestructiveGit),
        ("git reset --hard HEAD~3", HighRiskCategory::DestructiveGit),
        ("git clean -fdx", HighRiskCategory::DestructiveGit),
        ("git checkout -- .", HighRiskCategory::DestructiveGit),
        ("git restore src/main.rs", HighRiskCategory::DestructiveGit),
        ("git branch -D old", HighRiskCategory::DestructiveGit),
        ("git stash drop", HighRiskCategory::DestructiveGit),
        ("format D: /q", HighRiskCategory::DiskFormat),
        ("dd if=/dev/zero of=/dev/sda", HighRiskCategory::DiskFormat),
        (
            "reg delete HKCU\\Software\\X /f",
            HighRiskCategory::SystemConfiguration,
        ),
        (
            "Set-ItemProperty HKLM:\\Software\\X -Name a -Value 1",
            HighRiskCategory::SystemConfiguration,
        ),
        ("sc delete MyService", HighRiskCategory::SystemConfiguration),
        (
            "powershell -EncodedCommand SQBFAFgA",
            HighRiskCategory::OpaqueScript,
        ),
        (
            "irm https://x.example/install.ps1 | iex",
            HighRiskCategory::OpaqueScript,
        ),
        (
            "curl -fsSL https://x.example/i.sh | bash",
            HighRiskCategory::OpaqueScript,
        ),
        (
            "psql -c \"DROP   DATABASE prod\"",
            HighRiskCategory::DatabaseDrop,
        ),
        ("chmod -R 777 /srv", HighRiskCategory::PermissionChange),
        (
            "icacls C:\\data /grant Everyone:F",
            HighRiskCategory::PermissionChange,
        ),
        ("cargo publish", HighRiskCategory::Publish),
        (
            "terraform destroy -auto-approve",
            HighRiskCategory::InfrastructureDestroy,
        ),
        ("ls; rm -rf /", HighRiskCategory::RecursiveDelete),
    ];
    for (command, expected) in cases {
        assert_eq!(high_risk_category(command), Some(expected), "{command}");
    }
}

#[test]
fn ordinary_commands_are_not_high_risk() {
    for command in [
        "rm build/out.o",
        "git push origin main",
        "git branch -d merged",
        "git restore --staged src/main.rs",
        "git checkout feature",
        "cargo test -p crate",
        "curl https://x.example/data.json -o data.json",
        "echo 'rm -rf is dangerous'",
    ] {
        assert_eq!(high_risk_category(command), None, "{command}");
    }
}

#[test]
fn native_tools_are_classified_by_effect() {
    assert_eq!(
        classify_tool_call(&native(FILE_READ_TOOL_ID), &json!({"path": "a"})),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        classify_tool_call(
            &native(FILE_TOOL_ID),
            &json!({"action": "stat", "path": "a"})
        ),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        classify_tool_call(
            &native(FILE_WRITE_TOOL_ID),
            &json!({"path": "src/x.rs", "content": ""})
        ),
        PermissionSubject::FileWrite {
            paths: vec![String::from("src/x.rs")]
        }
    );
    assert_eq!(
        classify_tool_call(
            &native(FILE_TOOL_ID),
            &json!({"action": "move", "path": "a", "destination": "../b"})
        ),
        PermissionSubject::FileWrite {
            paths: vec![String::from("a"), String::from("../b")]
        }
    );
    assert_eq!(
        classify_tool_call(&native(FILE_EDIT_TOOL_ID), &json!({"path": "src/x.rs"})),
        PermissionSubject::FileWrite {
            paths: vec![String::from("src/x.rs")]
        }
    );
    assert_eq!(
        classify_tool_call(&native(APPLY_PATCH_TOOL_ID), &json!({"patch": "garbage"})),
        PermissionSubject::Opaque
    );
    assert_eq!(
        classify_tool_call(
            &native(SHELL_COMMAND_TOOL_ID),
            &json!({"command": "git status"})
        ),
        PermissionSubject::Command {
            text: String::from("git status")
        }
    );
    assert_eq!(
        classify_tool_call(
            &native(TERMINAL_SESSION_TOOL_ID),
            &json!({"action": "send", "inputs": [{"kind": "key", "key": "Enter"}]})
        ),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        classify_tool_call(
            &native(TERMINAL_SESSION_TOOL_ID),
            &json!({"action": "send", "inputs": [{"kind": "text", "text": "rm -rf x"}]})
        ),
        PermissionSubject::Command {
            text: String::from("rm -rf x")
        }
    );
    assert_eq!(
        classify_tool_call(&native("todolist"), &json!({})),
        PermissionSubject::Ungoverned
    );
}

#[test]
fn extension_tools_reuse_the_plan_read_only_classification() {
    let read = definition(
        VulcanToolSourceKind::Mcp,
        "server__list_items",
        VulcanPlanToolPolicy::HeuristicText(String::from("list_items")),
    );
    let write = definition(
        VulcanToolSourceKind::Mcp,
        "server__create_item",
        VulcanPlanToolPolicy::HeuristicText(String::from("create_item")),
    );
    assert_eq!(
        classify_tool_call(&read, &json!({})),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        classify_tool_call(&write, &json!({})),
        PermissionSubject::Opaque
    );
}

/// Built-in LuaSkills entries are classified exactly, directly and through the dispatcher.
/// 内置 LuaSkills 入口被精确分类，直接调用与经调度器调用一致。
#[test]
fn builtin_luaskills_entries_are_classified_exactly() {
    let skill = |name: &str| {
        definition(
            VulcanToolSourceKind::LuaSkills,
            name,
            VulcanPlanToolPolicy::HeuristicText(name.to_string()),
        )
    };
    // The name heuristic would not clear these, but they only read.
    // 名称启发式不会放行它们，但它们只读取。
    for name in ["vulcan-codekit-repo-map", "vulcan-codekit-node-source"] {
        assert_eq!(
            classify_tool_call(&skill(name), &json!({})),
            PermissionSubject::Ungoverned
        );
    }
    assert_eq!(
        classify_tool_call(
            &skill("vulcan-codekit-patch"),
            &json!({"patches": [{"file": "src/a.rs"}, {"file": "src/b.rs"}]})
        ),
        PermissionSubject::FileWrite {
            paths: vec![String::from("src/a.rs"), String::from("src/b.rs")]
        }
    );
    assert_eq!(
        classify_tool_call(&skill("vulcan-codekit-patch"), &json!({})),
        PermissionSubject::Opaque
    );
    // An explicit PWD replaces the injected workspace root as the base of relative targets.
    // 显式 PWD 取代注入的工作区根目录，成为相对目标的基准。
    let explicit_pwd = std::env::temp_dir();
    assert_eq!(
        classify_tool_call(
            &skill("vulcan-codekit-patch"),
            &json!({"file": "x.rs", "PWD": explicit_pwd.to_str().unwrap()})
        ),
        PermissionSubject::FileWrite {
            paths: vec![explicit_pwd.join("x.rs").to_str().unwrap().to_string()]
        }
    );
    assert_eq!(
        classify_tool_call(
            &skill("vulcan-codekit-patch"),
            &json!({"file": "x.rs", "PWD": "relative/base"})
        ),
        PermissionSubject::Opaque
    );
    let get = skill("vulcan-curl-get");
    assert_eq!(
        classify_tool_call(&get, &json!({"url": "https://example.invalid"})),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        classify_tool_call(&get, &json!({"download_to": "out.bin"})),
        PermissionSubject::Opaque
    );
    assert_eq!(
        classify_tool_call(&skill("vulcan-curl-post"), &json!({})),
        PermissionSubject::Opaque
    );

    let dispatcher = definition(
        VulcanToolSourceKind::Native,
        vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME,
        VulcanPlanToolPolicy::Hidden,
    );
    let dispatch = |name: &str, args: serde_json::Value| {
        classify_tool_call(&dispatcher, &json!({"name": name, "args": args}))
    };
    assert_eq!(
        dispatch("vulcan-codekit-repo-map", json!([])),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        dispatch(
            "vulcan-codekit-patch",
            json!([{"name": "patches", "value": "[{\"file\":\"src/a.rs\"}]"}])
        ),
        PermissionSubject::FileWrite {
            paths: vec![String::from("src/a.rs")]
        }
    );
    // Unknown entries reuse the name heuristic instead of bypassing the gate.
    // 未知入口复用名称启发式，而不是绕过闸门。
    assert_eq!(
        dispatch("other-skill-list", json!([])),
        PermissionSubject::Ungoverned
    );
    assert_eq!(
        dispatch("other-skill-deploy", json!([])),
        PermissionSubject::Opaque
    );
    assert_eq!(
        classify_tool_call(&dispatcher, &json!({})),
        PermissionSubject::Opaque
    );
}

#[test]
fn rules_follow_the_mode_table() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let inside = PermissionSubject::FileWrite {
        paths: vec![String::from("src/x.rs")],
    };
    let outside = PermissionSubject::FileWrite {
        paths: vec![String::from("src/x.rs"), String::from("../elsewhere.txt")],
    };
    let read_command = PermissionSubject::Command {
        text: String::from("git status"),
    };
    let build = PermissionSubject::Command {
        text: String::from("cargo build"),
    };
    let wipe = PermissionSubject::Command {
        text: String::from("rm -rf target"),
    };
    let escalate = RuleOutcome::Escalate { high_risk: None };

    use PermissionMode::*;
    assert_eq!(
        evaluate_rules(WorkspaceWrite, &inside, Some(root)),
        RuleOutcome::Allow
    );
    assert_eq!(
        evaluate_rules(WorkspaceWrite, &outside, Some(root)),
        escalate
    );
    assert_eq!(evaluate_rules(WorkspaceWrite, &inside, None), escalate);
    assert_eq!(
        evaluate_rules(AutoApprove, &inside, Some(root)),
        RuleOutcome::Allow
    );
    assert_eq!(evaluate_rules(AutoApprove, &outside, Some(root)), escalate);
    assert_eq!(evaluate_rules(ReadOnly, &inside, Some(root)), escalate);
    assert_eq!(
        evaluate_rules(ReadOnly, &read_command, Some(root)),
        RuleOutcome::Allow
    );
    assert_eq!(evaluate_rules(WorkspaceWrite, &build, Some(root)), escalate);
    assert_eq!(
        evaluate_rules(WorkspaceWrite, &wipe, Some(root)),
        RuleOutcome::Escalate {
            high_risk: Some(HighRiskCategory::RecursiveDelete)
        }
    );
    assert_eq!(
        evaluate_rules(ReadOnly, &PermissionSubject::Opaque, Some(root)),
        escalate
    );
    for subject in [&inside, &outside, &build, &wipe, &PermissionSubject::Opaque] {
        assert_eq!(
            evaluate_rules(FullAccess, subject, Some(root)),
            RuleOutcome::Allow
        );
    }
}

/// Git metadata names commands Git runs, so writing it is never a plain workspace edit, however
/// the path spells the `.git` component.
/// Git 元数据指定 Git 会运行的命令，因此无论路径如何拼写 `.git` 组件，写入它都不是普通工作区编辑。
#[test]
fn writes_into_git_metadata_escalate_inside_the_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
    std::fs::create_dir_all(root.join("vendor/lib")).unwrap();
    let write = |path: &str| PermissionSubject::FileWrite {
        paths: vec![String::from(path)],
    };
    for path in [
        ".git/config",
        ".GIT/hooks/pre-commit",
        "src/../.git/config",
        "vendor/lib/.git/config",
        "vendor/.git",
        ".git./config",
    ] {
        for mode in [PermissionMode::WorkspaceWrite, PermissionMode::AutoApprove] {
            assert_eq!(
                evaluate_rules(mode, &write(path), Some(root)),
                RuleOutcome::Escalate { high_risk: None },
                "{path}"
            );
        }
    }
    for path in [".gitignore", ".github/workflows/ci.yml", "src/git/config"] {
        assert_eq!(
            evaluate_rules(PermissionMode::WorkspaceWrite, &write(path), Some(root)),
            RuleOutcome::Allow,
            "{path}"
        );
    }
}

#[tokio::test]
async fn gate_asks_the_user_for_escalated_calls_and_fails_closed() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().to_path_buf();
    let edit = native(FILE_EDIT_TOOL_ID);
    let args = json!({"path": "src/x.rs"});
    let session = SessionId::nil();
    let admit = |host: Arc<FixedHost>, args: Value| {
        let edit = edit.clone();
        let root = root.clone();
        async move {
            let host: Arc<dyn ToolPermissionHost> = host;
            admit_tool_call(
                Some(&host),
                session,
                &edit,
                &args,
                PermissionTexts::default(),
                &context(),
                Some(&root),
            )
            .await
        }
    };

    // Read-only escalates even in-workspace writes; the user's answer decides.
    // 只读模式连工作区内写入也会升级；由用户的回答决定。
    let approving = FixedHost::new(Ok(PermissionMode::ReadOnly), Ok(true));
    assert_eq!(
        admit(Arc::clone(&approving), args.clone()).await,
        ToolPermissionVerdict::Proceed
    );
    let prompts = approving.prompts.lock().unwrap().clone();
    assert_eq!(
        prompts,
        vec![(
            String::from(FILE_EDIT_TOOL_ID),
            ToolApprovalPrompt {
                mode: PermissionMode::ReadOnly,
                subject: PermissionSubject::FileWrite {
                    paths: vec![String::from("src/x.rs")]
                },
                high_risk: None,
                judge: None,
                review: None,
                deadline_unix_ms: None,
            }
        )]
    );
    // An approval nobody answers in time is denied with its own outcome and guidance.
    // 无人及时回答的审批以其专属结果与指引被拒绝。
    let expiring = FixedHost::new(Ok(PermissionMode::ReadOnly), Ok(true));
    *expiring.expire.lock().unwrap() = true;
    assert_eq!(
        admit(Arc::clone(&expiring), args.clone()).await,
        ToolPermissionVerdict::Deny(String::from(APPROVAL_EXPIRED_DENIAL))
    );
    assert_eq!(
        admit(
            FixedHost::new(Ok(PermissionMode::ReadOnly), Ok(false)),
            args.clone()
        )
        .await,
        ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
    );
    assert_eq!(
        admit(
            FixedHost::new(
                Ok(PermissionMode::WorkspaceWrite),
                Err(String::from("closed"))
            ),
            json!({"path": "../outside.rs"})
        )
        .await,
        ToolPermissionVerdict::Fail(String::from("closed"))
    );

    // Allowed calls never reach the user; an unreadable mode denies without asking.
    // 已放行调用不会打扰用户；无法读取模式时不询问直接拒绝。
    let silent = FixedHost::new(Ok(PermissionMode::WorkspaceWrite), Ok(false));
    assert_eq!(
        admit(Arc::clone(&silent), args.clone()).await,
        ToolPermissionVerdict::Proceed
    );
    let unreadable = FixedHost::new(Err(String::from("gone")), Ok(true));
    assert_eq!(
        admit(Arc::clone(&unreadable), args.clone()).await,
        ToolPermissionVerdict::Deny(String::from(MODE_UNAVAILABLE_DENIAL))
    );
    assert!(silent.prompts.lock().unwrap().is_empty());
    assert!(unreadable.prompts.lock().unwrap().is_empty());
    assert_eq!(
        admit_tool_call(
            None,
            session,
            &edit,
            &args,
            PermissionTexts::default(),
            &context(),
            Some(workspace.path())
        )
        .await,
        ToolPermissionVerdict::Proceed
    );
}

#[tokio::test]
async fn judge_clears_only_auto_approve_calls_without_high_risk() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().to_path_buf();
    let shell = native(SHELL_COMMAND_TOOL_ID);
    let session = SessionId::nil();
    let admit = |host: Arc<FixedHost>, command: &'static str| {
        let shell = shell.clone();
        let root = root.clone();
        async move {
            let host: Arc<dyn ToolPermissionHost> = host;
            let raw = format!(r#"{{"command": "{command}"}}"#);
            let args: Value = serde_json::from_str(&raw).unwrap();
            admit_tool_call(
                Some(&host),
                session,
                &shell,
                &args,
                PermissionTexts {
                    raw_arguments: Some(&raw),
                    user_request: None,
                },
                &context(),
                Some(&root),
            )
            .await
        }
    };
    let judge_of = |host: &FixedHost| {
        host.prompts
            .lock()
            .unwrap()
            .iter()
            .map(|(_, prompt)| prompt.judge.clone())
            .collect::<Vec<_>>()
    };

    // An allow clears an ordinary escalated call without asking; the judge reads the raw text.
    // allow 会不经询问放行普通升级调用；判定器读取原样文本。
    let clearing = FixedHost::judging(
        Ok(PermissionMode::AutoApprove),
        Ok(false),
        Ok(Some(JudgeLabel::Allow)),
    );
    assert_eq!(
        admit(Arc::clone(&clearing), "cargo build").await,
        ToolPermissionVerdict::Proceed
    );
    assert!(clearing.prompts.lock().unwrap().is_empty());
    assert_eq!(
        clearing.judged.lock().unwrap().clone(),
        vec![JudgeCall {
            workspace_root: Some(root.clone()),
            tool_name: String::from(SHELL_COMMAND_TOOL_ID),
            raw_arguments: Some(String::from(r#"{"command": "cargo build"}"#)),
        }]
    );

    // A high-risk rule still asks, carrying the judge's allow to the user.
    // 高风险规则仍需询问，并把判定器的 allow 带给用户。
    assert_eq!(
        admit(Arc::clone(&clearing), "rm -rf build").await,
        ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
    );
    assert_eq!(
        judge_of(&clearing),
        vec![Some(JudgeFinding::Scored(JudgeLabel::Allow))]
    );

    // Review, block and judge failures all go to the user with the finding.
    // review、block 与判定失败都会带着结论交给用户。
    for (judge, finding) in [
        (
            Ok(Some(JudgeLabel::Review)),
            JudgeFinding::Scored(JudgeLabel::Review),
        ),
        (
            Ok(Some(JudgeLabel::Block)),
            JudgeFinding::Scored(JudgeLabel::Block),
        ),
        (
            Err(String::from("not loaded")),
            JudgeFinding::Unavailable(String::from("not loaded")),
        ),
    ] {
        let host = FixedHost::judging(Ok(PermissionMode::AutoApprove), Ok(true), judge);
        assert_eq!(
            admit(Arc::clone(&host), "cargo build").await,
            ToolPermissionVerdict::Proceed
        );
        assert_eq!(judge_of(&host), vec![Some(finding)]);
    }

    // Read-only and workspace-write never consult the judge.
    // 只读与工作区修改模式从不咨询判定器。
    for mode in [PermissionMode::ReadOnly, PermissionMode::WorkspaceWrite] {
        let unjudged = FixedHost::judging(Ok(mode), Ok(false), Ok(Some(JudgeLabel::Allow)));
        assert_eq!(
            admit(Arc::clone(&unjudged), "cargo build").await,
            ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
        );
        assert!(unjudged.judged.lock().unwrap().is_empty());
        assert_eq!(judge_of(&unjudged), vec![None]);
    }
}

#[tokio::test]
async fn reviewer_decides_within_the_judge_and_high_risk_boundary() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().to_path_buf();
    let shell = native(SHELL_COMMAND_TOOL_ID);
    let session = SessionId::nil();
    let admit = |host: Arc<FixedHost>, command: &'static str| {
        let shell = shell.clone();
        let root = root.clone();
        async move {
            let host: Arc<dyn ToolPermissionHost> = host;
            let args = json!({"command": command});
            admit_tool_call(
                Some(&host),
                session,
                &shell,
                &args,
                PermissionTexts {
                    raw_arguments: None,
                    user_request: Some("clean the build"),
                },
                &context(),
                Some(&root),
            )
            .await
        }
    };
    let review = |verdict, may_allow_blocked| {
        Ok(Some(LlmReview {
            verdict,
            reason: String::from("because"),
            may_allow_blocked,
        }))
    };
    let host = |judge: Option<JudgeLabel>, review| {
        FixedHost::reviewing(
            Ok(PermissionMode::AutoApprove),
            Ok(false),
            Ok(judge),
            review,
        )
    };

    // A reviewer allow clears a judge review without asking; it reads the call and the intent.
    // 复核器的 allow 会不经询问放行判定为 review 的调用；它读取调用与意图。
    let clearing = host(
        Some(JudgeLabel::Review),
        review(ReviewVerdict::Allow, false),
    );
    assert_eq!(
        admit(Arc::clone(&clearing), "cargo build").await,
        ToolPermissionVerdict::Proceed
    );
    assert!(clearing.prompts.lock().unwrap().is_empty());
    assert_eq!(
        clearing.reviewed.lock().unwrap().clone(),
        vec![ReviewCall {
            session_id: SessionId::nil(),
            workspace_root: Some(root.clone()),
            tool_name: String::from(SHELL_COMMAND_TOOL_ID),
            arguments: String::from(r#"{"command":"cargo build"}"#),
            user_request: Some(String::from("clean the build")),
            high_risk: None,
            judge: Some(JudgeFinding::Scored(JudgeLabel::Review)),
            purpose: None,
            intent: None,
            unattended: None,
        }]
    );

    // Without a judge the reviewer may clear too; a judge allow never reaches it.
    // 没有判定器时复核器同样可以放行；判定器的 allow 不会到达复核器。
    let unjudged = host(None, review(ReviewVerdict::Allow, false));
    assert_eq!(
        admit(Arc::clone(&unjudged), "cargo build").await,
        ToolPermissionVerdict::Proceed
    );
    let judged_allow = host(Some(JudgeLabel::Allow), review(ReviewVerdict::Deny, false));
    assert_eq!(
        admit(Arc::clone(&judged_allow), "cargo build").await,
        ToolPermissionVerdict::Proceed
    );
    assert!(judged_allow.reviewed.lock().unwrap().is_empty());

    // Blocked or high-risk calls reach the user with the reviewer's allow, unless the user lets
    // the reviewer clear them.
    // 被拦截或高风险的调用会带着复核器的 allow 交给用户，除非用户允许复核器放行它们。
    for (judge, command) in [
        (Some(JudgeLabel::Block), "cargo build"),
        (Some(JudgeLabel::Review), "rm -rf build"),
    ] {
        let guarded = host(judge, review(ReviewVerdict::Allow, false));
        assert_eq!(
            admit(Arc::clone(&guarded), command).await,
            ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
        );
        let verdicts: Vec<_> = guarded
            .prompts
            .lock()
            .unwrap()
            .iter()
            .map(|(_, prompt)| match &prompt.review {
                Some(ReviewFinding::Reviewed(review)) => Some(review.verdict),
                _ => None,
            })
            .collect();
        assert_eq!(verdicts, vec![Some(ReviewVerdict::Allow)]);
        let trusted = host(judge, review(ReviewVerdict::Allow, true));
        assert_eq!(
            admit(Arc::clone(&trusted), command).await,
            ToolPermissionVerdict::Proceed
        );
    }

    // A deny returns the reason to the model; ask_user and failures reach the user.
    // deny 把理由返回给模型；ask_user 与失败交给用户。
    assert_eq!(
        admit(
            host(None, review(ReviewVerdict::Deny, false)),
            "cargo build"
        )
        .await,
        ToolPermissionVerdict::Deny(format!("{REVIEW_DENIAL_PREFIX}because"))
    );
    for (answer, finding) in [
        (
            review(ReviewVerdict::AskUser, true),
            ReviewFinding::Reviewed(LlmReview {
                verdict: ReviewVerdict::AskUser,
                reason: String::from("because"),
                may_allow_blocked: true,
            }),
        ),
        (
            Err(String::from("offline")),
            ReviewFinding::Unavailable(String::from("offline")),
        ),
    ] {
        let asking = host(None, answer);
        assert_eq!(
            admit(Arc::clone(&asking), "cargo build").await,
            ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
        );
        let reviews: Vec<_> = asking
            .prompts
            .lock()
            .unwrap()
            .iter()
            .map(|(_, prompt)| prompt.review.clone())
            .collect();
        assert_eq!(reviews, vec![Some(finding)]);
    }

    // Read-only and workspace-write never consult the reviewer.
    // 只读与工作区修改模式从不咨询复核器。
    for mode in [PermissionMode::ReadOnly, PermissionMode::WorkspaceWrite] {
        let unreviewed = FixedHost::reviewing(
            Ok(mode),
            Ok(false),
            Ok(None),
            review(ReviewVerdict::Allow, true),
        );
        assert_eq!(
            admit(Arc::clone(&unreviewed), "cargo build").await,
            ToolPermissionVerdict::Deny(String::from(USER_DENIAL))
        );
        assert!(unreviewed.reviewed.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn records_each_decision_the_judge_or_reviewer_took_part_in() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().to_path_buf();
    let shell = native(SHELL_COMMAND_TOOL_ID);
    let admit = |host: Arc<FixedHost>| {
        let shell = shell.clone();
        let root = root.clone();
        async move {
            let host: Arc<dyn ToolPermissionHost> = host;
            let raw = r#"{"command": "cargo build"}"#;
            let args: Value = serde_json::from_str(raw).unwrap();
            admit_tool_call(
                Some(&host),
                SessionId::nil(),
                &shell,
                &args,
                PermissionTexts {
                    raw_arguments: Some(raw),
                    user_request: None,
                },
                &context(),
                Some(&root),
            )
            .await
        }
    };
    let outcomes = |host: &FixedHost| {
        host.records
            .lock()
            .unwrap()
            .iter()
            .map(|record| record.outcome)
            .collect::<Vec<_>>()
    };

    // A judge allow is recorded with what the judge read and the raw call text.
    // 判定器的 allow 会连同其读取内容与原样调用文本一并记录。
    let clearing = FixedHost::judging(
        Ok(PermissionMode::AutoApprove),
        Ok(false),
        Ok(Some(JudgeLabel::Allow)),
    );
    admit(Arc::clone(&clearing)).await;
    let records = clearing.records.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, PermissionOutcome::JudgeAllowed);
    assert_eq!(records[0].judge_trace, Some(trace()));
    assert_eq!(records[0].arguments, r#"{"command": "cargo build"}"#);
    assert!(records[0].judge_elapsed.is_some());

    // The user's answer and the reviewer's denial are recorded as the deciding layer.
    // 用户的回答与复核器的拒绝按作出决定的层记录。
    let asking = FixedHost::judging(
        Ok(PermissionMode::AutoApprove),
        Ok(false),
        Ok(Some(JudgeLabel::Review)),
    );
    admit(Arc::clone(&asking)).await;
    assert_eq!(outcomes(&asking), vec![PermissionOutcome::UserDenied]);
    let denying = FixedHost::reviewing(
        Ok(PermissionMode::AutoApprove),
        Ok(true),
        Ok(None),
        Ok(Some(LlmReview {
            verdict: ReviewVerdict::Deny,
            reason: String::from("because"),
            may_allow_blocked: false,
        })),
    );
    admit(Arc::clone(&denying)).await;
    assert_eq!(outcomes(&denying), vec![PermissionOutcome::ReviewDenied]);

    // Without a judge or reviewer nothing is recorded.
    // 没有判定器与复核器时不记录任何内容。
    let plain = FixedHost::new(Ok(PermissionMode::AutoApprove), Ok(true));
    admit(Arc::clone(&plain)).await;
    assert!(plain.records.lock().unwrap().is_empty());
}

/// Runs one call of `tool` with `args` under `user_request` through the gate of `host`.
/// 在 `user_request` 下经 `host` 的闸门运行一次 `tool` 调用。
async fn admit_call(
    host: &Arc<FixedHost>,
    tool: &str,
    args: Value,
    user_request: Option<&str>,
    root: &Path,
) -> ToolPermissionVerdict {
    let host: Arc<dyn ToolPermissionHost> = Arc::clone(host) as Arc<dyn ToolPermissionHost>;
    let raw = args.to_string();
    admit_tool_call(
        Some(&host),
        SessionId::nil(),
        &native(tool),
        &args,
        PermissionTexts {
            raw_arguments: Some(&raw),
            user_request,
        },
        &context(),
        Some(root),
    )
    .await
}

/// Builds an unattended host whose reviewer answers every call with `review`.
/// 构建一个无人值守宿主，其复核器对每次调用都回答 `review`。
fn unattended(mode: PermissionMode, review: Result<Option<LlmReview>, String>) -> Arc<FixedHost> {
    let host = FixedHost::reviewing(Ok(mode), Ok(true), Ok(None), review);
    *host.unattended.lock().unwrap() = Some(String::from("never push"));
    host
}

#[tokio::test]
async fn unattended_mode_never_asks_the_user() {
    let workspace = tempfile::tempdir().unwrap();
    let command = || json!({"command": "cargo build"});
    let verdict = |verdict| {
        Ok(Some(LlmReview {
            verdict,
            reason: String::from("reviewer note"),
            may_allow_blocked: false,
        }))
    };

    // Whatever the reviewer cannot settle is denied with its note, and no prompt is shown.
    // 复核器无法定夺的调用都会附带其说明被拒绝，且不显示任何提示。
    for review in [
        verdict(ReviewVerdict::AskUser),
        verdict(ReviewVerdict::Deny),
        Ok(None),
        Err(String::from("offline")),
    ] {
        let host = unattended(PermissionMode::AutoApprove, review);
        let ToolPermissionVerdict::Deny(denial) = admit_call(
            &host,
            SHELL_COMMAND_TOOL_ID,
            command(),
            None,
            workspace.path(),
        )
        .await
        else {
            panic!("an unattended call the reviewer did not allow must be denied");
        };
        assert!(!denial.is_empty());
        assert!(host.prompts.lock().unwrap().is_empty());
        let reviewed = host.reviewed.lock().unwrap();
        assert_eq!(reviewed[0].unattended.as_deref(), Some("never push"));
    }
    let host = unattended(PermissionMode::AutoApprove, verdict(ReviewVerdict::AskUser));
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        command(),
        None,
        workspace.path(),
    )
    .await;
    let ToolPermissionVerdict::Deny(denial) = admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        json!({"command": "cargo test"}),
        None,
        workspace.path(),
    )
    .await
    else {
        panic!("the reviewer's ask_user must be denied");
    };
    assert!(denial.contains("reviewer note"));
    assert_eq!(
        host.records.lock().unwrap()[0].outcome,
        PermissionOutcome::UnattendedDenied
    );

    // A reviewer allow still runs the call.
    // 复核器放行时调用仍会执行。
    let host = unattended(PermissionMode::AutoApprove, verdict(ReviewVerdict::Allow));
    assert_eq!(
        admit_call(
            &host,
            SHELL_COMMAND_TOOL_ID,
            command(),
            None,
            workspace.path()
        )
        .await,
        ToolPermissionVerdict::Proceed
    );

    // Unattended access outside auto-approve is unreadable: denied without asking anyone.
    // 自动审批以外的无人值守授权不可读：不询问任何人直接拒绝。
    for mode in [PermissionMode::ReadOnly, PermissionMode::WorkspaceWrite] {
        let host = unattended(mode, verdict(ReviewVerdict::Allow));
        assert_eq!(
            admit_call(
                &host,
                SHELL_COMMAND_TOOL_ID,
                command(),
                None,
                workspace.path()
            )
            .await,
            ToolPermissionVerdict::Deny(String::from(MODE_UNAVAILABLE_DENIAL))
        );
        assert!(host.prompts.lock().unwrap().is_empty());
        assert!(host.reviewed.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn declare_intent_needs_a_denial_that_invites_it() {
    let workspace = tempfile::tempdir().unwrap();
    let denying = || {
        Ok(Some(LlmReview {
            verdict: ReviewVerdict::Deny,
            reason: String::from("unclear why"),
            may_allow_blocked: false,
        }))
    };
    let declare = |host: &Arc<FixedHost>, request: &'static str| {
        let host = Arc::clone(host);
        let root = workspace.path().to_path_buf();
        async move {
            admit_call(
                &host,
                DECLARE_INTENT_TOOL_ID,
                json!({"intent": " publish the docs "}),
                Some(request),
                &root,
            )
            .await
        }
    };
    let uninvited = ToolPermissionVerdict::Deny(String::from(DECLARE_INTENT_UNINVITED));

    // Attended sessions and calls that were never denied give no invitation.
    // 有人值守会话与从未被拒的调用都不会给出邀请。
    let attended = FixedHost::reviewing(
        Ok(PermissionMode::AutoApprove),
        Ok(false),
        Ok(None),
        denying(),
    );
    assert_eq!(declare(&attended, "build").await, uninvited);
    admit_call(
        &attended,
        SHELL_COMMAND_TOOL_ID,
        json!({"command": "cargo build"}),
        Some("build"),
        workspace.path(),
    )
    .await;
    assert_eq!(declare(&attended, "build").await, uninvited);

    // An unexplained unattended denial invites one declaration under the same user request,
    // and that intent reaches the reviewer with the next call only.
    // 未说明理由的无人值守拒绝邀请同一用户请求下的一次声明，该意图只随下一次调用交给复核器。
    let host = unattended(PermissionMode::AutoApprove, denying());
    let build = |purpose: Option<&str>| match purpose {
        Some(purpose) => json!({"command": "cargo build", "purpose": purpose}),
        None => json!({"command": "cargo build"}),
    };
    let ToolPermissionVerdict::Deny(first) = admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        build(None),
        Some("build"),
        workspace.path(),
    )
    .await
    else {
        panic!("the reviewer denies the call");
    };
    assert!(first.contains("declare_intent"));
    assert_eq!(declare(&host, "other").await, uninvited);
    assert_eq!(
        declare(&host, "build").await,
        ToolPermissionVerdict::Proceed
    );
    assert_eq!(declare(&host, "build").await, uninvited);
    let ToolPermissionVerdict::Deny(second) = admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        build(None),
        Some("build"),
        workspace.path(),
    )
    .await
    else {
        panic!("the reviewer denies the call again");
    };
    assert!(!second.contains("declare_intent"));
    assert_eq!(declare(&host, "build").await, uninvited);
    {
        let reviewed = host.reviewed.lock().unwrap();
        assert_eq!(reviewed[0].intent, None);
        assert_eq!(reviewed[1].intent.as_deref(), Some("publish the docs"));
        assert!(reviewed[1].unattended.is_some());
    }

    // A call that already stated its purpose is told to move on, not invited.
    // 已说明目的的调用会被告知改做其他事，而不会被邀请。
    let host = unattended(PermissionMode::AutoApprove, denying());
    let ToolPermissionVerdict::Deny(explained) = admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        build(Some("ship the fix")),
        Some("build"),
        workspace.path(),
    )
    .await
    else {
        panic!("the reviewer denies the call");
    };
    assert!(!explained.contains("declare_intent"));
    assert_eq!(declare(&host, "build").await, uninvited);
    assert_eq!(
        host.reviewed.lock().unwrap()[0].purpose.as_deref(),
        Some("ship the fix")
    );

    // Without a reviewer no reason can help, so nothing is invited.
    // 没有复核器时任何理由都无济于事，因此不邀请。
    let host = unattended(PermissionMode::AutoApprove, Ok(None));
    let ToolPermissionVerdict::Deny(unreviewed) = admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        build(None),
        Some("build"),
        workspace.path(),
    )
    .await
    else {
        panic!("an unreviewed unattended call is denied");
    };
    assert!(!unreviewed.contains("declare_intent"));
    assert_eq!(declare(&host, "build").await, uninvited);
}

#[tokio::test]
async fn unattended_mode_stops_reviewing_a_call_denied_twice_in_a_row() {
    let workspace = tempfile::tempdir().unwrap();
    let host = unattended(
        PermissionMode::AutoApprove,
        Ok(Some(LlmReview {
            verdict: ReviewVerdict::Deny,
            reason: String::from("no"),
            may_allow_blocked: false,
        })),
    );
    let push = |purpose: Option<&str>| match purpose {
        Some(purpose) => json!({"command": "git push", "purpose": purpose}),
        None => json!({"command": "git push"}),
    };
    let reviews = || host.reviewed.lock().unwrap().len();

    // Adding a purpose does not make it a different call.
    // 添加 purpose 不会使其成为不同的调用。
    for purpose in [None, Some("ship"), Some("ship it now")] {
        let verdict = admit_call(
            &host,
            SHELL_COMMAND_TOOL_ID,
            push(purpose),
            Some("release"),
            workspace.path(),
        )
        .await;
        assert!(matches!(verdict, ToolPermissionVerdict::Deny(_)));
    }
    assert_eq!(reviews() as u32, UNATTENDED_REVIEWED_REPEATS);

    // A different gated call in between starts over.
    // 中间出现不同的受管控调用会重新计数。
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        json!({"command": "cargo publish"}),
        Some("release"),
        workspace.path(),
    )
    .await;
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        push(None),
        Some("release"),
        workspace.path(),
    )
    .await;
    assert_eq!(reviews() as u32, UNATTENDED_REVIEWED_REPEATS + 2);

    // A new user request starts over as well.
    // 新的用户请求同样会重新计数。
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        push(None),
        Some("release"),
        workspace.path(),
    )
    .await;
    let before = reviews();
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        push(None),
        Some("release"),
        workspace.path(),
    )
    .await;
    assert_eq!(reviews(), before);
    admit_call(
        &host,
        SHELL_COMMAND_TOOL_ID,
        push(None),
        Some("push now"),
        workspace.path(),
    )
    .await;
    assert_eq!(reviews(), before + 1);
}
