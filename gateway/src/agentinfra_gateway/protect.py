"""Context protection rules.

The rules depend only on message position and type, so they hold for any
harness and need no markers:

* protected, never touched lossily: the system prompt (including Infra rules
  and AGENTS.md), tool definitions, the first user message, the last N turns;
* compressible: tool results in earlier turns.

A turn starts at an assistant message. "Last N turns" is everything from the
N-th last assistant message onward, so the model always sees its own recent
reasoning and the results it is acting on.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

Region = Literal["protected", "compressible", "kept"]


@dataclass(frozen=True)
class Slot:
    """One place in the request: a whole message, or one content block of it."""

    message: int
    block: int | None
    region: Region


def _protected_tail_start(roles: list[str], last_turns: int) -> int:
    assistants = [i for i, r in enumerate(roles) if r == "assistant"]
    if last_turns <= 0:
        return len(roles)
    if len(assistants) < last_turns:
        return 0
    return assistants[-last_turns]


def _first_user(messages: list[dict]) -> int | None:
    for i, m in enumerate(messages):
        if m.get("role") == "user" and not _is_tool_result_only(m):
            return i
    return None


def _is_tool_result_only(m: dict) -> bool:
    c = m.get("content")
    return isinstance(c, list) and bool(c) and all(isinstance(b, dict) and b.get("type") == "tool_result" for b in c)


def classify_anthropic(body: dict, last_turns: int = 2) -> list[Slot]:
    """Slots of an Anthropic Messages request. `system` and `tools` are always protected
    and are not part of `messages`, so they never appear as slots."""
    messages = body.get("messages", [])
    tail = _protected_tail_start([m.get("role", "") for m in messages], last_turns)
    first = _first_user(messages)
    slots: list[Slot] = []
    for i, m in enumerate(messages):
        if i >= tail or i == first:
            slots.append(Slot(i, None, "protected"))
            continue
        content = m.get("content")
        if m.get("role") == "user" and isinstance(content, list):
            for j, b in enumerate(content):
                region: Region = "compressible" if isinstance(b, dict) and b.get("type") == "tool_result" else "kept"
                slots.append(Slot(i, j, region))
        else:
            slots.append(Slot(i, None, "kept"))
    return slots


def classify_openai(body: dict, last_turns: int = 2) -> list[Slot]:
    """Slots of an OpenAI Chat Completions request: system/developer messages are
    protected, `tool` messages in earlier turns are compressible."""
    messages = body.get("messages", [])
    tail = _protected_tail_start([m.get("role", "") for m in messages], last_turns)
    first = next((i for i, m in enumerate(messages) if m.get("role") == "user"), None)
    slots = []
    for i, m in enumerate(messages):
        role = m.get("role")
        if role in ("system", "developer") or i >= tail or i == first:
            slots.append(Slot(i, None, "protected"))
        elif role == "tool":
            slots.append(Slot(i, None, "compressible"))
        else:
            slots.append(Slot(i, None, "kept"))
    return slots
