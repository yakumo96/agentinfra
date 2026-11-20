import json
import os
import stat

import pytest

from agentinfra_gateway.context import ContextBudgetExceeded, prepare
from agentinfra_gateway.protect import classify_anthropic, classify_openai
from agentinfra_gateway.store import OriginalStore

BIG = "\n".join(f"line {i}" for i in range(500))


def tool_result(id_, text):
    return {"role": "user", "content": [{"type": "tool_result", "tool_use_id": id_, "content": text}]}


def tool_use(id_):
    return {"role": "assistant", "content": [{"type": "tool_use", "id": id_, "name": "bash", "input": {"command": "ls"}}]}


def conversation(turns: int):
    msgs = [{"role": "user", "content": "Implement the feature. " + BIG}]
    for t in range(turns):
        msgs += [tool_use(f"t{t}"), tool_result(f"t{t}", BIG)]
    return {"system": "Infra rules ... AGENTS.md ...", "tools": [{"name": "bash"}], "messages": msgs}


@pytest.fixture
def store(tmp_path):
    s = OriginalStore(tmp_path / "originals.sqlite", retention_s=3600)
    yield s
    s.close()


def test_regions_by_position():
    body = conversation(4)
    regions = {(s.message, s.block): s.region for s in classify_anthropic(body, last_turns=2)}
    assert regions[(0, None)] == "protected"  # first user message / 第一条用户消息
    assert regions[(1, None)] == "kept"  # old assistant tool_use: not compressible / 较早的 assistant tool_use：不可压缩
    assert regions[(2, 0)] == "compressible"  # old tool result / 较早的工具结果
    assert regions[(4, 0)] == "compressible"
    assert all(regions[(i, None)] == "protected" for i in (5, 6, 7, 8))  # last two turns / 最近两轮


def test_only_old_tool_results_change_and_originals_are_retrievable(store):
    body = conversation(4)
    out, report = prepare(body, "anthropic", store, context_limit=10**9)
    assert out["system"] == body["system"] and out["tools"] == body["tools"]
    assert out["messages"][0] == body["messages"][0]
    assert out["messages"][5:] == body["messages"][5:]
    assert len(out["messages"]) == len(body["messages"])
    compressed = out["messages"][2]["content"][0]["content"]
    assert "lines omitted" in compressed and "context_retrieve" in compressed
    assert len(report.compressed) == 2 and len(set(report.compressed)) == 1, "identical originals share one digest"
    assert store.get(report.compressed[0]) == BIG
    assert report.tokens_after < report.tokens_before
    # tool_use / tool_result pairing is intact
    # tool_use / tool_result 配对保持完整
    assert [b["tool_use_id"] for m in out["messages"][2::2] for b in m["content"]] == ["t0", "t1", "t2", "t3"]


def test_compression_off_and_budget(store):
    body = conversation(4)
    out, report = prepare(body, "anthropic", store, context_limit=10**9, compression="off")
    assert out == body and report.compressed == []
    with pytest.raises(ContextBudgetExceeded):
        prepare(body, "anthropic", store, context_limit=100)


def test_openai_tool_messages(store):
    body = {"messages": [
        {"role": "system", "content": "rules"},
        {"role": "user", "content": "go"},
        {"role": "assistant", "tool_calls": [{"id": "a"}]},
        {"role": "tool", "tool_call_id": "a", "content": json.dumps({"rows": list(range(800))}, indent=2)},
        {"role": "assistant", "content": "next"},
        {"role": "tool", "tool_call_id": "b", "content": BIG},
        {"role": "assistant", "content": "done"},
    ]}
    regions = [s.region for s in classify_openai(body, last_turns=2)]
    assert regions == ["protected", "protected", "kept", "compressible", "protected", "protected", "protected"]
    out, _ = prepare(body, "openai", store, context_limit=10**9)
    assert out["messages"][3]["content"].startswith('{"rows":[0,1,2')
    assert out["messages"][5]["content"] == BIG


def test_store_file_mode_and_retention(tmp_path):
    s = OriginalStore(tmp_path / "o.sqlite", retention_s=10)
    d = s.put("x", now=100.0)
    assert stat.S_IMODE(os.stat(tmp_path / "o.sqlite").st_mode) == 0o600
    assert s.get(d, now=105.0) == "x"
    assert s.get(d, now=111.0) is None
    assert s.expire(now=111.0) == 1
    s.close()
