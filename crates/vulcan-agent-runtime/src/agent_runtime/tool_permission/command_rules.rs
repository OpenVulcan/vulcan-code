//! Deterministic shell-command rules: the known read-only list and the high-risk list.
//! 确定性 shell 命令规则：已知只读清单与高风险清单。
//!
//! Both lists are conservative text rules, not a shell parser. A command is read-only only
//! when every segment is a known read-only command with no write redirection or command
//! substitution; anything unrecognised is simply "not read-only" and goes through the
//! permission pipeline. The high-risk list marks commands a judge model may never allow alone.
//! 两份清单都是保守的文本规则，而不是 shell 解析器。只有每个片段都是已知只读命令、且不含写入
//! 重定向或命令替换时才算只读；无法识别的命令仅视为“非只读”并进入权限管线。高风险清单标记
//! 判定模型不能单独放行的命令。

/// Redirection targets that discard or merge output and therefore write nothing.
/// 丢弃或合并输出、因而不写入任何内容的重定向目标。
const HARMLESS_REDIRECTION_TARGETS: &[&str] = &["&1", "&2", "$null", "/dev/null", "nul"];

/// Commands that only read, whatever their arguments.
/// 无论参数如何都只读的命令。
const ALWAYS_READ_ONLY: &[&str] = &[
    "cat",
    "cmp",
    "cut",
    "date",
    "df",
    "diff",
    "dir",
    "dirname",
    "basename",
    "du",
    "echo",
    "egrep",
    "fgrep",
    "file",
    "findstr",
    "grep",
    "head",
    "hostname",
    "jq",
    "ls",
    "md5sum",
    "nl",
    "printenv",
    "pwd",
    "realpath",
    "sha1sum",
    "sha256sum",
    "stat",
    "tail",
    "tree",
    "type",
    "uname",
    "wc",
    "where",
    "which",
    "whoami",
    "format-list",
    "format-table",
    "get-childitem",
    "get-command",
    "get-content",
    "get-date",
    "get-filehash",
    "get-item",
    "get-itemproperty",
    "get-location",
    "get-process",
    "get-service",
    "measure-object",
    "out-string",
    "resolve-path",
    "select-object",
    "select-string",
    "sort-object",
    "test-path",
    "where-object",
    "write-host",
    "write-output",
    "gci",
    "gc",
    "gcm",
    "gi",
    "gl",
    "sls",
];

/// Git subcommands that only read repository state.
/// 只读取仓库状态的 Git 子命令。
const GIT_READ_ONLY_SUBCOMMANDS: &[&str] = &[
    "blame",
    "cat-file",
    "describe",
    "diff",
    "for-each-ref",
    "grep",
    "log",
    "ls-files",
    "ls-remote",
    "ls-tree",
    "merge-base",
    "name-rev",
    "rev-list",
    "rev-parse",
    "shortlog",
    "show",
    "show-ref",
    "status",
    "version",
    "whatchanged",
];

/// Git global options that change neither configuration nor the programs Git runs; `-C <dir>`
/// is handled separately because it takes a value.
/// 既不改变配置也不改变 Git 所运行程序的 Git 全局选项；`-C <dir>` 带有取值，因此单独处理。
const GIT_READ_ONLY_GLOBAL_OPTIONS: &[&str] = &[
    "--no-pager",
    "-P",
    "--no-optional-locks",
    "--literal-pathspecs",
    "--glob-pathspecs",
    "--noglob-pathspecs",
    "--icase-pathspecs",
    "--no-replace-objects",
];

/// `git branch` flags that modify branches; any of them makes the call a write.
/// 会修改分支的 `git branch` 参数；出现任意一个即为写入。
const GIT_BRANCH_WRITE_FLAGS: &[&str] = &[
    "-d",
    "-D",
    "-m",
    "-M",
    "-c",
    "-C",
    "-f",
    "-u",
    "--delete",
    "--move",
    "--copy",
    "--force",
    "--set-upstream-to",
    "--unset-upstream",
    "--edit-description",
];

/// `find` actions that delete, execute, or write files.
/// 会删除、执行或写入文件的 `find` 动作。
const FIND_WRITE_ACTIONS: &[&str] = &[
    "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls",
];

