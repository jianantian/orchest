"""Deep research example using a web-search sub-agent as a tool.

This example is intentionally deterministic: by default it starts a tiny
Anthropic-compatible local provider so the SDK flow can be evaluated without
network access or an API key. The structure mirrors the real target shape:

- the main deep-research agent uses DEEP_RESEARCH_MODEL
- the web-search agent uses WEB_SEARCH_MODEL
- the main agent calls the web-search agent through a normal SDK tool
- the web-search agent owns query rewriting and the actual web_search tool

The sub-agent boundary keeps search-only prompt instructions and search context
out of the main agent's conversation.
"""

from __future__ import annotations

import json
import os
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

from agent_runtime import Agent, RuntimeEvent


DEEP_RESEARCH_MODEL = os.environ.get("DEEP_RESEARCH_MODEL", "deep-research-main-demo")
WEB_SEARCH_MODEL = os.environ.get("WEB_SEARCH_MODEL", "web-search-query-rewriter-demo")
LOCAL_PROVIDER_PORT = int(os.environ.get("DEEP_RESEARCH_DEMO_PORT", "8791"))
EXA_SEARCH_URL = "https://api.exa.ai/search"


def _sse(event: str, data: dict[str, Any]) -> bytes:
    return f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()


def _latest_user_text(messages: list[dict[str, Any]]) -> str:
    for message in reversed(messages):
        if message.get("role") != "user":
            continue
        parts = message.get("content", [])
        text_parts = [
            block.get("text", "")
            for block in parts
            if isinstance(block, dict) and block.get("type") == "text"
        ]
        if text_parts:
            return "\n".join(text_parts)
    return ""


def _latest_tool_result(messages: list[dict[str, Any]]) -> Any:
    for message in reversed(messages):
        for block in message.get("content", []):
            if isinstance(block, dict) and block.get("type") == "tool_result":
                return block.get("content")
    return None


def _rewrite_query(question: str) -> str:
    normalized = " ".join(question.strip().split())
    return f"{normalized} latest architecture SDK agent runtime sub-agent web search"


def _text_response(text: str, output_tokens: int = 80) -> list[bytes]:
    return [
        _sse("content_block_start", {"content_block": {"type": "text"}}),
        _sse("content_block_delta", {"delta": {"type": "text_delta", "text": text}}),
        _sse("content_block_stop", {}),
        _sse(
            "message_delta",
            {
                "delta": {"stop_reason": "end_turn"},
                "usage": {"output_tokens": output_tokens},
            },
        ),
    ]


def _tool_use_response(tool_name: str, args: dict[str, Any]) -> list[bytes]:
    return [
        _sse(
            "content_block_start",
            {
                "content_block": {
                    "type": "tool_use",
                    "id": "call_1",
                    "name": tool_name,
                }
            },
        ),
        _sse(
            "content_block_delta",
            {
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": json.dumps(args),
                }
            },
        ),
        _sse("content_block_stop", {}),
        _sse(
            "message_delta",
            {"delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 24}},
        ),
    ]


class DeepResearchDemoProvider(BaseHTTPRequestHandler):
    def log_message(self, _format: str, *_args: Any) -> None:
        return

    def do_POST(self) -> None:
        if self.path != "/v1/messages":
            self.send_error(404)
            return

        content_len = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(content_len) or b"{}")
        model = body.get("model", "")
        messages = body.get("messages", [])
        tool_names = [tool.get("name") for tool in body.get("tools", [])]
        tool_result = _latest_tool_result(messages)
        question = _latest_user_text(messages)

        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        self.wfile.write(_sse("message_start", {"message": {"usage": {"input_tokens": 32}}}))

        if model == WEB_SEARCH_MODEL:
            chunks = self._web_search_agent_response(question, tool_names, tool_result)
        else:
            chunks = self._deep_research_agent_response(question, tool_names, tool_result)

        for chunk in chunks:
            self.wfile.write(chunk)

    def _deep_research_agent_response(
        self,
        question: str,
        tool_names: list[str],
        tool_result: Any,
    ) -> list[bytes]:
        if tool_result is None and "web_research" in tool_names:
            return _tool_use_response(
                "web_research",
                {
                    "question": question,
                    "reason": "Need isolated web-search context and rewritten queries.",
                },
            )

        return _text_response(
            "Research brief:\n"
            "- The main agent delegated web evidence gathering to a search sub-agent.\n"
            "- The sub-agent rewrote the user question before calling web_search.\n"
            "- Keeping search in a sub-agent isolates search prompts and intermediate context.\n\n"
            f"Sub-agent result: {json.dumps(tool_result, ensure_ascii=False)}"
        )

    def _web_search_agent_response(
        self,
        question: str,
        tool_names: list[str],
        tool_result: Any,
    ) -> list[bytes]:
        if tool_result is None and "web_search" in tool_names:
            rewritten_query = _rewrite_query(question)
            return _tool_use_response(
                "web_search",
                {
                    "query": rewritten_query,
                    "rationale": "Expand the user question into a search-oriented query.",
                },
            )

        return _text_response(
            "Web search summary:\n"
            "- Query rewriting happened inside the web-search agent.\n"
            "- Search results stayed in the sub-agent context.\n"
            "- The main agent receives only the distilled search output.\n\n"
            f"Search tool result: {json.dumps(tool_result, ensure_ascii=False)}",
            output_tokens=64,
        )


