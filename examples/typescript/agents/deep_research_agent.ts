/**
 * Deep research example using real models, Exa, and agent-as-tool.
 *
 * Required environment:
 *   cp examples/support/deep_research.env.example .env
 *
 *   EXA_API_KEY=...
 *   DEEP_RESEARCH_MODEL=...
 *   WEB_SEARCH_MODEL=...
 *
 * Also set the provider API key for each configured model, such as
 * ANTHROPIC_API_KEY, OPENAI_API_KEY, DEEPSEEK_API_KEY, or OPENROUTER_API_KEY.
 */

declare const __dirname: string;
declare const process: any;
declare function require(name: string): any;

const { execFileSync } = require("node:child_process");
const { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } = require("node:fs");
const { dirname, join, resolve } = require("node:path");

const repoRoot = resolve(__dirname, "../../..");
const nativeSource = join(repoRoot, "target/debug/liborchest_node.dylib");
const nativeAddon = join(repoRoot, "target/debug/orchest_node.node");
const promptDir = join(repoRoot, "examples/support/deep_research_prompts");
const envPath = join(repoRoot, ".env");

if (!existsSync(nativeAddon)) {
  copyFileSync(nativeSource, nativeAddon);
}

const { Agent } = require(nativeAddon);

const EXA_SEARCH_URL = "https://api.exa.ai/search";
const DEFAULT_REPORT_PATH = "target/deep-research-report.md";
const DEFAULT_MIN_RESEARCH_CALLS = 6;
const DEFAULT_MAX_TOKENS = 16000;
const MAX_HIGHLIGHT_CHARS = 900;
const SUPPORTED_EXA_CATEGORIES = new Set([
  "company",
  "people",
  "research paper",
  "news",
  "personal site",
  "financial report",
]);

