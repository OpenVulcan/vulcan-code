//! Deterministic shell-command rules: the known read-only list and the high-risk list.
//! 确定性 shell 命令规则：已知只读清单与高风险清单。
//!
//! Both lists are conservative text rules, not a shell parser. A command is read-only only
//! when every segment is a known read-only command with no write redirection or command
//! substitution; anything unrecognised is simply "not read-only" and goes through the
//! permission pipeline. A bare grouping `(...)` whose content is itself read-only, optionally
//! followed by property access such as `.Count`, counts as a read-only value.
//! The high-risk list marks commands a judge model may never allow alone.
//! 两份清单都是保守的文本规则，而不是 shell 解析器。只有每个片段都是已知只读命令、且不含写入
//! 重定向或命令替换时才算只读；无法识别的命令仅视为“非只读”并进入权限管线。内容本身只读的
//! 裸分组 `(...)`（可后接 `.Count` 等属性访问）视为只读值。高风险清单标记判定模型不能单独放行的命令。

use vulcan_host_tools::shell_host::HostCommandShell;

/// cmd built-ins whose arguments cannot launch an external program or modify files.
/// 参数不会启动外部程序或修改文件的 cmd 内建命令。
const CMD_READ_ONLY_BUILTINS: &[&str] =
    &["cd", "chdir", "dir", "echo", "rem", "type", "ver", "vol"];

/// Placeholder word for a read-only grouping; the NUL prefix keeps it apart from typed words.
/// 只读分组的占位词；NUL 前缀使其不会与输入的词混淆。
const READ_ONLY_GROUP_WORD: &str = "\u{0}group";

/// Commands that only read, whatever their arguments.
/// 无论参数如何都只读的命令。
const ALWAYS_READ_ONLY: &[&str] = &[
    // POSIX and GNU text and system tools.
    // POSIX 与 GNU 文本及系统工具。
    "[",
    "basename",
    "cal",
    "cat",
    "cd",
    "cmp",
    "column",
    "comm",
    "cut",
    "df",
    "diff",
    "dirname",
    "du",
    "echo",
    "egrep",
    "expand",
    "expr",
    "false",
    "fgrep",
    "fmt",
    "fold",
    "free",
    "grep",
    "groups",
    "head",
    "hexdump",
    "id",
    "jq",
    "locale",
    "ls",
    "lsof",
    "md5sum",
    "netstat",
    "nl",
    "nproc",
    "numfmt",
    "od",
    "paste",
    "pgrep",
    "pr",
    "printenv",
    "printf",
    "ps",
    "pwd",
    "readlink",
    "realpath",
    "rev",
    "seq",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "sleep",
    "stat",
    "strings",
    "tac",
    "tail",
    "test",
    "tr",
    "true",
    "tsort",
    "type",
    "uname",
    "unexpand",
    "uptime",
    "wc",
    "which",
    "whoami",
    // Windows console tools.
    // Windows 控制台工具。
    "dir",
    "findstr",
    "systeminfo",
    "tasklist",
    "ver",
    "vol",
    "where",
    // PowerShell cmdlets that only read state or reshape pipeline objects.
    // 只读取状态或整理管道对象的 PowerShell cmdlet。
    "compare-object",
    "convertfrom-csv",
    "convertfrom-json",
    "convertto-csv",
    "convertto-json",
    "format-list",
    "format-table",
    "format-wide",
    "get-acl",
    "get-alias",
    "get-childitem",
    "get-ciminstance",
    "get-command",
    "get-computerinfo",
    "get-content",
    "get-culture",
    "get-date",
    "get-executionpolicy",
    "get-filehash",
    "get-host",
    "get-item",
    "get-itemproperty",
    "get-itempropertyvalue",
    "get-location",
    "get-member",
    "get-module",
    "get-netipaddress",
    "get-nettcpconnection",
    "get-process",
    "get-psdrive",
    "get-psprovider",
    "get-service",
    "get-timezone",
    "get-unique",
    "get-uptime",
    "get-variable",
    "get-winevent",
    "group-object",
    "import-csv",
    "join-path",
    "measure-object",
    "out-host",
    "out-string",
    "pop-location",
    "push-location",
    "resolve-path",
    "select-object",
    "select-string",
    "select-xml",
    "set-location",
    "sort-object",
    "split-path",
    "start-sleep",
    "test-json",
    "test-path",
    "where-object",
    "write-host",
    "write-output",
    // PowerShell aliases of the cmdlets above.
    // 上述 cmdlet 的 PowerShell 别名。
    "chdir",
    "fl",
    "ft",
    "fw",
    "gal",
    "gc",
    "gci",
    "gcm",
    "gdr",
    "gi",
    "gl",
    "gm",
    "gp",
    "gps",
    "gpv",
    "group",
    "gsv",
    "gu",
    "gv",
    "measure",
    "popd",
    "pushd",
    "rvpa",
    "select",
    "sl",
    "sls",
];

