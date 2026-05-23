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
    DEEP_RESEARCH_MIN_CALLS=6

This example is intentionally not mocked. It exercises the SDK shape we want:

- the main deep-research agent has a `web_research` tool backed by another Agent
- the web-search agent has its own model, prompt, context, and Exa tool
- the main agent also gets the core `write_file` tool for report writing
- the sub-agent returns compact evidence so raw search context stays isolated
- the main agent follows a multi-phase research loop before synthesis
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from argparse import ArgumentParser
from datetime import date
from pathlib import Path
from typing import Any

from agent_runtime import Agent, RuntimeEvent


EXA_SEARCH_URL = "https://api.exa.ai/search"
DEFAULT_REPORT_PATH = "target/deep-research-report.md"
MAX_HIGHLIGHT_CHARS = 900
DEFAULT_MIN_RESEARCH_CALLS = 6
SUPPORTED_EXA_CATEGORIES = {
    "company",
    "people",
    "research paper",
    "news",
    "personal site",
    "financial report",
}


def require_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise RuntimeError(f"{name} is required for this non-mock example")
    return value


def provider_url() -> str | None:
    return os.environ.get("ANTHROPIC_API_URL")


def current_date_label() -> str:
    return date.today().isoformat()


def split_csv(value: str) -> list[str]:
    return [part.strip() for part in value.split(",") if part.strip()]


