/**
 * DeepSeek V4 example: reasoning model with tool calling.
 *
 * Shows DeepSeek's native thinking/reasoning blocks alongside tool use.
 * Uses deepseek-v4-pro for best reasoning quality.
 *
 * Usage:
 *   cargo build -p orchest-node
 *   DEEPSEEK_API_KEY=sk-... npx ts-node --compiler-options '{"module":"CommonJS"}' examples/typescript/providers/deepseek.ts
 *
 * Requires: DEEPSEEK_API_KEY
 */

declare const __dirname: string;
declare const process: any;
declare function require(name: string): any;

const { spawnSync } = require("node:child_process");
const { copyFileSync, existsSync } = require("node:fs");
const { join, resolve } = require("node:path");

const repoRoot = resolve(__dirname, "../../..");
const nativeSource = join(repoRoot, "target/debug/liborchest_node.dylib");
const nativeAddon = join(repoRoot, "target/debug/orchest_node.node");

if (existsSync(nativeSource)) {
  copyFileSync(nativeSource, nativeAddon);
  if (process.platform === "darwin") {
    spawnSync("codesign", ["--force", "--sign", "-", nativeAddon]);
  }
}

const { Agent } = require(nativeAddon);

function extractDelta(delta: unknown): { text: string; reasoning: string } {
  const value = delta as Record<string, unknown>;
  const t = (value.Text || value.text) as Record<string, unknown> | undefined;
  const r = (value.Thinking || value.thinking) as Record<string, unknown> | undefined;
  return {
    text: String(t?.delta ?? ""),
    reasoning: String(r?.delta ?? ""),
  };
}

if (!process.env.DEEPSEEK_API_KEY) {
  console.error("Set DEEPSEEK_API_KEY first.");
  process.exit(1);
}

const agent = new Agent({
  name: "deepseek-assistant",
  model: "deepseek/deepseek-v4-pro",
  systemPrompt: "You are a concise reasoning assistant. Think step by step.",
  apiKeyEnv: "DEEPSEEK_API_KEY",
  requestOptions: { thinking: "high" },
});

agent.registerToolWithHandler(
  "get_weather",
  "Get the current weather for a city",
  {
    type: "object",
    properties: { city: { type: "string" } },
    required: ["city"],
  },
  (input: Record<string, unknown>) => ({
    city: input.city,
    temperature: 22,
    condition: "sunny",
  }),
);

(async () => {
  const events = await agent.runSync(
    "What's the weather in Tokyo and should I bring an umbrella? Explain your reasoning.",
  );
  for (const event of events as Array<Record<string, unknown>>) {
    switch (event.type) {
      case "model_stream_chunk": {
        const { text, reasoning } = extractDelta(event.delta);
        if (reasoning) process.stdout.write(`\x1b[90m${reasoning}\x1b[0m`); // dim/gray
        if (text) process.stdout.write(text);
        break;
      }
      case "tool_call_started":
        console.log(`\n\n[Tool] ${event.tool}(${JSON.stringify(event.input)})`);
        break;
      case "tool_call_completed":
        console.log(`[Result] ${JSON.stringify(event.output)}\n`);
        break;
      case "run_completed":
        console.log("\n\nDone.");
        break;
      case "run_failed":
        console.error(`\n[Error] ${event.error}`);
        process.exitCode = 1;
        break;
    }
  }
})();
