// Run only the offline tests that replace process-global LuaSkills callbacks, one fresh process at a time.
// 仅运行替换进程全局 LuaSkills 回调的离线测试，每项使用独立进程串行执行。
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Repository root keeps invocation independent of the caller's working directory.
// 仓库根目录确保入口不依赖调用者当前工作目录。
const root = fileURLToPath(new URL('..', import.meta.url));
// Exact terminal function names are an explicit offline allowlist, not an unrestricted --ignored run.
// 精确的末级函数名构成显式离线白名单，不执行无范围的 --ignored。
const tests = [
  ['vulcan-agent-runtime', 'real_skill_runlua_and_system_calls_overlap_inside_host_callbacks'],
  ['vulcan-service-kernel', 'pending_and_committed_provider_installations_have_exact_owners'],
  ['vulcan-service-kernel', 'pending_callbacks_roll_back_before_service_publication'],
  ['vulcan-service-kernel', 'stale_in_flight_projection_cannot_regress_terminal_state'],
  ['vulcan-service-kernel', 'operation_finalization_cannot_return_or_retain_in_flight_state'],
  ['vulcan-service-kernel', 'operation_progress_production_projector_exposes_live_start'],
];
for (const [pkg, name] of tests) {
  // Nextest isolates each test and fails if a renamed/deleted test no longer matches.
  // Nextest 隔离每个测试，测试改名或删除导致无匹配时直接失败。
  const result = spawnSync('cargo', [
    'nextest', 'run', '--locked', '--lib', '-p', pkg,
    '--run-ignored', 'only', '--test-threads=1', '--no-tests=fail',
    '-E', `test(/::${name}$/)`,
  ], { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