/// Download commands whose output piped into an interpreter runs remote code.
/// 输出被管道送入解释器即执行远程代码的下载命令。
const DOWNLOADERS: &[&str] = &[
    "curl",
    "wget",
    "irm",
    "iwr",
    "invoke-restmethod",
    "invoke-webrequest",
];

/// Interpreters that execute their standard input.
/// 执行其标准输入的解释器。
const INTERPRETERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "pwsh",
    "powershell",
    "cmd",
    "python",
    "python3",
    "node",
    "iex",
    "invoke-expression",
];

/// Lower-case text fragments that mark destructive database statements.
/// 标记破坏性数据库语句的小写文本片段。
const DATABASE_DROP_FRAGMENTS: &[&str] = &[
    "drop database",
    "drop schema",
    "drop table",
    "truncate table",
    "dropdatabase(",
];

/// High-risk categories; stable keys shared with the permission policy and audit records.
/// 高风险类别；与权限策略和审计记录共享的稳定键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighRiskCategory {
    /// Recursive or forced deletion of files or directories.
    /// 递归或强制删除文件或目录。
    RecursiveDelete,
    /// Discarding work or rewriting history: hard reset, clean, force push, branch deletion.
    /// 丢弃工作或改写历史：硬重置、清理、强制推送、删除分支。
    DestructiveGit,
    /// Disk formatting or raw device writes.
    /// 磁盘格式化或裸设备写入。
    DiskFormat,
    /// Registry, services, boot, scheduled tasks, firewall, shutdown, or execution policy.
    /// 注册表、服务、启动、计划任务、防火墙、关机或执行策略。
    SystemConfiguration,
    /// Encoded commands, `Invoke-Expression`, or a download piped into an interpreter.
    /// 编码命令、`Invoke-Expression` 或下载内容管道送入解释器。
    OpaqueScript,
    /// Dropping or truncating databases, schemas, or tables.
    /// 删除或清空数据库、模式或表。
    DatabaseDrop,
    /// Recursive ownership or access-control changes.
    /// 递归所有权或访问控制变更。
    PermissionChange,
    /// Publishing packages or deleting remote repositories.
    /// 发布软件包或删除远程仓库。
    Publish,
    /// Destroying containers, volumes, clusters, or infrastructure.
    /// 销毁容器卷、集群或基础设施。
    InfrastructureDestroy,
}

impl HighRiskCategory {
    /// Returns the stable snake-case key of this category.
    /// 返回此类别的稳定蛇形命名键。
    pub fn key(self) -> &'static str {
        match self {
            Self::RecursiveDelete => "recursive_delete",
            Self::DestructiveGit => "destructive_git",
            Self::DiskFormat => "disk_format",
            Self::SystemConfiguration => "system_configuration",
            Self::OpaqueScript => "opaque_script",
            Self::DatabaseDrop => "database_drop",
            Self::PermissionChange => "permission_change",
            Self::Publish => "publish",
            Self::InfrastructureDestroy => "infrastructure_destroy",
        }
    }
}

/// One command split at unquoted separators, with the unsafe constructs found while splitting.
/// 在未加引号的分隔符处切分的命令，以及切分时发现的不安全结构。
struct CommandScan {
    /// Segments in order; each holds its unquoted, whitespace-split words.
    /// 按顺序排列的片段；每个片段保存去除引号并按空白切分后的词。
    segments: Vec<Vec<String>>,
    /// Whether output is redirected into anything other than a discard target.
    /// 是否把输出重定向到丢弃目标以外的地方。
    writes_redirection: bool,
    /// Whether the text contains command substitution or an escape we do not interpret.
    /// 文本是否含有命令替换或我们不解释的转义。
    has_substitution: bool,
}

/// Returns whether `command` is a known read-only command in every segment.
/// 返回 `command` 的每个片段是否都是已知只读命令。
pub fn is_known_read_only(command: &str) -> bool {
    let scan = scan_command(command);
    if scan.writes_redirection || scan.has_substitution || scan.segments.is_empty() {
        return false;
    }
    scan.segments
        .iter()
        .all(|words| segment_is_read_only(words))
}