def start_demo_provider() -> str:
    os.environ.setdefault("ANTHROPIC_API_KEY", "local-demo-key")
    server = ThreadingHTTPServer(("127.0.0.1", LOCAL_PROVIDER_PORT), DeepResearchDemoProvider)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return f"http://127.0.0.1:{LOCAL_PROVIDER_PORT}/v1/messages"


def configure_provider() -> str | None:
    if os.environ.get("DEEP_RESEARCH_USE_REAL_PROVIDER") == "1":
        return os.environ.get("ANTHROPIC_API_URL")
    return start_demo_provider()


def final_output(events: list[RuntimeEvent]) -> Any:
    for event in reversed(events):
        if event.get("type") == "run_completed":
            return event.get("output")
    return None


def run_agent_in_thread(agent: Agent, question: str) -> list[RuntimeEvent]:
    """Run a nested agent from a tool handler without nesting Tokio runtimes."""
    result: dict[str, Any] = {}

    def target() -> None:
        try:
            result["events"] = agent.run(question)
        except BaseException as exc:
            result["error"] = exc

    thread = threading.Thread(target=target)
    thread.start()
    thread.join()

    if "error" in result:
        raise result["error"]
    return result["events"]


api_url = configure_provider()

web_search_agent = Agent(
    model=WEB_SEARCH_MODEL,
    system_prompt=(
        "You are a focused web-search agent. Rewrite the user's question into a "
        "precise web search query, call web_search, then return a compact summary. "
        "Do not answer from memory."
    ),
    api_url=api_url,
)


@web_search_agent.tool
def web_search(query: str, rationale: str = "") -> dict[str, Any]:
    """Search the web for the rewritten query."""
    api_key = os.environ.get("EXA_API_KEY")
    if api_key:
        return exa_search(query=query, rationale=rationale, api_key=api_key)

    return {
        "provider": "fixture",
        "query": query,
        "rationale": rationale,
        "results": [
            {
                "title": "Agent SDK architecture notes",
                "url": "https://example.test/agent-sdk-architecture",
                "snippet": "Sub-agents are useful when a task needs isolated instructions and context.",
            },
            {
                "title": "Research workflow patterns",
                "url": "https://example.test/research-workflows",
                "snippet": "Query rewriting improves search recall before synthesis.",
            },
        ],
    }


def exa_search(query: str, rationale: str, api_key: str) -> dict[str, Any]:
    payload = {
        "query": query,
        "type": os.environ.get("EXA_SEARCH_TYPE", "auto"),
        "numResults": int(os.environ.get("EXA_NUM_RESULTS", "5")),
        "contents": {"highlights": True},
    }
    if os.environ.get("EXA_LIVECRAWL") == "1":
        payload["contents"]["maxAgeHours"] = 0

    request = urllib.request.Request(
        EXA_SEARCH_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "x-api-key": api_key,
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        return {
            "provider": "exa",
            "query": query,
            "rationale": rationale,
            "error": f"Exa HTTP {exc.code}: {detail}",
        }
    except urllib.error.URLError as exc:
        return {
            "provider": "exa",
            "query": query,
            "rationale": rationale,
            "error": f"Exa request failed: {exc.reason}",
        }

    return {
        "provider": "exa",
        "query": query,
        "rationale": rationale,
        "search_type": data.get("searchType"),
        "request_id": data.get("requestId"),
        "cost_dollars": data.get("costDollars"),
        "results": [
            {
                "title": result.get("title"),
                "url": result.get("url"),
                "published_date": result.get("publishedDate"),
                "highlights": result.get("highlights", []),
            }
            for result in data.get("results", [])
        ],
    }


deep_research_agent = Agent(
    model=DEEP_RESEARCH_MODEL,
    system_prompt=(
        "You are a deep-research agent. Use web_research for evidence gathering. "
        "Keep synthesis in the main context, but delegate search query rewriting "
        "and raw search work to the web-search sub-agent."
    ),
    api_url=api_url,
)


@deep_research_agent.tool
def web_research(question: str, reason: str = "") -> dict[str, Any]:
    """Run the web-search sub-agent for an isolated research pass."""
    print(f"\n[sub-agent:start] model={WEB_SEARCH_MODEL}")
    print(f"[sub-agent:reason] {reason}")

    events = run_agent_in_thread(web_search_agent, question)
    output = final_output(events)

    print(f"[sub-agent:done] output={output}\n")
    return {
        "sub_agent": "web_search_agent",
        "model": WEB_SEARCH_MODEL,
        "output": output,
    }


def print_trace(events: list[RuntimeEvent]) -> None:
    for event in events:
        event_type = event.get("type")
        if event_type == "tool_call_started":
            print(f"[main:tool] {event.get('tool')} input={event.get('input')}")
        elif event_type == "tool_call_completed":
            print(f"[main:tool-result] {event.get('tool')}")
        elif event_type == "run_completed":
            print(f"\n[main:final]\n{event.get('output')}")
        elif event_type == "run_failed":
            print(f"[main:error] {event.get('error')}")


if __name__ == "__main__":
    question = (
        "How should we structure a deep research SDK example that uses a web "
        "search sub-agent as a tool?"
    )
    print(f"[main:start] model={DEEP_RESEARCH_MODEL}")
    print(f"[question] {question}")
    print_trace(deep_research_agent.run(question))
