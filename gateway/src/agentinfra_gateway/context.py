"""The context stage of the request pipeline: classify, compress, check the budget.

Compression only ever rewrites the content of a compressible tool result in
place; blocks are never added or removed, so tool_use/tool_result pairing and
tool definitions are untouched. Every compressed original goes to the
OriginalStore and the replacement names its digest for `context_retrieve`.

The compressors here are the baseline until Headroom's content-type pipeline
is absorbed; they share its contract: (text) -> shorter text or None.

请求流水线的上下文阶段：分类、压缩、检查预算。

压缩只会原地改写可压缩工具结果的内容；从不增删块，因此 tool_use/tool_result
配对和工具定义不受影响。每个被压缩的原文都进入 OriginalStore，替换文本里写明
它的摘要，供 `context_retrieve` 取回。

这里的压缩器是吸收 Headroom 按内容类型的流水线之前的基线；约定与之相同：
(text) -> 更短的文本或 None。
"""
from __future__ import annotations

import copy
import json
from dataclasses import dataclass, field
from typing import Callable

from .protect import Slot, classify_anthropic, classify_openai
from .store import OriginalStore

Compressor = Callable[[str], "str | None"]


class ContextBudgetExceeded(Exception):
    """Even after compression the protected content does not fit. Never drop it silently.

    压缩之后受保护内容仍然放不下。绝不悄悄丢弃。
    """

    code = "context_budget_exceeded"

    def __init__(self, estimated: int, limit: int):
        super().__init__(f"estimated {estimated} tokens exceeds the model's {limit}")
        self.estimated = estimated
        self.limit = limit


def estimate_tokens(obj) -> int:
    """Cheap, provider-independent estimate (~4 bytes per token).

    与供应商无关的粗略估算（约 4 字节一个 token）。
    """
    return (len(json.dumps(obj, ensure_ascii=False).encode("utf-8")) + 3) // 4


def compress_json(text: str) -> str | None:
    try:
        value = json.loads(text)
    except ValueError:
        return None
    small = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    return small if len(small) < len(text) else None


def head_tail(keep_lines: int = 40) -> Compressor:
    def run(text: str) -> str | None:
        lines = text.splitlines()
        if len(lines) <= 2 * keep_lines:
            return None
        cut = len(lines) - 2 * keep_lines
        return "\n".join(lines[:keep_lines] + [f"[... {cut} lines omitted ...]"] + lines[-keep_lines:])

    return run


DEFAULT_COMPRESSORS: list[Compressor] = [compress_json, head_tail()]


@dataclass
class ContextReport:
    tokens_before: int = 0
    tokens_after: int = 0
    compressed: list[str] = field(default_factory=list)  # digests / 摘要


def _marker(digest: str) -> str:
    return f"\n[compressed by gateway; full content: context_retrieve(digest=\"{digest}\")]"


def _compress_text(text: str, compressors: list[Compressor], store: OriginalStore, min_chars: int, report: ContextReport) -> str:
    if len(text) < min_chars:
        return text
    for c in compressors:
        out = c(text)
        if out is not None and len(out) < len(text):
            d = store.put(text)
            report.compressed.append(d)
            return out + _marker(d)
    return text


def _compress_block_content(content, compressors, store, min_chars, report):
    if isinstance(content, str):
        return _compress_text(content, compressors, store, min_chars, report)
    if isinstance(content, list):
        out = []
        for part in content:
            if isinstance(part, dict) and part.get("type") == "text" and isinstance(part.get("text"), str):
                part = {**part, "text": _compress_text(part["text"], compressors, store, min_chars, report)}
            out.append(part)
        return out
    return content


def prepare(
    body: dict,
    api: str,
    store: OriginalStore,
    *,
    context_limit: int,
    protect_last_turns: int = 2,
    compression: str = "auto",
    compressors: list[Compressor] | None = None,
    min_chars: int = 2000,
) -> tuple[dict, ContextReport]:
    """Return the request to forward and what was done to it.

    返回要转发的请求，以及对它做了什么。
    """
    compressors = DEFAULT_COMPRESSORS if compressors is None else compressors
    report = ContextReport(tokens_before=estimate_tokens(body))
    out = copy.deepcopy(body)
    if compression != "off":
        slots: list[Slot] = (classify_anthropic if api == "anthropic" else classify_openai)(out, protect_last_turns)
        for s in slots:
            if s.region != "compressible":
                continue
            msg = out["messages"][s.message]
            if s.block is None:
                msg["content"] = _compress_block_content(msg.get("content"), compressors, store, min_chars, report)
            else:
                block = msg["content"][s.block]
                block["content"] = _compress_block_content(block.get("content"), compressors, store, min_chars, report)
    report.tokens_after = estimate_tokens(out)
    if report.tokens_after > context_limit:
        raise ContextBudgetExceeded(report.tokens_after, context_limit)
    return out, report
