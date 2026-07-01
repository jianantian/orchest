"""Deep research example using real models, Exa, and agent-as-tool.

Required environment:

    cp examples/support/deep_research.env.example .env

    EXA_API_KEY=...
    DEEP_RESEARCH_MODEL=...      # Main synthesis model
    WEB_SEARCH_MODEL=...         # Query rewrite/search model

Also set the provider API key for each configured model, for example:

    ANTHROPIC_API_KEY=...
    OPENAI_API_KEY=...
    DEEPSEEK_API_KEY=...
    OPENROUTER_API_KEY=...

Optional environment:

    ANTHROPIC_API_URL=...        # Anthropic-compatible /v1/messages endpoint
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

from orchest import Agent, RuntimeEvent

EXA_SEARCH_URL = "https://api.exa.ai/search"
DEFAULT_REPORT_PATH = "target/deep-research-report.md"
PROMPT_DIR = Path(__file__).resolve().parents[2] / "support" / "deep_research_prompts"
ENV_PATH = Path(__file__).resolve().parents[3] / ".env"
MAX_HIGHLIGHT_CHARS = 900
DEFAULT_MIN_RESEARCH_CALLS = 6
DEFAULT_MAX_TOKENS = 16000
SUPPORTED_EXA_CATEGORIES = {
    "company",
    "people",
    "research paper",
    "news",
    "personal site",
    "financial report",
}


def load_dotenv(path: Path = ENV_PATH) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        if not key or key in os.environ:
            continue
        stripped = value.strip().strip('"').strip("'")
        if not stripped:
            continue
        os.environ[key] = stripped


def prompt_template(name: str, **values: object) -> str:
    prompt = (PROMPT_DIR / f"{name}.md").read_text(encoding="utf-8").strip()
    for key, value in values.items():
        prompt = prompt.replace(f"{{{{{key}}}}}", str(value))
    return prompt


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


def collect_and_print(event: RuntimeEvent, events: list[RuntimeEvent]) -> None:
    events.append(event)
    print_event(event)


def print_event(event: RuntimeEvent) -> None:
    event_type = event.get("type")
    if event_type == "tool_call_started":
        print(f"[tool] {event.get('tool')} input={event.get('input')}", flush=True)
    elif event_type == "tool_call_completed":
        print(f"[tool:done] {event.get('tool')}", flush=True)
    elif event_type == "sub_agent_started":
        print(f"[sub-agent] started {event.get('config_summary')}", flush=True)
    elif event_type == "sub_agent_completed":
        print(f"[sub-agent] completed child={event.get('child_run_id')}", flush=True)
    elif event_type == "child_run_event":
        child = event.get("event", {})
        if isinstance(child, dict) and child.get("type") == "tool_call_started":
            print(
                f"[sub-agent:tool] {child.get('tool')} input={child.get('input')}",
                flush=True,
            )
    elif event_type == "run_completed":
        print(f"\n[final]\n{event.get('output')}", flush=True)
    elif event_type == "run_failed":
        print(f"\n[error] {event.get('error')}", flush=True)


def build_web_search_agent() -> Agent:
    today = current_date_label()
    agent = Agent(
        model=require_env("WEB_SEARCH_MODEL"),
        system_prompt=prompt_template("web_search_system", today=today),
        api_url=provider_url(),
    )
    agent.register_tool(exa_search)
    return agent


def research_instructions(question: str, report_path: str, min_calls: int) -> str:
    today = current_date_label()
    return prompt_template(
        "research_instructions",
        question=question,
        today=today,
        report_path=report_path,
        min_calls=min_calls,
    )


def build_deep_research_agent(
    web_search_agent: Agent,
    report_path: str,
    min_calls: int,
) -> Agent:
    today = current_date_label()
    max_tokens = int(os.environ.get("DEEP_RESEARCH_MAX_TOKENS", DEFAULT_MAX_TOKENS))
    agent = Agent(
        model=require_env("DEEP_RESEARCH_MODEL"),
        system_prompt=prompt_template(
            "main_system",
            today=today,
            report_path=report_path,
            min_calls=min_calls,
        ),
        api_url=provider_url(),
        max_tokens=max_tokens,
    )
    agent.register_agent_tool(
        name="web_research",
        description="Delegate web research to an isolated web-search sub-agent.",
        agent=web_search_agent,
        input_key="question",
    )
    agent.register_write_file_tool(approval="never")
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
    load_dotenv()
    question, report_path, min_calls = parse_args()

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
    events: list[RuntimeEvent] = []
    deep_agent.run_stream(
        research_instructions(question, report_path, min_calls),
        lambda event: collect_and_print(event, events),
    )
    print(f"\n[report] {report_path}")
    print(f"[raw-output] {final_output(events)}")
