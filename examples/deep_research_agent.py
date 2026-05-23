"""Deep research example using real models, Exa, and agent-as-tool.

Required environment:

    ANTHROPIC_API_KEY=...
    EXA_API_KEY=...

Optional environment:

    ANTHROPIC_API_URL=...        # Anthropic-compatible /v1/messages endpoint
    DEEP_RESEARCH_MODEL=...      # Main synthesis model, required
    WEB_SEARCH_MODEL=...         # Query rewrite/search model, required
    EXA_SEARCH_TYPE=auto|fast|deep-lite|deep
    EXA_NUM_RESULTS=5
    EXA_LIVECRAWL=1

This example is intentionally not mocked. It exercises the SDK shape we want:

- the main deep-research agent has a `web_research` tool backed by another Agent
- the web-search agent has its own model, prompt, context, and Exa tool
- the main agent also gets the core `write_file` tool for report writing
- the sub-agent returns compact evidence so raw search context stays isolated
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from argparse import ArgumentParser
from pathlib import Path
from typing import Any

from agent_runtime import Agent, RuntimeEvent


EXA_SEARCH_URL = "https://api.exa.ai/search"
DEFAULT_REPORT_PATH = "target/deep-research-report.md"
MAX_HIGHLIGHT_CHARS = 900


def require_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise RuntimeError(f"{name} is required for this non-mock example")
    return value


def provider_url() -> str | None:
    return os.environ.get("ANTHROPIC_API_URL")


def exa_search(query: str, rationale: str = "") -> dict[str, Any]:
    """Search the web with Exa highlights for agent workflows."""
    api_key = require_env("EXA_API_KEY")
    payload: dict[str, Any] = {
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
        headers={"Content-Type": "application/json", "x-api-key": api_key},
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"Exa HTTP {exc.code}: {detail}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Exa request failed: {exc.reason}") from exc

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
                "evidence": " ".join(result.get("highlights", []))[:MAX_HIGHLIGHT_CHARS],
            }
            for result in data.get("results", [])
        ],
    }


def final_output(events: list[RuntimeEvent]) -> Any:
    for event in reversed(events):
        if event.get("type") == "run_completed":
            return event.get("output")
    return None


def print_trace(events: list[RuntimeEvent]) -> None:
    for event in events:
        event_type = event.get("type")
        if event_type == "tool_call_started":
            print(f"[tool] {event.get('tool')} input={event.get('input')}")
        elif event_type == "tool_call_completed":
            print(f"[tool:done] {event.get('tool')}")
        elif event_type == "child_run_event":
            child = event.get("event", {})
            if isinstance(child, dict) and child.get("type") == "tool_call_started":
                print(f"[sub-agent:tool] {child.get('tool')} input={child.get('input')}")
        elif event_type == "run_completed":
            print(f"\n[final]\n{event.get('output')}")
        elif event_type == "run_failed":
            print(f"\n[error] {event.get('error')}")


def build_web_search_agent() -> Agent:
    agent = Agent(
        model=require_env("WEB_SEARCH_MODEL"),
        system_prompt=(
            "You are a web-search research sub-agent. Rewrite the user's task into "
            "one precise Exa query, call exa_search, and return a compact research "
            "brief with key findings and source URLs. Do not answer from memory."
        ),
        api_url=provider_url(),
    )
    agent.register_tool(exa_search)
    return agent


def build_deep_research_agent(web_search_agent: Agent, report_path: str) -> Agent:
    agent = Agent(
        model=require_env("DEEP_RESEARCH_MODEL"),
        system_prompt=(
            "You are a deep-research agent. For evidence gathering, call the "
            "web_research tool. That tool is a separate web-search sub-agent with "
            "its own model, prompt, tools, and context. After synthesizing the "
            f"answer, call write_file to write a markdown report to {report_path}. "
            "Then return a concise final answer with the report path and sources."
        ),
        api_url=provider_url(),
    )
    agent.register_agent_tool(
        name="web_research",
        description="Delegate web research to an isolated web-search sub-agent.",
        agent=web_search_agent,
        input_key="question",
    )
    agent.register_write_file_tool(requires_approval=False)
    return agent


def parse_args() -> tuple[str, str]:
    parser = ArgumentParser(description="Run the real deep-research agent example.")
    parser.add_argument(
        "question",
        nargs="?",
        default="How should Orchest expose sub-agent-as-tool ergonomics?",
    )
    parser.add_argument("--report", default=DEFAULT_REPORT_PATH)
    args = parser.parse_args()
    return args.question, args.report


if __name__ == "__main__":
    require_env("ANTHROPIC_API_KEY")
    require_env("EXA_API_KEY")
    deep_research_model = require_env("DEEP_RESEARCH_MODEL")
    web_search_model = require_env("WEB_SEARCH_MODEL")
    question, report_path = parse_args()
    Path(report_path).parent.mkdir(parents=True, exist_ok=True)

    web_agent = build_web_search_agent()
    deep_agent = build_deep_research_agent(web_agent, report_path)

    print(f"[main:model] {deep_research_model}")
    print(f"[web:model] {web_search_model}")
    print(f"[question] {question}")
    events = deep_agent.run(question)
    print_trace(events)
    print(f"\n[report] {report_path}")
    print(f"[raw-output] {final_output(events)}")