/// Returns the first high-risk category matched by `command`, if any.
/// 返回 `command` 命中的第一个高风险类别（如有）。
pub fn high_risk_category(command: &str) -> Option<HighRiskCategory> {
    let lowered = command.to_lowercase();
    if DATABASE_DROP_FRAGMENTS
        .iter()
        .any(|fragment| collapse_whitespace(&lowered).contains(fragment))
    {
        return Some(HighRiskCategory::DatabaseDrop);
    }
    let scan = scan_command(command);
    let mut previous_downloads = false;
    for words in &scan.segments {
        let Some(program) = words.first().map(|word| program_name(word)) else {
            continue;
        };
        let args: Vec<String> = words[1..].iter().map(|word| word.to_lowercase()).collect();
        if previous_downloads && INTERPRETERS.contains(&program.as_str()) {
            return Some(HighRiskCategory::OpaqueScript);
        }
        // Git flags are case-sensitive (`-D` versus `-d`), so git sees the original words.
        // Git 参数区分大小写（`-D` 与 `-d`），因此 git 使用原始词。
        let category = if program == "git" {
            git_high_risk(&words[1..])
        } else {
            segment_high_risk(&program, &args)
        };
        if let Some(category) = category {
            return Some(category);
        }
        previous_downloads = previous_downloads || DOWNLOADERS.contains(&program.as_str());
    }
    None
}

/// Classifies one segment's program and lower-cased arguments against the high-risk list.
/// 依据高风险清单对一个片段的程序及其小写参数分类。
fn segment_high_risk(program: &str, args: &[String]) -> Option<HighRiskCategory> {
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let has_prefix = |prefix: &str| args.iter().any(|arg| arg.starts_with(prefix));
    // Short-flag clusters such as `-rf` carry several single-letter flags at once.
    // `-rf` 之类的短参数簇一次携带多个单字母参数。
    let short_cluster_has = |letter: char| {
        args.iter().any(|arg| {
            arg.len() > 1
                && arg.starts_with('-')
                && !arg.starts_with("--")
                && arg[1..].contains(letter)
        })
    };
    match program {
        "rm" if short_cluster_has('r') || has("--recursive") => {
            Some(HighRiskCategory::RecursiveDelete)
        }
        "remove-item" | "ri" | "del" | "erase" | "rd" | "rmdir"
            if has_prefix("-rec") || has("/s") =>
        {
            Some(HighRiskCategory::RecursiveDelete)
        }
        "format" | "format-volume" | "clear-disk" | "initialize-disk" | "diskpart" | "fdisk"
        | "parted" => Some(HighRiskCategory::DiskFormat),
        "dd" if has_prefix("of=") => Some(HighRiskCategory::DiskFormat),
        program if program.starts_with("mkfs") => Some(HighRiskCategory::DiskFormat),
        "reg"
            if args.first().is_some_and(|verb| {
                ["add", "delete", "import", "restore", "load", "unload"].contains(&verb.as_str())
            }) =>
        {
            Some(HighRiskCategory::SystemConfiguration)
        }
        "set-itemproperty"
        | "new-itemproperty"
        | "remove-itemproperty"
        | "new-item"
        | "remove-item"
            if args.iter().any(|arg| {
                arg.starts_with("hklm:")
                    || arg.starts_with("hkcu:")
                    || arg.starts_with("registry::")
            }) =>
        {
            Some(HighRiskCategory::SystemConfiguration)
        }
        "sc" if args.first().is_some_and(|verb| {
            ["delete", "config", "create", "stop"].contains(&verb.as_str())
        }) =>
        {
            Some(HighRiskCategory::SystemConfiguration)
        }
        "schtasks" if has("/create") || has("/delete") || has("/change") => {
            Some(HighRiskCategory::SystemConfiguration)
        }
        "new-service"
        | "remove-service"
        | "set-service"
        | "stop-service"
        | "bcdedit"
        | "shutdown"
        | "restart-computer"
        | "stop-computer"
        | "netsh"
        | "set-executionpolicy" => Some(HighRiskCategory::SystemConfiguration),
        "powershell" | "pwsh"
            if args.iter().any(|arg| {
                ["-e", "-ec", "-enc", "-encodedcommand"].contains(&arg.as_str())
                    || arg.starts_with("-encodedc")
            }) =>
        {
            Some(HighRiskCategory::OpaqueScript)
        }
        "iex" | "invoke-expression" => Some(HighRiskCategory::OpaqueScript),
        "chmod" | "chown" if short_cluster_has('r') || has("--recursive") => {
            Some(HighRiskCategory::PermissionChange)
        }
        "takeown" => Some(HighRiskCategory::PermissionChange),
        "icacls"
            if ["/grant", "/deny", "/remove", "/reset", "/setowner"]
                .iter()
                .any(|flag| args.iter().any(|arg| arg.starts_with(flag))) =>
        {
            Some(HighRiskCategory::PermissionChange)
        }
        "cipher" if has_prefix("/w") => Some(HighRiskCategory::RecursiveDelete),
        "npm" | "pnpm" | "yarn" | "cargo" if args.first().is_some_and(|verb| verb == "publish") => {
            Some(HighRiskCategory::Publish)
        }
        "gh" if args.len() >= 2 && args[0] == "repo" && args[1] == "delete" => {
            Some(HighRiskCategory::Publish)
        }
        "docker"
            if (args.len() >= 2 && args[0] == "system" && args[1] == "prune")
                || (args.len() >= 2
                    && args[0] == "volume"
                    && (args[1] == "prune" || args[1] == "rm")) =>
        {
            Some(HighRiskCategory::InfrastructureDestroy)
        }
        "kubectl" if args.first().is_some_and(|verb| verb == "delete") => {
            Some(HighRiskCategory::InfrastructureDestroy)
        }
        "terraform" if args.first().is_some_and(|verb| verb == "destroy") => {
            Some(HighRiskCategory::InfrastructureDestroy)
        }
        _ => None,
    }
}

