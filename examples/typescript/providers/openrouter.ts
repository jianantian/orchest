/**
 * OpenRouter provider example: Claude via OpenRouter with tool calling.
 *
 * Usage:
 *   cargo build -p orchest-node
 *   OPENROUTER_API_KEY=sk-or-... npx ts-node --compiler-options '{"module":"CommonJS"}' examples/typescript/providers/openrouter.ts
 *
 * Requires: OPENROUTER_API_KEY
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

function textDelta(delta: unknown): string {
  const value = delta as Record<string, unknown>;
  const text = (value.Text || value.text) as Record<string, unknown> | undefined;
  return String(text?.delta ?? "");
}

if (!process.env.OPENROUTER_API_KEY) {
  console.error("Set OPENROUTER_API_KEY first.");
  process.exit(1);
}

const agent = new Agent({
  model: "openrouter/anthropic/claude-sonnet-4-6",
  systemPrompt: "You are a helpful assistant. Answer concisely.",
  apiKeyEnv: "OPENROUTER_API_KEY",
  requestOptions: { thinking: "minimal", maxTokens: 512 },
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
  const events = await agent.runSync("What's the weather in Tokyo? One sentence.");
  for (const event of events as Array<Record<string, unknown>>) {
    switch (event.type) {
      case "model_stream_chunk":
        process.stdout.write(textDelta(event.delta));
        break;
      case "tool_call_started":
        console.log(`\n[Tool] ${event.tool}(${JSON.stringify(event.input)})`);
        break;
      case "tool_call_completed":
        console.log(`[Result] ${JSON.stringify(event.output)}`);
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