def exa_search(
    query: str,
    rationale: str = "",
    category: str = "",
    include_domains: str = "",
    start_published_date: str = "",
) -> dict[str, Any]:
    """Search the web with Exa highlights for agent workflows."""
    api_key = require_env("EXA_API_KEY")
    payload: dict[str, Any] = {
        "query": query,
        "type": os.environ.get("EXA_SEARCH_TYPE", "auto"),
        "numResults": int(os.environ.get("EXA_NUM_RESULTS", "5")),
        "contents": {"highlights": True},
    }
    if category:
        if category not in SUPPORTED_EXA_CATEGORIES:
            raise RuntimeError(f"unsupported Exa category: {category}")
        payload["category"] = category
    if include_domains:
        payload["includeDomains"] = split_csv(include_domains)
    if start_published_date:
        payload["startPublishedDate"] = start_published_date
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
        "category": category or None,
        "include_domains": split_csv(include_domains) if include_domains else None,
        "start_published_date": start_published_date or None,
        "search_type": data.get("searchType"),
        "request_id": data.get("requestId"),
        "cost_dollars": data.get("costDollars"),
        "result_count": len(data.get("results", [])),
        "results": [
            {
                "title": result.get("title"),
                "url": result.get("url"),
                "published_date": result.get("publishedDate"),
                "author": result.get("author"),
                "evidence": " ".join(result.get("highlights", []))[
                    :MAX_HIGHLIGHT_CHARS
                ],
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
        elif event_type == "sub_agent_started":
            print(f"[sub-agent] started {event.get('config_summary')}")
        elif event_type == "sub_agent_completed":
            print(f"[sub-agent] completed child={event.get('child_run_id')}")
        elif event_type == "child_run_event":
            child = event.get("event", {})
            if isinstance(child, dict) and child.get("type") == "tool_call_started":
                print(f"[sub-agent:tool] {child.get('tool')} input={child.get('input')}")
        elif event_type == "run_completed":
            print(f"\n[final]\n{event.get('output')}")
        elif event_type == "run_failed":
            print(f"\n[error] {event.get('error')}")


def build_web_search_agent() -> Agent:
    today = current_date_label()
    agent = Agent(
        model=require_env("WEB_SEARCH_MODEL"),
        system_prompt=f"""
You are the isolated web-search sub-agent for a deep-research workflow.
Current date: {today}.

Your job is narrow:
1. Read the delegated research assignment.
2. Rewrite it into one high-signal Exa query. Use the actual current year/date
   when freshness matters.
3. Call exa_search exactly once.
4. Return a compact, source-grounded evidence brief.

Use these Exa parameters only when useful:
- category: one of "news", "research paper", "company", "people",
  "personal site", "financial report".
- include_domains: comma-separated domains when the assignment asks for
  official or named-source coverage.
- start_published_date: ISO date when recency is required.

Return markdown with these sections:
- Rewritten query
- Angle researched
- Findings: 3-6 bullets, each tied to at least one source URL
- Source list: title, URL, publication date if available
- Gaps / next queries

Do not answer from memory. If Exa returns weak evidence, say what is missing.
""".strip(),
        api_url=provider_url(),
    )
    agent.register_tool(exa_search)
    return agent


def research_instructions(question: str, report_path: str, min_calls: int) -> str:
    today = current_date_label()
    return f"""
Research question:
{question}

Current date: {today}
Report path: {report_path}

Run a real deep-research workflow before answering. Use web_research as an
isolated sub-agent; each call should delegate exactly one research angle.

Required phases:
1. Broad exploration
   - Call web_research for an initial landscape survey.
   - Call web_research again to identify dimensions, stakeholders, or schools
     of thought.
2. Targeted deep dives
   - Choose the most important dimensions.
   - Call web_research separately for concrete data/statistics, examples or
     case studies, and expert/authoritative views.
3. Diversity and validation
   - Call web_research for challenges, limitations, criticism, or conflicting
     evidence.
   - If the topic is current, include a recency-focused query using {today}.
4. Synthesis check
   - Do not write the final report until you have at least {min_calls}
     web_research calls unless the question is clearly too narrow. If you use
     fewer, explicitly justify why in the report.
   - Verify coverage includes facts/data, examples, expert or authoritative
     sources, trends/current context, and limitations.

Write a markdown report to {report_path} using write_file. The report must
include:
- Executive summary
- Research method: list the search angles used
- Key findings with citations as URLs
- Evidence table: claim, source URL, date, confidence
- Limitations / contradictory evidence
- Remaining open questions
- Final answer

After write_file succeeds, return a concise final message with the report path
and the most important source URLs.
""".strip()


def build_deep_research_agent(
    web_search_agent: Agent,
    report_path: str,
    min_calls: int,
) -> Agent:
    today = current_date_label()
    agent = Agent(
        model=require_env("DEEP_RESEARCH_MODEL"),
        system_prompt=f"""
You are the main deep-research agent.
Current date: {today}.

Architecture:
- web_research is an agent-as-tool. It has its own model, prompt, Exa tool,
  and isolated context. Use it for evidence gathering and query rewriting.
- write_file is a core tool. Use it once to persist the final markdown report.

Research standard:
- Never synthesize from general memory when current or factual claims matter.
- A single search is insufficient for broad questions.
- Search from multiple angles, then validate with criticism or contradictory
  evidence before writing.
- Prefer primary, official, research, reputable news, or expert sources.
- Keep raw search context inside the web-search sub-agent; only use its compact
  briefs in your main synthesis.

Operational rule:
- For normal broad research, perform at least {min_calls} web_research calls
  across broad survey, dimensions, data, cases, expert/official views, and
  limitations. For narrow questions, fewer calls are allowed only if the report
  explains why.
- The report path is {report_path}. Always call write_file before the final
  answer.
""".strip(),
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


def parse_args() -> tuple[str, str, int]:
    parser = ArgumentParser(description="Run the real deep-research agent example.")
    parser.add_argument(
        "question",
        nargs="?",
        default="How should Orchest expose sub-agent-as-tool ergonomics?",
    )
    parser.add_argument("--report", default=DEFAULT_REPORT_PATH)
    parser.add_argument(
        "--min-research-calls",
        type=int,
        default=int(
            os.environ.get("DEEP_RESEARCH_MIN_CALLS", DEFAULT_MIN_RESEARCH_CALLS)
        ),
    )
    args = parser.parse_args()
    return args.question, args.report, args.min_research_calls


if __name__ == "__main__":
    question, report_path, min_calls = parse_args()

    require_env("ANTHROPIC_API_KEY")
    require_env("EXA_API_KEY")
    deep_research_model = require_env("DEEP_RESEARCH_MODEL")
    web_search_model = require_env("WEB_SEARCH_MODEL")
    Path(report_path).parent.mkdir(parents=True, exist_ok=True)

    web_agent = build_web_search_agent()
    deep_agent = build_deep_research_agent(web_agent, report_path, min_calls)

    print(f"[main:model] {deep_research_model}")
    print(f"[web:model] {web_search_model}")
    print(f"[min-research-calls] {min_calls}")
    print(f"[question] {question}")
    events = deep_agent.run(research_instructions(question, report_path, min_calls))
    print_trace(events)
    print(f"\n[report] {report_path}")
    print(f"[raw-output] {final_output(events)}")