function loadDotenv(): void {
  if (!existsSync(envPath)) {
    return;
  }
  for (const rawLine of readFileSync(envPath, "utf8").split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#") || !line.includes("=")) {
      continue;
    }
    const [rawKey, ...rawValue] = line.split("=");
    const key = rawKey.trim();
    if (!key || process.env[key] !== undefined) {
      continue;
    }
    const value = rawValue.join("=").trim().replace(/^['"]|['"]$/g, "");
    if (!value) {
      continue;
    }
    process.env[key] = value;
  }
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required for this non-mock example`);
  }
  return value;
}

function currentDateLabel(): string {
  return new Date().toISOString().slice(0, 10);
}

function promptTemplate(name: string, values: Record<string, unknown>): string {
  let prompt = readFileSync(join(promptDir, `${name}.md`), "utf8").trim();
  for (const [key, value] of Object.entries(values)) {
    prompt = prompt.split(`{{${key}}}`).join(String(value));
  }
  return prompt;
}

function splitCsv(value?: string): string[] {
  return String(value ?? "")
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function exaSearch(input: Record<string, unknown>): Record<string, unknown> {
  const query = String(input.query ?? "");
  const rationale = String(input.rationale ?? "");
  const category = String(input.category ?? "");
  const includeDomains = String(input.include_domains ?? "");
  const startPublishedDate = String(input.start_published_date ?? "");

  const payload: Record<string, unknown> = {
    query,
    type: process.env.EXA_SEARCH_TYPE || "auto",
    numResults: Number(process.env.EXA_NUM_RESULTS || "5"),
    contents: { highlights: true },
  };
  if (category) {
    if (!SUPPORTED_EXA_CATEGORIES.has(category)) {
      throw new Error(`unsupported Exa category: ${category}`);
    }
    payload.category = category;
  }
  if (includeDomains) {
    payload.includeDomains = splitCsv(includeDomains);
  }
  if (startPublishedDate) {
    payload.startPublishedDate = startPublishedDate;
  }
  if (process.env.EXA_LIVECRAWL === "1") {
    payload.contents = { highlights: true, maxAgeHours: 0 };
  }

  const response = execFileSync(
    "curl",
    [
      "--fail-with-body",
      "--silent",
      "--show-error",
      "--max-time",
      "30",
      "-X",
      "POST",
      EXA_SEARCH_URL,
      "-H",
      "Content-Type: application/json",
      "-H",
      `x-api-key: ${requireEnv("EXA_API_KEY")}`,
      "--data",
      JSON.stringify(payload),
    ],
    { encoding: "utf8" },
  );
  const data = JSON.parse(response);
  const results = Array.isArray(data.results) ? data.results : [];

  return {
    provider: "exa",
    query,
    rationale,
    category: category || null,
    include_domains: includeDomains ? splitCsv(includeDomains) : null,
    start_published_date: startPublishedDate || null,
    search_type: data.searchType,
    request_id: data.requestId,
    cost_dollars: data.costDollars,
    result_count: results.length,
    results: results.map((result: Record<string, unknown>) => ({
      title: result.title,
      url: result.url,
      published_date: result.publishedDate,
      author: result.author,
      evidence: (Array.isArray(result.highlights) ? result.highlights.join(" ") : "").slice(
        0,
        MAX_HIGHLIGHT_CHARS,
      ),
    })),
  };
}

function finalOutput(events: Array<Record<string, unknown>>): unknown {
  for (const event of [...events].reverse()) {
    if (event.type === "run_completed") {
      return event.output;
    }
  }
  return null;
}

function printEvent(event: Record<string, unknown>): void {
  if (event.type === "tool_call_started") {
    console.log(`[tool] ${event.tool} input=${JSON.stringify(event.input)}`);
  } else if (event.type === "tool_call_completed") {
    console.log(`[tool:done] ${event.tool}`);
  } else if (event.type === "sub_agent_started") {
    console.log(`[sub-agent] started ${JSON.stringify(event.config_summary)}`);
  } else if (event.type === "sub_agent_completed") {
    console.log(`[sub-agent] completed child=${event.child_run_id}`);
  } else if (event.type === "child_run_event") {
    const child = event.event as Record<string, unknown> | undefined;
    if (child?.type === "tool_call_started") {
      console.log(`[sub-agent:tool] ${child.tool} input=${JSON.stringify(child.input)}`);
    }
  } else if (event.type === "run_completed") {
    console.log(`\n[final]\n${event.output}`);
  } else if (event.type === "run_failed") {
    console.log(`\n[error] ${event.error}`);
  }
}

function buildWebSearchAgent(): any {
  const agent = new Agent({
    model: requireEnv("WEB_SEARCH_MODEL"),
    systemPrompt: promptTemplate("web_search_system", { today: currentDateLabel() }),
    apiUrl: process.env.ANTHROPIC_API_URL || undefined,
  });
  agent.registerToolWithHandler(
    "exa_search",
    "Search the web with Exa highlights for agent workflows.",
    {
      type: "object",
      properties: {
        query: { type: "string" },
        rationale: { type: "string" },
        category: { type: "string" },
        include_domains: { type: "string" },
        start_published_date: { type: "string" },
      },
      required: ["query"],
    },
    exaSearch,
  );
  return agent;
}

function researchInstructions(question: string, reportPath: string, minCalls: number): string {
  return promptTemplate("research_instructions", {
    question,
    today: currentDateLabel(),
    report_path: reportPath,
    min_calls: minCalls,
  });
}

function buildDeepResearchAgent(webSearchAgent: any, reportPath: string, minCalls: number): any {
  const maxTokens = Number(process.env.DEEP_RESEARCH_MAX_TOKENS || DEFAULT_MAX_TOKENS);
  const agent = new Agent({
    model: requireEnv("DEEP_RESEARCH_MODEL"),
    systemPrompt: promptTemplate("main_system", {
      today: currentDateLabel(),
      report_path: reportPath,
      min_calls: minCalls,
    }),
    apiUrl: process.env.ANTHROPIC_API_URL || undefined,
    maxTokens,
  });

  agent.registerToolWithHandler(
    "web_research",
    "Delegate web research to an isolated web-search sub-agent.",
    {
      type: "object",
      properties: {
        question: {
          type: "string",
          description: "Task or question to delegate to the child agent",
        },
      },
      required: ["question"],
    },
    (input: Record<string, unknown>) => finalOutput(webSearchAgent.runSync(String(input.question))),
  );

  agent.registerToolWithHandler(
    "write_file",
    "Write UTF-8 text content to a file, creating parent directories if needed",
    {
      type: "object",
      properties: {
        path: { type: "string" },
        content: { type: "string" },
      },
      required: ["path", "content"],
    },
    (input: Record<string, unknown>) => {
      const path = String(input.path);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, String(input.content), "utf8");
      return { path, bytes_written: String(input.content).length };
    },
    { sideEffect: true },
  );
  return agent;
}

function parseArgs(): { question: string; reportPath: string; minCalls: number } {
  const args = process.argv.slice(2);
  let question = "How should Orchest expose sub-agent-as-tool ergonomics?";
  let reportPath = DEFAULT_REPORT_PATH;
  let minCalls = Number(process.env.DEEP_RESEARCH_MIN_CALLS || DEFAULT_MIN_RESEARCH_CALLS);

  for (let i = 0; i < args.length; i += 1) {
    if (args[i] === "--report") {
      reportPath = args[(i += 1)];
    } else if (args[i] === "--min-research-calls") {
      minCalls = Number(args[(i += 1)]);
    } else {
      question = args.slice(i).join(" ");
      break;
    }
  }
  return { question, reportPath, minCalls };
}

loadDotenv();

const { question, reportPath, minCalls } = parseArgs();
requireEnv("EXA_API_KEY");
console.log(`[main:model] ${requireEnv("DEEP_RESEARCH_MODEL")}`);
console.log(`[web:model] ${requireEnv("WEB_SEARCH_MODEL")}`);
console.log(`[min-research-calls] ${minCalls}`);
console.log(`[question] ${question}`);

const webAgent = buildWebSearchAgent();
const deepAgent = buildDeepResearchAgent(webAgent, reportPath, minCalls);
const events: Array<Record<string, unknown>> = [];
deepAgent.runStream(
  researchInstructions(question, reportPath, minCalls),
  (event: Record<string, unknown>) => {
    events.push(event);
    printEvent(event);
  },
);
console.log(`\n[report] ${reportPath}`);
console.log(`[raw-output] ${JSON.stringify(finalOutput(events))}`);