/// Commands that only read unless given one of the listed options, which write a file, change
/// system state, or run another program. A short option (`-o`) also matches inside a bundle
/// (`-uo`), a long option (`--output`) matches any abbreviation, and a slash option (`/o`) matches
/// case-insensitively by prefix.
/// 除非带有所列选项（写入文件、改变系统状态或运行其他程序），否则只读的命令。短选项（`-o`）
/// 也匹配合并写法（`-uo`），长选项（`--output`）匹配任意缩写，斜杠选项（`/o`）不区分大小写按
/// 前缀匹配。
const READ_ONLY_UNLESS: &[(&str, &[&str])] = &[
    ("base64", &["-o", "--output"]),
    ("file", &["-C", "--compile"]),
    ("sort", &["-o", "--output", "--compress-program", "/o"]),
    ("ss", &["-K", "--kill"]),
    ("tree", &["-o", "-R"]),
];

/// Options under which `date` only formats or reads the clock; anything else may set it.
/// `date` 只格式化或读取时钟的选项；其他选项可能设置时钟。
const DATE_READ_OPTIONS: &[&str] = &[
    "-u",
    "--utc",
    "--universal",
    "-R",
    "--rfc-email",
    "-I",
    "--iso-8601",
    "--rfc-3339",
    "-d",
    "--date",
    "-r",
    "--reference",
];

/// Options under which `hostname` only reports names and addresses.
/// `hostname` 只报告名称与地址的选项。
const HOSTNAME_READ_OPTIONS: &[&str] = &[
    "-a",
    "--alias",
    "-A",
    "--all-fqdns",
    "-d",
    "--domain",
    "-f",
    "--fqdn",
    "--long",
    "-i",
    "--ip-address",
    "-I",
    "--all-ip-addresses",
    "-s",
    "--short",
    "-y",
    "--yp",
    "--nis",
    "-V",
    "--version",
    "-h",
    "--help",
];

/// `docker` subcommands that only report state when given no global options.
/// 未带全局选项时只报告状态的 `docker` 子命令。
const DOCKER_READ_ONLY_SUBCOMMANDS: &[&str] = &["images", "info", "ps", "version"];

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
    /// Whether the text contains command substitution or an expansion we do not interpret.
    /// 文本是否含有命令替换或我们不解释的展开。
    has_substitution: bool,
    /// Whether the shell may quote differently than the scan (escaped quotes, comments,
    /// here-documents), so quoted spans and separators may be wrong.
    /// shell 的引号划分是否可能与扫描不同（转义引号、注释、here 文档），因而引号范围与分隔符
    /// 可能有误。
    ambiguous_quoting: bool,
}

/// Returns whether every segment of `command` is known read-only under the selected `shell`.
/// 返回 `command` 的每个片段在所选 `shell` 下是否都是已知只读命令。
pub fn is_known_read_only(command: &str, shell: HostCommandShell) -> bool {
    let scan = scan_command(command, Some(shell));
    if scan.writes_redirection
        || scan.has_substitution
        || scan.ambiguous_quoting
        || scan.segments.is_empty()
    {
        return false;
    }
    scan.segments
        .iter()
        .all(|words| segment_is_read_only(words, shell))
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
    let scan = scan_command(command, None);
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
fn segment_is_read_only(words: &[String], shell: HostCommandShell) -> bool {
    let Some(first) = words.first() else {
        return true;
    };
    // A read-only grouping only yields a value; anything after it would be a new command.
    // 只读分组只产生一个值；其后的任何内容都会成为新的命令。
    if first == READ_ONLY_GROUP_WORD {
        return words.len() == 1;
    }
    // Explicit paths may name workspace-owned impostors; only high-risk matching strips paths.
    // 显式路径可能指向工作区中的冒名程序；只有高风险匹配才去掉路径。
    if first.contains(['/', '\\']) {
        return false;
    }
    // cmd searches the working directory before PATH, so even bare external names are untrusted.
    // cmd 先搜索当前目录再搜索 PATH，因此裸外部程序名同样不可信。
    if shell == HostCommandShell::Cmd {
        return CMD_READ_ONLY_BUILTINS.contains(&first.to_ascii_lowercase().as_str());
    }
    // PowerShell variables: a lone `$name.Property` only yields its value, and `$name = <read-only>`
    // stores one; preference and scope-qualified variables change how later commands run.
    // PowerShell 变量：单独的 `$name.Property` 只产生其值，`$name = <只读命令>` 只保存一个值；
    // 偏好变量与带作用域的变量会改变之后命令的运行方式。
    if let Some(name) = first.strip_prefix('$') {
        if !matches!(shell, HostCommandShell::Pwsh | HostCommandShell::Powershell) {
            return false;
        }
        if words.len() == 1 {
            return !name.contains('=');
        }
        return words[1] == "="
            && words.len() > 2
            && plain_variable_name(name)
            && segment_is_read_only(&words[2..], shell);
    }
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
        "date" => date_is_read_only(args),
        // Any operand, or an option outside the reporting set, sets the host name.
        // 任何操作数或报告集合以外的选项都会设置主机名。
        "hostname" => args
            .iter()
            .all(|arg| HOSTNAME_READ_OPTIONS.contains(&arg.as_str())),
        // A second operand names the output file.
        // 第二个操作数指定输出文件。
        "uniq" => uniq_operand_count(args) <= 1,
        "ipconfig" => args.iter().all(|arg| arg.eq_ignore_ascii_case("/all")),
        "docker" => args
            .first()
            .is_some_and(|subcommand| DOCKER_READ_ONLY_SUBCOMMANDS.contains(&subcommand.as_str())),
        program => {
            if let Some((_, write_options)) =
                READ_ONLY_UNLESS.iter().find(|(name, _)| *name == program)
            {
                return !args
                    .iter()
                    .any(|arg| write_options.iter().any(|option| option_given(arg, option)));
            }
            ALWAYS_READ_ONLY.contains(&program)
        }
    }
}