/// Classifies one `git` invocation's original-case arguments against the high-risk list.
/// 依据高风险清单对一次 `git` 调用的原始大小写参数分类。
fn git_high_risk(args: &[String]) -> Option<HighRiskCategory> {
    let (subcommand, rest) = git_subcommand(args)?;
    let has = |flag: &str| rest.iter().any(|arg| arg == flag);
    let short_cluster_has = |letter: char| {
        rest.iter().any(|arg| {
            arg.len() > 1
                && arg.starts_with('-')
                && !arg.starts_with("--")
                && arg[1..].contains(letter)
        })
    };
    let destructive = match subcommand.as_str() {
        "push" => {
            has("--force")
                || has("-f")
                || has("--mirror")
                || has("--delete")
                || has("-d")
                || rest.iter().any(|arg| {
                    arg.starts_with("--force-with-lease")
                        || (!arg.starts_with('-') && (arg.starts_with('+') || arg.starts_with(':')))
                })
        }
        "reset" => has("--hard") || has("--merge") || has("--keep"),
        "clean" => short_cluster_has('f') || has("--force"),
        "checkout" => has("--") || has(".") || has("-f") || has("--force"),
        // Restoring the worktree (the default unless only the index is named) discards edits.
        // 恢复工作区（除非只指定索引，否则默认如此）会丢弃修改。
        "restore" => (!has("--staged") && !has("-S")) || has("--worktree") || has("-W"),
        "branch" => has("-D") || (has("-d") && has("-f")) || (has("--delete") && has("--force")),
        "stash" => rest
            .first()
            .is_some_and(|verb| verb == "drop" || verb == "clear"),
        "reflog" => rest
            .first()
            .is_some_and(|verb| verb == "expire" || verb == "delete"),
        "update-ref" => has("-d"),
        "filter-branch" | "filter-repo" => true,
        "gc" => rest.iter().any(|arg| arg.starts_with("--prune")),
        _ => false,
    };
    destructive.then_some(HighRiskCategory::DestructiveGit)
}

