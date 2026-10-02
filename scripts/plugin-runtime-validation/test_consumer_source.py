"""Reject misleading performance baselines using Cargo's actual metadata and lock shapes.
使用 Cargo 真实 metadata 与锁结构拒绝误导性性能基线。
"""

import copy
from pathlib import Path
import unittest

from execute import verify_consumer


class ConsumerSourceTests(unittest.TestCase):
    """Exercise source, dependency-edge and frozen-identity rejection; no Cargo or performance execution.
    验证来源、依赖边及冻结身份拒绝；不执行 Cargo 或性能测试。
    """

    def fixture(self, consumer):
        """Build Cargo-shaped metadata and locks for old/new consumer; return verification arguments.
        为旧/新 consumer 构造 Cargo 结构的 metadata 与锁；返回核验参数。
        """
        # Absolute manifests model Cargo's resolved paths without requiring a real workspace.
        # 绝对清单路径模拟 Cargo 解析路径，不要求真实工作区。
        destination, owner, upstream = [Path(name).resolve() for name in ["fixture-consumer", "fixture-host", "fixture-core"]]
        # Registry sources and checksums are optional only for path packages in Cargo.lock.
        # Cargo.lock 中仅路径包可以省略注册表来源与校验和。
        source = "registry+https://github.com/rust-lang/crates.io-index" if consumer == "old" else None
        # Each package has its own concrete Cargo package ID and manifest path.
        # 每个包拥有具体 Cargo 包 ID 与清单路径。
        core = {"name": "luaskills", "version": "0.5.9" if consumer == "old" else "0.6.0", "source": source, "id": "core-id", "manifest_path": str(upstream / "Cargo.toml")}
        # The runtime and standalone consumer expose separate direct Core dependencies.
        # 运行时与独立消费者分别具有直接 Core 依赖。
        runtime = {"name": "vulcan-plugin-runtime", "version": "1.0.0", "source": None, "id": "runtime-id", "manifest_path": str(owner / "crates/vulcan-plugin-runtime/Cargo.toml")}
        # Independent workspaces identify this package as resolve.root.
        # 独立工作区将此包标识为 resolve.root。
        root = {"name": "plugin-runtime-validation-" + consumer, "version": "0.0.0", "source": None, "id": "consumer-id", "manifest_path": str(destination / "Cargo.toml")}
        # Resolve dependency records retain the actual package ID and declared dependency kinds.
        # 解析依赖记录保留真实包 ID 及声明的依赖种类。
        metadata = {"packages": [core, runtime, root], "resolve": {"root": root["id"], "nodes": [{"id": package["id"], "dependencies": [core["id"]], "deps": [{"name": "luaskills", "pkg": core["id"], "dep_kinds": [{"kind": None, "target": None}]}], "features": []} for package in [root, runtime]]}}
        # Checksums belong to locks, never to metadata packages.
        # 校验和属于锁文件，绝不属于 metadata 包。
        locked_core = {"name": core["name"], "version": core["version"]}
        if consumer == "old":
            locked_core.update(source=source, checksum="frozen-registry-checksum")
        # A seed-only package may be pruned when the standalone dependency graph is smaller.
        # 当独立依赖图更小时，可以裁剪仅起点锁具有的包。
        seed = {"package": [locked_core, {"name": "unused", "version": "1.0.0"}]}
        # Preserve all retained identities and allow only the new standalone package identity.
        # 保持所有保留身份，仅允许新独立包身份。
        final_lock = {"package": [copy.deepcopy(locked_core), {"name": root["name"], "version": root["version"]}]}
        return {"metadata": metadata, "seed": seed, "final_lock": final_lock, "consumer": consumer, "destination": destination, "owner": owner, "upstream": upstream}

    def test_registered_and_local_consumers_are_accepted(self):
        """Accept independently proven old/new origins; assert retained checksum, edges and explicit seed pruning.
        接受独立证明的旧/新来源；断言保留校验和、依赖边及显式起点裁剪。
        """
        for consumer in ["old", "new"]:
            with self.subTest(consumer=consumer):
                # Verify each source independently instead of comparing two patched candidates.
                # 分别核验各来源，避免比较两个被补丁替换的候选。
                proof = verify_consumer(**self.fixture(consumer))
                self.assertEqual(len(proof["direct_core_edges"]), 2)
                self.assertEqual(proof["lock_normalization"]["removed_seed_identities"], [["unused", "1.0.0", None, None]])
                self.assertEqual(proof["core"]["checksum"], "frozen-registry-checksum" if consumer == "old" else None)

    def test_old_local_patch_is_rejected(self):
        """Reject a local replacement of the baseline even when the package version remains 0.5.9.
        即使包版本仍为 0.5.9，也拒绝本地补丁替换基线。
        """
        # A patched baseline loses the authoritative registry source in actual metadata.
        # 被补丁替换的基线在真实 metadata 中丢失权威注册表来源。
        arguments = self.fixture("old")
        next(package for package in arguments["metadata"]["packages"] if package["name"] == "luaskills")["source"] = None
        with self.assertRaisesRegex(ValueError, "metadata and final"):
            verify_consumer(**arguments)

    def test_matching_metadata_and_locks_cannot_replace_registry_baseline(self):
        """Reject a patched baseline or missing registry checksum even when metadata and both locks agree.
        即使 metadata 与两份锁一致，也拒绝被补丁替换的基线或缺失注册表校验和。
        """
        for mutation in ["local_patch", "missing_checksum"]:
            with self.subTest(mutation=mutation):
                # Agreement among altered artifacts does not prove the required original registered source.
                # 被修改产物之间的一致性不能证明要求的原注册表来源。
                arguments = self.fixture("old")
                for lock_name in ["seed", "final_lock"]:
                    # Locate Core by name so fixture growth cannot move the tested identity.
                    # 按名称定位 Core，避免夹具增长改变被测试身份位置。
                    core_lock = next(package for package in arguments[lock_name]["package"] if package["name"] == "luaskills")
                    core_lock.pop("checksum")
                    if mutation == "local_patch":
                        core_lock.pop("source")
                if mutation == "local_patch":
                    next(package for package in arguments["metadata"]["packages"] if package["name"] == "luaskills")["source"] = None
                with self.assertRaisesRegex(ValueError, "original registered|include a checksum"):
                    verify_consumer(**arguments)

    def test_duplicate_core_is_rejected(self):
        """Reject multiple Core package identities even if one expected version remains present.
        即使预期版本仍存在，也拒绝多个 Core 包身份。
        """
        # Competing Core packages cannot satisfy the unique implementation guarantee.
        # 竞争 Core 包不能满足唯一实现保证。
        arguments = self.fixture("new")
        arguments["metadata"]["packages"].append(dict(next(package for package in arguments["metadata"]["packages"] if package["name"] == "luaskills"), id="other-core-id"))
        with self.assertRaisesRegex(ValueError, "exactly one luaskills"):
            verify_consumer(**arguments)

    def test_wrong_manifest_owner_is_rejected(self):
        """Reject runtime or candidate Core manifests outside their explicitly declared source owner.
        拒绝不属于明确源码归属的运行时或候选 Core 清单。
        """
        for package_name in ["luaskills", "vulcan-plugin-runtime"]:
            with self.subTest(package_name=package_name):
                # Replace only the actual manifest path, preserving an otherwise valid dependency graph.
                # 仅替换真实清单路径，保持其余依赖图有效。
                arguments = self.fixture("new")
                next(package for package in arguments["metadata"]["packages"] if package["name"] == package_name)["manifest_path"] = str(Path("unowned/Cargo.toml").resolve())
                with self.assertRaises(ValueError):
                    verify_consumer(**arguments)

    def test_wrong_direct_core_edges_are_rejected(self):
        """Reject missing or redirected consumer/runtime edges despite a correct unique Core package.
        即使唯一 Core 包正确，也拒绝缺失或重定向的消费者/运行时依赖边。
        """
        for node_id in ["consumer-id", "runtime-id"]:
            for mutation in ["redirect", "missing", "inconsistent"]:
                with self.subTest(node_id=node_id, mutation=mutation):
                    # Both resolve edge representations must identify the same unique Core package.
                    # 两种解析依赖边表示必须标识同一个唯一 Core 包。
                    arguments = self.fixture("new")
                    # Select the explicitly tested node before changing its dependency edge.
                    # 修改依赖边前选择正在测试的明确节点。
                    node = next(node for node in arguments["metadata"]["resolve"]["nodes"] if node["id"] == node_id)
                    if mutation == "redirect":
                        next(edge for edge in node["deps"] if edge["name"] == "luaskills")["pkg"] = "other-core-id"
                    elif mutation == "missing":
                        node["deps"] = []
                    else:
                        node["dependencies"] = []
                    with self.assertRaisesRegex(ValueError, "Direct luaskills edge"):
                        verify_consumer(**arguments)

    def test_unexpected_dependency_changes_are_rejected(self):
        """Reject a new version/source/checksum or non-consumer dependency during seed normalization.
        拒绝起点规范化中新版本、来源、校验和或非消费者依赖。
        """
        for mutation in ["version", "source", "checksum", "additional"]:
            with self.subTest(mutation=mutation):
                # The immutable seed fixes identities before any resolver or compiler executes.
                # 不可变起点在任何解析器或编译器执行前固定身份。
                arguments = self.fixture("old")
                if mutation == "additional":
                    arguments["final_lock"]["package"].append({"name": "unexpected", "version": "1.0.0"})
                else:
                    next(package for package in arguments["final_lock"]["package"] if package["name"] == "luaskills")[mutation] = "changed"
                with self.assertRaisesRegex(ValueError, "Unexpected dependency identity"):
                    verify_consumer(**arguments)

    def test_wrong_resolve_root_is_rejected(self):
        """Reject a workspace metadata graph that does not identify the standalone consumer as its root.
        拒绝未将独立消费者标识为根包的工作区 metadata 图。
        """
        # Workspace metadata from another invocation must not be passed off as consumer evidence.
        # 其他调用产生的工作区 metadata 不得冒充消费者证据。
        arguments = self.fixture("new")
        arguments["metadata"]["resolve"]["root"] = None
        with self.assertRaisesRegex(ValueError, "resolve root"):
            verify_consumer(**arguments)


if __name__ == "__main__":
    unittest.main()
