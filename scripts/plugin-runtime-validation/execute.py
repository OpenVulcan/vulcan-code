"""Prepare, build or run only the bounded full-host consumers.
仅准备、构建或运行有限完整宿主消费者。
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

# Exact compiler toolchain required for both independent consumers; never use the system default.
# 两个独立消费者要求的精确编译工具链；绝不使用系统默认值。
EXACT_TOOLCHAIN = "+1.97.1"


def digest(path):
    """Hash the exact bytes at path; return its SHA-256 without normalizing source.
    散列 path 的精确字节；返回 SHA-256，不规范化源码。
    """
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(root, paths, revision_is_build_input):
    """Hash root/paths and scoped dirt; retain revision only when revision_is_build_input says build scripts consume it.
    散列 root、paths 及对应脏范围；仅在 revision_is_build_input 声明构建脚本消费修订时保留修订。
    """
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    listed = subprocess.run(["rtk", "proxy", "git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", *paths], capture_output=True, check=True)
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    files = sorted(set(item.decode("utf-8") for item in listed.stdout.split(b"\0") if item))
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    hashes = {name: digest(root / name) for name in files if (root / name).is_file()}
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    revision = subprocess.run(["rtk", "proxy", "git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    dirty = subprocess.run(["rtk", "proxy", "git", "-C", str(root), "status", "--porcelain=v1", "--untracked-files=all", "--", *paths], capture_output=True, text=True, check=True).stdout
    return {"root": str(root), "revision": revision if revision_is_build_input else None, "dirty": dirty, "files": hashes}


def evidence(roots, configs, records):
    """Freeze roots/configs, explicit extra records and driver bytes; return reproducible pre-run evidence.
    冻结 roots、configs、显式附加记录及驱动字节；返回可复现运行前证据。
    """
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    scoped = ["Cargo.toml", "Cargo.lock", "crates", "workspace-hack", "plugin-bridge", ".cargo/config.toml"]
    # Freeze the exact ignored native protoc installation consumed by both builds, including its import files.
    # 冻结两个构建消费的精确被忽略原生 protoc 安装，包含其导入文件。
    native_root = roots["main"] / ".cargo/protoc"
    # Resolve the executable name from the actual platform; require the configured native installation to exist.
    # 从实际平台解析可执行文件名；要求已配置原生安装存在。
    native_executable = native_root / "bin" / ("protoc.exe" if os.name == "nt" else "protoc")
    if not native_executable.is_file() or not (native_root / "include").is_dir():
        raise ValueError("Configured native protoc executable and include directory are required.")
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    driver = Path(__file__).resolve().parent
    return {"toolchain": EXACT_TOOLCHAIN, "main": source(roots["main"], scoped, False), "host": source(roots["host"], scoped, False), "upstream": source(roots["upstream"], [], True), "native_protoc": {str(path): digest(path) for path in sorted(native_root.rglob("*")) if path.is_file()}, "driver": {str(path.relative_to(driver)): digest(path) for path in sorted(driver.rglob("*")) if path.is_file() and "__pycache__" not in path.parts}, "configs": [{"path": str(path), "sha256": digest(path)} for path in configs], "records": {str(path): digest(path) for path in records}}


def unique_package(packages, name):
    """Require exactly one package named name in packages; return it or reject ambiguous resolution.
    要求 packages 中名为 name 的包恰有一个；返回该包或拒绝歧义解析。
    """
    # Duplicate Core instances can make direct and host dependencies consume different implementations.
    # 重复 Core 实例可能使直接依赖和宿主依赖消费不同实现。
    matches = [package for package in packages if package["name"] == name]
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one {name} package; resolved {len(matches)}.")
    return matches[0]


def lock_identity(package):
    """Return package's Cargo.lock identity, including optional source/checksum declared by the lock format.
    返回 package 的 Cargo.lock 身份，包含锁格式声明为可选的 source/checksum。
    """
    return (package["name"], package["version"], package.get("source"), package.get("checksum"))


def verify_consumer(metadata, seed, final_lock, consumer, destination, owner, upstream):
    """Verify actual metadata/locks for consumer at destination against seed, owner and upstream; return evidence.
    将 destination 中 consumer 的真实 metadata/锁与 seed、owner、upstream 核验；返回来源证据。
    """
    # Every retained dependency must keep its frozen version, source and checksum; seed pruning is allowed.
    # 每个保留依赖必须维持冻结版本、来源和校验和；允许裁剪不再使用的起点依赖。
    seed_identities = {lock_identity(package) for package in seed["package"]}
    # Only the new standalone root package is absent from its original host seed lock.
    # 仅新建独立根包不存在于原宿主起点锁。
    consumer_name = "plugin-runtime-validation-" + consumer
    # Preserve unexpected identities before rejecting them instead of silently accepting lock normalization.
    # 拒绝前保留意外身份，不静默接受锁规范化带来的依赖变化。
    unexpected = [list(lock_identity(package)) for package in final_lock["package"] if lock_identity(package) not in seed_identities and lock_identity(package) != (consumer_name, "0.0.0", None, None)]
    if unexpected:
        raise ValueError(f"Unexpected dependency identity changes for {consumer}: {json.dumps(unexpected)}")
    # Metadata describes actual source paths and graph; checksums come only from the corresponding lock.
    # metadata 描述真实来源路径与依赖图；校验和仅来自对应锁文件。
    core = unique_package(metadata["packages"], "luaskills")
    # Select the same uniquely resolved Core identity in final and frozen seed locks.
    # 在最终锁和冻结起点锁中选择同一个唯一 Core 身份。
    locked_core = unique_package(final_lock["package"], "luaskills")
    # The host seed is authoritative for expected registry provenance or the local candidate version.
    # 宿主起点锁是预期注册表来源或本地候选版本的权威。
    seed_core = unique_package(seed["package"], "luaskills")
    if core["version"] != locked_core["version"] or core["source"] != locked_core.get("source"):
        raise ValueError("Core metadata and final Cargo.lock disagree on version/source.")
    if consumer == "old":
        if core["version"] != "0.5.9" or core["source"] != "registry+https://github.com/rust-lang/crates.io-index" or lock_identity(locked_core) != lock_identity(seed_core):
            raise ValueError("Legacy consumer must use the original registered Core 0.5.9 and frozen checksum.")
        if "checksum" not in seed_core or not seed_core["checksum"]:
            raise ValueError("The original registered Core seed must include a checksum.")
    elif consumer == "new":
        if core["version"] != "0.6.0" or core["source"] is not None or Path(core["manifest_path"]).resolve() != upstream / "Cargo.toml" or lock_identity(locked_core) != lock_identity(seed_core):
            raise ValueError("Candidate consumer must use the explicit local Core 0.6.0 manifest and seed identity.")
    else:
        raise ValueError(f"Unknown consumer: {consumer}")
    # The runtime must be the explicit old/new host, rather than an unrelated package with the same name.
    # 运行时必须来自明确的旧/新宿主，不能由同名无关包替代。
    runtime = unique_package(metadata["packages"], "vulcan-plugin-runtime")
    if runtime["source"] is not None or Path(runtime["manifest_path"]).resolve() != owner / "crates/vulcan-plugin-runtime/Cargo.toml":
        raise ValueError("Resolved plugin runtime is not owned by the declared host.")
    # The independent root and its runtime must both point directly to this exact Core package ID.
    # 独立根包及其运行时必须直接指向此精确 Core 包 ID。
    root_package = unique_package(metadata["packages"], consumer_name)
    if root_package["version"] != "0.0.0" or root_package["source"] is not None or Path(root_package["manifest_path"]).resolve() != destination / "Cargo.toml" or metadata["resolve"]["root"] != root_package["id"]:
        raise ValueError("Metadata resolve root is not the explicit standalone consumer.")
    # Retain concrete dependency edges, including dependency kinds, as auditable source evidence.
    # 保留具体依赖边及依赖种类，作为可审核的来源证据。
    edges = {}
    for package in [root_package, runtime]:
        # Resolve nodes use the actual package ID, not package name or an inferred positional index.
        # 解析节点使用真实包 ID，不使用包名称或推测的位置下标。
        nodes = [node for node in metadata["resolve"]["nodes"] if node["id"] == package["id"]]
        if len(nodes) != 1:
            raise ValueError(f"Expected one resolve node for {package['id']}.")
        # A renamed or competing Core dependency must not silently satisfy the direct dependency proof.
        # 重命名或竞争 Core 依赖不得静默满足直接依赖证明。
        core_edges = [edge for edge in nodes[0]["deps"] if edge["name"] == "luaskills"]
        if len(core_edges) != 1 or core_edges[0]["pkg"] != core["id"] or core["id"] not in nodes[0]["dependencies"]:
            raise ValueError(f"Direct luaskills edge from {package['id']} does not resolve to the unique Core.")
        edges[package["id"]] = core_edges[0]
    return {"consumer": consumer, "core": {"id": core["id"], "version": core["version"], "source": core["source"], "manifest_path": core["manifest_path"], "checksum": locked_core.get("checksum")}, "runtime": {"id": runtime["id"], "manifest_path": runtime["manifest_path"]}, "root": root_package["id"], "direct_core_edges": edges, "lock_normalization": {"unexpected": unexpected, "removed_seed_identities": sorted([list(identity) for identity in seed_identities - {lock_identity(package) for package in final_lock["package"]}], key=lambda identity: json.dumps(identity))}}


def resolve_consumer(destination, consumer, owner, upstream, environment):
    """Normalize destination's seed offline, verify locked full metadata for consumer/owner/upstream; return evidence.
    离线规范化 destination 起点锁，核验 consumer/owner/upstream 的锁定完整 metadata；返回证据。
    """
    # Keep resolution and locked verification separate so normalization cannot be mistaken for a locked build.
    # 分离解析与锁定核验，避免把规范化误当作锁定构建。
    command = ["rtk", "proxy", "cargo", EXACT_TOOLCHAIN, "metadata", "--format-version", "1", "--offline", "--manifest-path", str(destination / "Cargo.toml")]
    for phase, flags in [("resolution", []), ("locked", ["--locked"])]:
        # Preserve actual stdout, stderr and exact arguments even when Cargo rejects a source or lock.
        # 即使 Cargo 拒绝来源或锁，也保留真实输出、错误输出及精确参数。
        result = subprocess.run([*command, *flags], cwd=owner, env=environment, capture_output=True)
        (destination / (phase + "-metadata.json")).write_bytes(result.stdout)
        (destination / (phase + "-metadata-stderr.txt")).write_bytes(result.stderr)
        (destination / (phase + "-metadata-command.json")).write_text(json.dumps({"argv": [*command, *flags], "exit_code": result.returncode}, indent=2), encoding="utf-8")
        result.check_returncode()
    # Use complete, actual locked metadata and both retained lock files without reconstructing Cargo output.
    # 使用完整真实锁定 metadata 及两份保留锁文件，不重建 Cargo 输出。
    metadata = json.loads((destination / "locked-metadata.json").read_bytes())
    # Read the immutable copied seed, rather than the Cargo-normalized active lock.
    # 读取不可变的已复制起点锁，不读取被 Cargo 规范化的活动锁。
    seed = tomllib.loads((destination / "Cargo.seed.lock").read_text(encoding="utf-8"))
    # Capture the normalized active lock that the locked build will consume.
    # 读取锁定构建将消费的规范化活动锁。
    final_lock = tomllib.loads((destination / "Cargo.lock").read_text(encoding="utf-8"))
    # Verification rejection leaves all source and resolution artifacts intact for diagnosis.
    # 核验拒绝时保留全部来源及解析产物供诊断。
    proof = verify_consumer(metadata, seed, final_lock, consumer, destination, owner, upstream)
    (destination / "consumer-source-proof.json").write_text(json.dumps(proof, ensure_ascii=False, indent=2), encoding="utf-8")
    return proof


def process_budget_ms(workload):
    """Derive the outer process deadline from frozen workload, invocation and cleanup limits; return milliseconds.
    从冻结负载、调用及清理上限派生外层进程截止时间；返回毫秒。
    """
    return workload["maximum_duration_ms"] + (workload["owner_cycles"] + workload["concurrency"] + 2) * workload["cleanup_timeout_ms"] + workload["timeout_ms"] * (workload["owner_cycles"] + workload["calls_per_worker"])


def main():
    """Parse explicit roots/action/configs and execute one finite phase; errors terminate nonzero.
    解析显式 roots、action、configs 并执行单个有限阶段；错误以非零状态终止。
    """
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "build", "run"])
    parser.add_argument("--main-root", type=Path, required=True)
    parser.add_argument("--host-root", type=Path, required=True)
    parser.add_argument("--upstream-root", type=Path, required=True)
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--config", type=Path, action="append", required=True)
    parser.add_argument("--evidence-file", type=Path, action="append", default=[])
    parser.add_argument("--windows-sampling-gates", type=Path)
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    args = parser.parse_args()
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    roots = {"main": args.main_root.resolve(strict=True), "host": args.host_root.resolve(strict=True), "upstream": args.upstream_root.resolve(strict=True)}
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    configs = [path.resolve(strict=True) for path in args.config]
    # Optional explicitly named records freeze gates, machine observations or resource samplers without executing them.
    # 可选显式命名记录冻结门槛、机器观察或资源采样器，不执行这些记录。
    records = [path.resolve(strict=True) for path in args.evidence_file]
    # Optional Windows sampling executes only the explicit gate already included in the frozen records.
    # 可选 Windows 采样仅执行已包含于冻结记录的显式门禁。
    sampling_gates = args.windows_sampling_gates.resolve(strict=True) if args.windows_sampling_gates else None
    if sampling_gates is not None and (os.name != "nt" or sampling_gates not in records):
        raise ValueError("Windows sampling requires Windows and the exact gate in --evidence-file.")
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    run_dir = args.run_dir.resolve()
    if any(run_dir.is_relative_to(root) for root in roots.values()):
        raise ValueError("Run directory must be outside all source trees.")
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    frozen = run_dir / "source-freeze.json"
    # Check the exact copied Rust and expanded manifest inputs that Cargo actually consumes.
    # 检查 Cargo 实际消费的精确复制 Rust 及展开清单输入。
    generated_paths = [run_dir / "common.rs", *[run_dir / consumer / name for consumer in ["old", "new"] for name in ["main.rs", "Cargo.toml", "Cargo.seed.lock"]]]
    # Include immutable seed copies; only the active Cargo.lock may be normalized before final dependency freezing.
    # 包含不可变起点副本；仅活动 Cargo.lock 可在最终依赖冻结前被规范化。
    generated_freeze = run_dir / "generated-source-freeze.json"
    # Freeze actual locks, full metadata, provenance and executables; later runs require identical evidence bytes.
    # 冻结真实锁、完整 metadata、来源和可执行文件；后续运行要求证据字节完全相同。
    dependency_paths = [run_dir / consumer / name for consumer in ["old", "new"] for name in ["Cargo.seed.lock", "Cargo.lock", "resolution-metadata.json", "locked-metadata.json", "consumer-source-proof.json"]] + [run_dir / consumer / "target/release" / ("plugin-runtime-validation-" + consumer + (".exe" if os.name == "nt" else "")) for consumer in ["old", "new"]]
    # Final dependency evidence is independent of the host locks copied only as resolution seeds.
    # 最终依赖证据独立于仅作为解析起点复制的宿主锁。
    dependency_freeze = run_dir / "dependency-freeze.json"
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    current = evidence(roots, configs, records)
    current["windows_sampling_gates"] = str(sampling_gates) if sampling_gates is not None else None
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    driver = Path(__file__).resolve().parent
    if args.action == "prepare":
        run_dir.mkdir(parents=True, exist_ok=False)
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        for consumer, owner in [("old", "main"), ("new", "host")]:
            # Retain the exact local input or evidence for this finite phase.
            # 为此有限阶段保留精确本地输入或证据。
            destination = run_dir / consumer
            destination.mkdir()
            shutil.copyfile(driver / consumer / "main.rs", destination / "main.rs")
            # Seed from the exact frozen host lock; standalone resolution must not silently choose newer dependencies.
            # 从精确冻结宿主锁开始；独立解析不得静默选择更新依赖。
            shutil.copyfile(roots[owner] / "Cargo.lock", destination / "Cargo.lock")
            shutil.copyfile(roots[owner] / "Cargo.lock", destination / "Cargo.seed.lock")
            # Retain the exact local input or evidence for this finite phase.
            # 为此有限阶段保留精确本地输入或证据。
            template = (driver / consumer / "Cargo.toml").read_text(encoding="utf-8")
            # Retain the exact local input or evidence for this finite phase.
            # 为此有限阶段保留精确本地输入或证据。
            for name, root in roots.items():
                # Retain the exact local input or evidence for this finite phase.
                # 为此有限阶段保留精确本地输入或证据。
                template = template.replace("@" + name.upper() + "_ROOT@", root.as_posix())
            (destination / "Cargo.toml").write_text(template, encoding="utf-8")
        shutil.copyfile(driver / "common.rs", run_dir / "common.rs")
        frozen.write_text(json.dumps(current, ensure_ascii=False, indent=2), encoding="utf-8")
        generated_freeze.write_text(json.dumps({str(path.relative_to(run_dir)): digest(path) for path in generated_paths}, indent=2), encoding="utf-8")
        # Preserve complete repository dirt as context without making unrelated non-input Web or bytecode changes build inputs.
        # 保存完整仓库脏范围作为上下文，不将无关非输入 Web 或字节码变化当作构建输入。
        (run_dir / "git-context.json").write_text(json.dumps({name: {"revision": subprocess.run(["rtk", "proxy", "git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip(), "dirty": subprocess.run(["rtk", "proxy", "git", "-C", str(root), "status", "--porcelain=v1", "--untracked-files=all"], capture_output=True, text=True, check=True).stdout} for name, root in roots.items()}, ensure_ascii=False, indent=2), encoding="utf-8")
        print(json.dumps({"prepared": str(run_dir), "freeze": str(frozen)}, ensure_ascii=False))
        return
    if json.loads(frozen.read_text(encoding="utf-8")) != current:
        raise ValueError("Source/config/driver differs from the frozen preparation; create a fresh run directory before measuring.")
    if json.loads(generated_freeze.read_text(encoding="utf-8")) != {str(path.relative_to(run_dir)): digest(path) for path in generated_paths}:
        raise ValueError("Actual generated Rust or manifest inputs differ from their frozen preparation.")
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    environment = dict(os.environ, CARGO_BUILD_JOBS="4", RUST_TEST_THREADS="4", PROTOC=str(roots["main"] / ".cargo/protoc/bin" / ("protoc.exe" if os.name == "nt" else "protoc")), PROTOC_INCLUDE=str(roots["main"] / ".cargo/protoc/include"))
    if args.action == "build":
        # Reject source/graph or dependency drift for both consumers before starting either compilation.
        # 在任一编译开始前，拒绝两个消费者的来源/依赖图或依赖身份漂移。
        for consumer, owner in [("old", "main"), ("new", "host")]:
            resolve_consumer(run_dir / consumer, consumer, roots[owner], roots["upstream"], environment)
        # Compilation must preserve the exact normalized dependencies and source proof just verified.
        # 编译必须保留刚核验的精确规范化依赖及来源证明。
        resolved_hashes = {str(path.relative_to(run_dir)): digest(path) for path in dependency_paths if path.name in ["Cargo.seed.lock", "Cargo.lock", "resolution-metadata.json", "locked-metadata.json", "consumer-source-proof.json"]}
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        for consumer, owner in [("old", "main"), ("new", "host")]:
            environment["VALIDATION_BRIDGE_SOURCE"] = str(roots[owner] / "plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua")
            subprocess.run(["rtk", "proxy", "cargo", EXACT_TOOLCHAIN, "build", "--locked", "--offline", "--release", "--jobs", "4", "--target-dir", str(run_dir / consumer / "target"), "--manifest-path", str(run_dir / consumer / "Cargo.toml")], cwd=roots[owner], env=environment, check=True)
        # Repeat the exact generation check after compilation before attesting any executable.
        # 在编译之后再次核验精确生成输入，随后才证明任何可执行文件。
        if json.loads(generated_freeze.read_text(encoding="utf-8")) != {str(path.relative_to(run_dir)): digest(path) for path in generated_paths}:
            raise ValueError("Actual generated Rust or manifest inputs changed during compilation; no build freeze is published.")
        # Preserve the explicit sampler selection in the same post-build comparison.
        # 在同一编译后比较中保留显式采样器选择。
        completed_inputs = evidence(roots, configs, records)
        completed_inputs["windows_sampling_gates"] = str(sampling_gates) if sampling_gates is not None else None
        if completed_inputs != current:
            raise ValueError("Actual source/config/driver inputs changed during compilation; no build freeze is published.")
        if resolved_hashes != {str(path.relative_to(run_dir)): digest(path) for path in dependency_paths if path.name in ["Cargo.seed.lock", "Cargo.lock", "resolution-metadata.json", "locked-metadata.json", "consumer-source-proof.json"]}:
            raise ValueError("Verified consumer dependencies or source evidence changed during compilation; no build freeze is published.")
        dependency_freeze.write_text(json.dumps({str(path.relative_to(run_dir)): digest(path) for path in dependency_paths}, indent=2), encoding="utf-8")
        return
    if json.loads(dependency_freeze.read_text(encoding="utf-8")) != {str(path.relative_to(run_dir)): digest(path) for path in dependency_paths}:
        raise ValueError("Actual consumer dependency locks, metadata, source proofs or binaries differ from the completed build freeze.")
    # Retain the exact local input or evidence for this finite phase.
    # 为此有限阶段保留精确本地输入或证据。
    for index, config in enumerate(configs):
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        workload = json.loads(config.read_text(encoding="utf-8"))
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        consumer = "old" if workload["group"] == "legacy" else "new"
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        executable = run_dir / consumer / "target/release" / ("plugin-runtime-validation-" + consumer + (".exe" if os.name == "nt" else ""))
        # Retain the exact local input or evidence for this finite phase.
        # 为此有限阶段保留精确本地输入或证据。
        output = run_dir / (f"{index:03d}-" + config.stem + ".jsonl")
        if sampling_gates is not None:
            # The sampler owns the original process through cleanup even when observational sampling fails.
            # 即使观察采样失败，采样器也持续拥有原进程直至清理。
            subprocess.run(["rtk", "proxy", "powershell", "-NoProfile", "-File", str(driver / "sample-windows.ps1"), "-Executable", str(executable), "-Configuration", str(config), "-InvocationOutput", str(output), "-ResourceOutput", str(output.with_suffix(".resources.jsonl")), "-Stdout", str(output.with_suffix(".stdout.txt")), "-Stderr", str(output.with_suffix(".stderr.txt")), "-Gates", str(sampling_gates), "-MaximumProcessDurationMs", str(process_budget_ms(workload))], cwd=run_dir, env=environment, check=True)
        else:
            subprocess.run(["rtk", "proxy", str(executable), str(config), str(output)], cwd=run_dir, env=environment, check=True, timeout=process_budget_ms(workload) / 1000)


if __name__ == "__main__":
    main()