/// Returns whether `name` is an ordinary PowerShell variable whose assignment affects no command.
/// 返回 `name` 是否为赋值不影响任何命令的普通 PowerShell 变量。
fn plain_variable_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !lowered.ends_with("preference")
        && !lowered.starts_with("ps")
}

/// Returns whether `arg` passes `option` in the conventions of [`READ_ONLY_UNLESS`].
/// 按 [`READ_ONLY_UNLESS`] 的约定返回 `arg` 是否传入了 `option`。
fn option_given(arg: &str, option: &str) -> bool {
    if let Some(long) = option.strip_prefix("--") {
        let Some(name) = arg.strip_prefix("--") else {
            return false;
        };
        let name = name.split('=').next().unwrap_or(name);
        return !name.is_empty() && long.starts_with(name);
    }
    if let Some(short) = option.strip_prefix('-') {
        return !arg.starts_with("--")
            && arg
                .strip_prefix('-')
                .is_some_and(|bundle| short.chars().all(|flag| bundle.contains(flag)));
    }
    arg.to_ascii_lowercase()
        .starts_with(option.to_ascii_lowercase().as_str())
}

/// Returns whether one `date` call only prints: `+FORMAT` operands and reporting options only.
/// 返回一次 `date` 调用是否只打印：只含 `+FORMAT` 操作数与报告类选项。
fn date_is_read_only(args: &[String]) -> bool {
    let mut words = args.iter();
    while let Some(arg) = words.next() {
        if arg.starts_with('+') {
            continue;
        }
        let name = arg.split('=').next().unwrap_or(arg);
        if !DATE_READ_OPTIONS.contains(&name) {
            return false;
        }
        // `-d`, `--date`, `-r` and `--reference` take a value as the next word.
        // `-d`、`--date`、`-r` 与 `--reference` 以下一个词作为取值。
        if !arg.contains('=') && ["-d", "--date", "-r", "--reference"].contains(&name) {
            words.next();
        }
    }
    true
}