/// Returns whether one segment is a known read-only command.
/// 返回一个片段是否为已知只读命令。
fn segment_is_read_only(words: &[String]) -> bool {
    let Some(first) = words.first() else {
        return true;
    };
    // `NAME=value cmd` changes the environment of an arbitrary program; never read-only.
    // `NAME=value cmd` 改变任意程序的环境；绝不视为只读。
    if first.contains('=') {
        return false;
    }
    let program = program_name(first);
    let args = &words[1..];
    match program.as_str() {
        "git" => git_is_read_only(args),
        "find" => !args
            .iter()
            .any(|arg| FIND_WRITE_ACTIONS.contains(&arg.as_str())),
        "fd" => !args
            .iter()
            .any(|arg| ["-x", "-X", "--exec", "--exec-batch"].contains(&arg.as_str())),
        "rg" => !args.iter().any(|arg| arg.starts_with("--pre")),
        program => ALWAYS_READ_ONLY.contains(&program),
    }
}

/// Returns whether one `git` invocation only reads repository state.
/// 返回一次 `git` 调用是否只读取仓库状态。
///
/// Configuration can make any subcommand run a program (`-c core.fsmonitor=…`), and `--git-dir`,
/// `--work-tree`, `--bare` and `--exec-path` choose which configuration or programs apply, so only
/// global options that change neither keep the call read-only.
/// 配置可使任意子命令运行程序（`-c core.fsmonitor=…`），`--git-dir`、`--work-tree`、`--bare` 与
/// `--exec-path` 决定适用哪份配置或程序，因此只有两者都不改变的全局选项才保持只读。
fn git_is_read_only(args: &[String]) -> bool {
    let Some((subcommand, rest)) = git_subcommand(args) else {
        return false;
    };
    let global = &args[..args.len() - rest.len() - 1];
    let mut options = global.iter();
    while let Some(option) = options.next() {
        if option == "-C" {
            options.next();
        } else if !GIT_READ_ONLY_GLOBAL_OPTIONS.contains(&option.as_str()) {
            return false;
        }
    }
    if rest.iter().any(|arg| arg.starts_with("--output")) {
        return false;
    }
    match subcommand.as_str() {
        // `-O`/`--open-files-in-pager` runs the named program on the matches.
        // `-O`/`--open-files-in-pager` 会对匹配结果运行指定程序。
        "grep" => !rest
            .iter()
            .any(|arg| git_option_given(arg, 'O', "--open-files-in-pager")),
        // `--upload-pack`/`-u` names the program run for a local or SSH remote.
        // `--upload-pack`/`-u` 指定本地或 SSH 远端所运行的程序。
        "ls-remote" => !rest
            .iter()
            .any(|arg| git_option_given(arg, 'u', "--upload-pack")),
        "branch" => rest
            .iter()
            .all(|arg| arg.starts_with('-') && !GIT_BRANCH_WRITE_FLAGS.contains(&arg.as_str())),
        "remote" => rest.iter().all(|arg| arg == "-v" || arg == "--verbose"),
        "tag" => rest.iter().all(|arg| arg == "-l" || arg == "--list"),
        "stash" => rest
            .first()
            .is_some_and(|verb| verb == "list" || verb == "show"),
        "config" => rest.iter().any(|arg| {
            ["--get", "--get-all", "--get-regexp", "--list", "-l"].contains(&arg.as_str())
        }),
        subcommand => GIT_READ_ONLY_SUBCOMMANDS.contains(&subcommand),
    }
}

/// Returns whether `arg` may pass the option `short` or `long` to a git command parsed by git's
/// option parser, which accepts short options bundled in one word (`-nO<program>`) and any
/// abbreviation of a long option (`--open=<program>`).
/// 返回 `arg` 是否可能向由 git 选项解析器解析的命令传入选项 `short` 或 `long`；该解析器接受合并在
/// 一个词中的短选项（`-nO<程序>`）以及长选项的任意缩写（`--open=<程序>`）。
///
/// A bundle counts whenever it contains `short`, even where the letter would be another option's
/// value, so the check can only err toward asking.
/// 只要合并词中含有 `short` 就计入，即使该字母实为其他选项的值，因此判断只会偏向询问。
fn git_option_given(arg: &str, short: char, long: &str) -> bool {
    if let Some(name) = arg.strip_prefix("--") {
        let name = name.split('=').next().unwrap_or(name);
        return !name.is_empty() && long[2..].starts_with(name);
    }
    arg.strip_prefix('-')
        .is_some_and(|bundle| bundle.contains(short))
}

