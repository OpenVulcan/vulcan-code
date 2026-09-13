"""Execute production plugin SQL against isolated SQLite fixtures and report stage timings.
在隔离 SQLite 夹具中执行生产插件 SQL，并报告各阶段耗时。
"""
import datetime
import json
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile
import time

# ROOT identifies the current checkout without depending on the caller's directory.
# ROOT 标识当前检出目录，不依赖调用方目录。
ROOT = Path(__file__).resolve().parents[1]


def queries(source):
    """Extract exact dashboard SQL and its fixed filter prelude; return ordered query pairs.
    提取精确仪表盘 SQL 及固定筛选前缀；返回有序查询对。
    """
    # body is restricted to the source-defined dashboard function.
    # body 限制在源码定义的仪表盘函数内。
    body = source.split("local function load_dashboard", 1)[1].split("function M.settings_get", 1)[0]
    # prelude is present only in the explicit filtered dashboard implementation.
    # prelude 仅存在于明确的筛选仪表盘实现。
    prelude_match = re.search(r"return sqlite_query\(\[\[(.*?)\]\] \.\. sql, filter_params\)", body, re.S)
    prelude = prelude_match.group(1) if prelude_match else ""
    statements = [
        (name, (prelude if method == "query" else "") + sql, method == "query")
        for name, method, sql in re.findall(r"local (\w+) = (?:\([^\n]+\) and )?(query|sqlite_query)\(\[\[(.*?)\]\]", body, re.S)
    ]
    cost = re.search(r"local cost_sql = \[\[(.*?)\]\]", body, re.S)
    if cost:
        statements.append(("cost_rows", prelude + cost.group(1), True))
    return statements


def run_queries(connection, statements, parameters):
    """Run exact SQL with bound parameters; return rows and per-query elapsed milliseconds.
    使用绑定参数运行精确 SQL；返回行与每项查询毫秒耗时。
    """
    rows = {}
    timings = {}
    for name, sql, filtered in statements:
        started = time.perf_counter()
        rows[name] = [dict(row) for row in connection.execute(sql, parameters + ["", "newest", '["fixture-000000"]', 0] if filtered else [])]
        timings[name] = round((time.perf_counter() - started) * 1000, 3)
    return rows, timings


def seed(connection, count):
    """Create current production schema and synthetic rows; never access the user database.
    创建当前生产 Schema 与合成行；绝不访问用户数据库。
    """
    source = (ROOT / "crates/vulcan-workspace-store/src/workspace_store/provider_usage.rs").read_text(encoding="utf-8")
    body = source.split("async fn ensure_provider_usage_schema_at(", 1)[1]
    schema = body.split('r#"', 1)[1].split('"#', 1)[0]
    connection.executescript(schema)
    today = datetime.date.today().isoformat()
    now = int(time.time() * 1000)
    for index in range(count):
        consumer = "webui" if index % 2 == 0 else "automation"
        connection.execute("INSERT INTO provider_usage_daily_v3 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                           (today, "fixture", "entry", consumer, "response_create", f"fixture-{index:06}",
                            1, 1, 1, 10, 8, 2, 0, 20, 0, 30, 0))
        # Required source columns receive synthetic values according to the actual table schema.
        # 必需源码列按真实表结构接收合成值。
        required = [row for row in connection.execute("PRAGMA table_info(provider_usage_attempts_v3)")
                    if row["notnull"] and row["dflt_value"] is None]
        values = {row["name"]: 0 if row["type"] == "INTEGER" else "fixture" for row in required}
        values.update(attempt_id=f"attempt-{index:06}", consumer_id=consumer,
                      terminal_at_ms=now, calendar_date=today, mapped_model=f"fixture-{index:06}",
                      status="succeeded", input_tokens=10, uncached_input_tokens=8,
                      cache_read_tokens=2, cache_write_tokens=0, output_tokens=20, total_tokens=30)
        connection.execute("INSERT INTO provider_usage_attempts_v3 (" + ",".join(values)
                           + ") VALUES (" + ",".join("?" for _ in values) + ")", tuple(values.values()))
    connection.commit()