/// Counts the file operands of one `uniq` call, skipping the values of `-f`, `-s` and `-w`.
/// 统计一次 `uniq` 调用的文件操作数，跳过 `-f`、`-s` 与 `-w` 的取值。
fn uniq_operand_count(args: &[String]) -> usize {
    let mut count = 0;
    let mut words = args.iter();
    while let Some(arg) = words.next() {
        if ["-f", "-s", "-w"].contains(&arg.as_str()) {
            words.next();
        } else if arg == "-" || !arg.starts_with('-') {
            count += 1;
        }
    }
    count
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

/// Returns the index just past a bare grouping that opens at `open` when its content is a known
/// read-only command and only property access (`.Count`) follows before the word ends.
/// 当从 `open` 开始的裸分组内容为已知只读命令、且词结束前只跟属性访问（`.Count`）时，返回紧随
/// 该分组之后的下标。
///
/// Any other shape, such as a method call, an index, or an unmatched bracket, returns `None` so the
/// caller keeps treating the text as nested code.
/// 其他形态（方法调用、索引或括号不匹配）返回 `None`，调用方继续将该文本视为嵌套代码。
fn read_only_group_end(chars: &[char], open: usize, shell: HostCommandShell) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut close = None;
    for (offset, current) in chars[open..].iter().enumerate() {
        if let Some(open_quote) = quote {
            if *current == open_quote {
                quote = None;
            }
            continue;
        }
        match current {
            '\'' | '"' => quote = Some(*current),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let inner: String = chars[open + 1..close].iter().collect();
    if !is_known_read_only(&inner, shell) {
        return None;
    }
    let mut end = close + 1;
    while chars.get(end) == Some(&'.')
        && chars
            .get(end + 1)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
    {
        end += 1;
        while chars
            .get(end)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
        {
            end += 1;
        }
    }
    match chars.get(end) {
        None => Some(end),
        Some(c) if c.is_whitespace() || "|;&".contains(*c) => Some(end),
        Some(_) => None,
    }
}

/// Splits `command` at unquoted separators and records redirections and substitutions.
/// 在未加引号的分隔符处切分 `command`，并记录重定向与命令替换。
fn scan_command(command: &str, shell: Option<HostCommandShell>) -> CommandScan {
    let mut scan = CommandScan {
        segments: Vec::new(),
        writes_redirection: false,
        has_substitution: false,
        ambiguous_quoting: false,
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
        // Reject syntax whose quoting differs across shells instead of guessing its effects.
        // Comments (`#`, PowerShell `<#`), here-documents (`<<`) and PowerShell here-strings
        // (`@'`, `@"`) hold quote characters the shell ignores, which would shift every later
        // quoted span; cmd has no `#` comments.
        // 拒绝跨 shell 引号语义不一致的语法，不猜测其影响。注释（`#`、PowerShell `<#`）、here 文档
        // （`<<`）与 PowerShell here 字符串（`@'`、`@"`）含有 shell 忽略的引号字符，会使之后所有
        // 引号范围错位；cmd 没有 `#` 注释。
        let unquoted = quote.is_none();
        if current == '\\' && matches!(chars.get(index + 1), Some('\'' | '"'))
            || matches!(current, '\u{2018}'..='\u{201b}' | '\u{201c}'..='\u{201e}')
            || unquoted && current == '^'
            || unquoted
                && current == '#'
                && shell != Some(HostCommandShell::Cmd)
                && (word.is_empty() || index > 0 && chars[index - 1] == '<')
            || unquoted && current == '<' && chars.get(index + 1) == Some(&'<')
            || unquoted && current == '@' && matches!(chars.get(index + 1), Some('\'' | '"'))
        {
            scan.ambiguous_quoting = true;
        }
        if shell == Some(HostCommandShell::Cmd)
            && (matches!(current, '%' | '!') || current == '"' && words.is_empty())
        {
            scan.has_substitution = true;
        }
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
            '"' => quote = Some(current),
            '\'' if shell != Some(HostCommandShell::Cmd) => quote = Some(current),
            '(' if word.is_empty() => {
                if let Some(shell @ (HostCommandShell::Pwsh | HostCommandShell::Powershell)) = shell
                    && let Some(end) = read_only_group_end(&chars, index, shell)
                {
                    words.push(String::from(READ_ONLY_GROUP_WORD));
                    index = end;
                    continue;
                }
                scan.has_substitution = true;
            }
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
                if !read_only_redirection_target(&target, shell) {
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
    // An unterminated quote is not evidence of a read-only command.
    // 未闭合的引号不能作为只读命令的依据。
    scan.ambiguous_quoting |= quote.is_some();
    finish_word(&mut word, &mut words);
    if !words.is_empty() {
        scan.segments.push(words);
    }
    scan
}

/// Returns whether `target` merges streams or discards output in the selected `shell`.
/// 返回 `target` 是否在所选 `shell` 中合并流或丢弃输出。
/// A sink spelling can create a normal file in another shell; unknown shells grant no sink.
/// 丢弃目标的拼写在其他 shell 中可能创建普通文件；未知 shell 不认可丢弃目标。
fn read_only_redirection_target(target: &str, shell: Option<HostCommandShell>) -> bool {
    matches!(target, "&1" | "&2")
        || match shell {
            Some(HostCommandShell::Cmd) => target.eq_ignore_ascii_case("nul"),
            Some(HostCommandShell::Pwsh | HostCommandShell::Powershell) => {
                target.eq_ignore_ascii_case("$null")
            }
            Some(HostCommandShell::Bash | HostCommandShell::Zsh | HostCommandShell::Sh) => {
                target == "/dev/null"
            }
            None => false,
        }
}