/// Skips git's global options and returns the subcommand plus its arguments.
/// 跳过 git 全局选项，返回子命令及其参数。
fn git_subcommand(args: &[String]) -> Option<(String, &[String])> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "-C" || arg == "-c" || arg == "--git-dir" || arg == "--work-tree" {
            index += 2;
        } else if arg.starts_with('-') {
            index += 1;
        } else {
            return Some((arg.to_lowercase(), &args[index + 1..]));
        }
    }
    None
}

/// Normalizes one program word: strips directories and `.exe`, lower-cases it.
/// 规范化一个程序词：去掉目录与 `.exe` 并转为小写。
fn program_name(word: &str) -> String {
    let base = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let lowered = base.to_lowercase();
    lowered
        .strip_suffix(".exe")
        .map(str::to_string)
        .unwrap_or(lowered)
}

/// Collapses every whitespace run to one space.
/// 将每段连续空白压缩为一个空格。
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits `command` at unquoted separators and records redirections and substitutions.
/// 在未加引号的分隔符处切分 `command`，并记录重定向与命令替换。
fn scan_command(command: &str) -> CommandScan {
    let mut scan = CommandScan {
        segments: Vec::new(),
        writes_redirection: false,
        has_substitution: false,
    };
    let chars: Vec<char> = command.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut index = 0;
    let finish_word = |word: &mut String, words: &mut Vec<String>| {
        if !word.is_empty() {
            words.push(std::mem::take(word));
        }
    };
    while index < chars.len() {
        let current = chars[index];
        if let Some(open) = quote {
            if current == open {
                quote = None;
            } else {
                if open == '"'
                    && (current == '`' || current == '$' && chars.get(index + 1) == Some(&'('))
                {
                    scan.has_substitution = true;
                }
                word.push(current);
            }
            index += 1;
            continue;
        }
        match current {
            '\'' | '"' => quote = Some(current),
            // Backticks, `$(...)`, `<(...)`, and PowerShell `(...)`, `@(...)`, and `{...}` all
            // run nested code that the word rules cannot see.
            // 反引号、`$(...)`、`<(...)` 以及 PowerShell 的 `(...)`、`@(...)`、`{...}` 都会运行
            // 词规则无法看到的嵌套代码。
            '`' | '(' | '{' => scan.has_substitution = true,
            '|' | '&' | ';' | '\n' | '\r' => {
                finish_word(&mut word, &mut words);
                if !words.is_empty() {
                    scan.segments.push(std::mem::take(&mut words));
                }
            }
            '>' => {
                // A leading descriptor digit (`2>`) belongs to the redirection, not the word.
                // 前导描述符数字（`2>`）属于重定向而非词。
                if word.chars().all(|c| c.is_ascii_digit()) {
                    word.clear();
                }
                finish_word(&mut word, &mut words);
                let mut target_start = index + 1;
                if chars.get(target_start) == Some(&'>') {
                    target_start += 1;
                }
                while chars.get(target_start).is_some_and(|c| c.is_whitespace()) {
                    target_start += 1;
                }
                let mut target_end = target_start;
                // The target ends at whitespace or a separator; a leading `&` names a descriptor.
                // 目标在空白或分隔符处结束；开头的 `&` 表示描述符。
                while chars.get(target_end).is_some_and(|c| {
                    (*c == '&' && target_end == target_start)
                        || (!c.is_whitespace() && !"|;&".contains(*c))
                }) {
                    target_end += 1;
                }
                let target: String = chars[target_start..target_end].iter().collect();
                if !HARMLESS_REDIRECTION_TARGETS.contains(&target.to_lowercase().as_str()) {
                    scan.writes_redirection = true;
                }
                index = target_end;
                continue;
            }
            c if c.is_whitespace() => finish_word(&mut word, &mut words),
            c => word.push(c),
        }
        index += 1;
    }
    finish_word(&mut word, &mut words);
    if !words.is_empty() {
        scan.segments.push(words);
    }
    scan
}