def main():
    """Validate production filtering/pagination and compare 0/10/10000-row cold/hot SQL costs.
    验证生产筛选与分页，并对比零、十、一万行冷/热 SQL 成本。
    """
    current = (ROOT / "plugins/vulcan-usage-statistics/runtime/plugin.bundle.lua").read_text(encoding="utf-8")
    baseline = subprocess.check_output(["rtk", "proxy", "git", "show", "HEAD:plugins/vulcan-usage-statistics/runtime/plugin.bundle.lua"], cwd=ROOT).decode("utf-8")
    results = []
    for count in [0, 10, 10000]:
        with tempfile.TemporaryDirectory(prefix="vulcan-plugin-sql-") as temporary:
            path = Path(temporary) / "fixture.sqlite3"
            connection = sqlite3.connect(path)
            connection.row_factory = sqlite3.Row
            seed(connection, count)
            connection.close()
            for label, source in [("baseline", baseline), ("current", current)]:
                connection = sqlite3.connect(path)
                connection.row_factory = sqlite3.Row
                statements = queries(source)
                assert len(statements) == (6 if label == "baseline" else 10)
                cold, cold_times = run_queries(connection, statements, [30, "", 0, 0])
                hot, hot_times = run_queries(connection, statements, [30, "", 0, 0])
                if label == "current":
                    assert hot["initial_revision_rows"][0]["dashboardRevision"] == hot["completed_revision_rows"][0]["dashboardRevision"]
                    assert hot["summary_rows"][0]["attempts"] == count
                    assert hot["meta_rows"][0]["dailyCount"] == count
                    assert len(hot["daily"]) == min(count, 100)
                    assert len(hot["recent_attempts"]) == min(count, 50)
                    assert len(hot["cost_rows"]) == min(count, 1)
                    filtered, _ = run_queries(connection, statements, [30, "webui", 100, 50])
                    assert filtered["summary_rows"][0]["attempts"] == (count + 1) // 2
                    assert filtered["meta_rows"][0]["dailyCount"] == (count + 1) // 2
                    assert all(row["consumerId"] == "webui" for row in filtered["daily"])
                    assert {row["modelId"] for row in hot["daily"]}.isdisjoint({row["modelId"] for row in run_queries(connection, statements, [30, "", 100, 50])[0]["daily"]})
                results.append({"rows": count, "version": label, "cold_ms": cold_times, "hot_ms": hot_times,
                                "response_bytes": len(json.dumps(hot).encode())})
                connection.close()
            # pricing_sql exercises complete used-model pagination independent of dashboard page size.
            # pricing_sql 验证完整已有用量模型分页，不依赖仪表盘分页大小。
            pricing_sql = re.search(r"sqlite_query\(\[\[(.*?)\]\]", current.split("function M.pricing_get", 1)[1], re.S).group(1)
            connection = sqlite3.connect(path)
            full = json.loads(connection.execute(pricing_sql, ['["catalog-only"]', '["override-only"]', "", "used", 0]).fetchone()[0])
            assert full["totalCount"] == count
            assert len(full["models"]) == min(count, 25)
            next_page = json.loads(connection.execute(pricing_sql, ['["catalog-only"]', '["override-only"]', "", "used", 25]).fetchone()[0])
            assert {row["modelId"] for row in full["models"]}.isdisjoint({row["modelId"] for row in next_page["models"]})
            unpriced = json.loads(connection.execute(pricing_sql, ['["fixture-000000"]', '[]', "", "unpriced", 0]).fetchone()[0])
            assert unpriced["totalCount"] == max(0, count - 1)
            searched = json.loads(connection.execute(pricing_sql, ['["catalog-only"]', '[]', "catalog-only", "all", 0]).fetchone()[0])
            assert searched["totalCount"] == 1
            # The exact production cost query must cover every configured model across bounded pages.
            # 精确生产费用查询必须跨有界分页覆盖每个已配置模型。
            cost_sql = next(sql for name, sql, _ in queries(current) if name == "cost_rows")
            priced_models = json.dumps([f"fixture-{index:06}" for index in range(count)])
            cost_models = set()
            cost_tokens = 0
            offset = 0
            connection.row_factory = sqlite3.Row
            while True:
                page = list(connection.execute(cost_sql, [30, "", 0, 0, "", "newest", priced_models, offset]))
                assert len(page) <= 1000
                assert cost_models.isdisjoint({row["modelId"] for row in page})
                cost_models.update(row["modelId"] for row in page)
                cost_tokens += sum(row["uncachedInputTokens"] + row["cacheReadTokens"] + row["outputTokens"] for row in page)
                if len(page) < 1000:
                    break
                offset += 1000
            assert len(cost_models) == count
            assert cost_tokens == count * 30
            connection.close()
    # Context7 aggregation uses its source migration and exact three bound parameters.
    # Context7 聚合使用其源码迁移与精确的三个绑定参数。
    context = (ROOT / "plugins/context7/runtime/plugin.bundle.lua").read_text(encoding="utf-8")
    schema_body = context.split("local function ensure_schema()", 1)[1].split("local existing_credential", 1)[0]
    context_query = re.search(r"sqlite_query\(\[\[(.*?)\]\]", context.split("local function usage_value", 1)[1], re.S).group(1)
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    for statement in re.findall(r"\[\[(.*?)\]\]", schema_body, re.S):
        connection.execute(statement)
    empty = dict(connection.execute(context_query, [30 * 86400000, "anonymous", 0]).fetchone())
    assert empty["requests"] == 0 and empty["quotaAuthMode"] is None
    connection.execute("INSERT INTO context7_request_events(request_id,operation,auth_mode,credential_generation,status,duration_ms,created_at_ms) VALUES ('fixture','query_docs','anonymous',0,'success',12,?)", [int(time.time() * 1000)])
    assert connection.execute(context_query, [30 * 86400000, "anonymous", 0]).fetchone()["successes"] == 1
    print(json.dumps({"sqlite_version": sqlite3.sqlite_version, "context7_source_query": "passed",
                      "measurement_scope": "isolated SQLite only; excludes Lua, hostcall queue, UI and real host engine",
                      "statistics": results}, indent=2))


if __name__ == "__main__":
    main()
