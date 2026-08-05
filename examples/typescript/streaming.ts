/**
 * Streaming example: print model text deltas from runtime events.
 *
 * Usage:
 *   cargo build -p orchest-node
 *   npx ts-node --compiler-options '{"module":"CommonJS"}' examples/typescript/streaming.ts
 */

declare const __dirname: string;
declare const process: any;
declare function require(name: string): any;

const { spawn, spawnSync } = require("node:child_process");
const { copyFileSync, existsSync } = require("node:fs");
const { join, resolve } = require("node:path");

const repoRoot = resolve(__dirname, "../..");
const nativeSource = join(repoRoot, "target/debug/liborchest_node.dylib");
const nativeAddon = join(repoRoot, "target/debug/orchest_node.node");

if (existsSync(nativeSource)) {
  copyFileSync(nativeSource, nativeAddon);
  if (process.platform === "darwin") {
    spawnSync("codesign", ["--force", "--sign", "-", nativeAddon]);
  }
}

const { Agent } = require(nativeAddon);

function startProvider(port: number) {
  const child = spawn("python3", ["examples/support/mock_anthropic_provider.py", String(port)], {
    cwd: repoRoot,
    stdio: "ignore",
  });
  process.on("exit", () => child.kill());
  return child;
}

function textDelta(delta: unknown): string {
  const value = delta as Record<string, unknown>;
  const text = (value.Text || value.text) as Record<string, unknown> | undefined;
  return String(text?.delta ?? "");
}

const port = 8798;
const provider = process.env.ANTHROPIC_API_KEY ? undefined : startProvider(port);
process.env.ANTHROPIC_API_KEY ||= "local-demo-key";

setTimeout(async () => {
  const agent = new Agent({
    name: "streaming-assistant",
    model: "anthropic/claude-sonnet-4-20250514",
    systemPrompt: "You are a helpful assistant.",
    apiUrl: process.env.ANTHROPIC_API_URL || `http://127.0.0.1:${port}/v1/messages`,
    budget: { maxToolCalls: 5, maxCostUsd: 0.1 },
  });

  agent.registerTool({
    name: "get_weather",
    description: "Get the current weather for a city",
    inputSchema: {
      type: "object",
      properties: { city: { type: "string" } },
      required: ["city"],
    },
  });

  const events = await agent.runSync("Tell me the Tokyo weather in one short sentence.");
  for (const event of events as Array<Record<string, unknown>>) {
    switch (event.type) {
      case "model_stream_chunk":
        process.stdout.write(textDelta(event.delta));
        break;
      case "tool_call_started":
        console.log(`\n[Tool] ${event.tool} called`);
        break;
      case "tool_call_completed":
        console.log(`[Tool] ${event.tool} -> ${JSON.stringify(event.output)}`);
        break;
      case "run_completed":
        console.log(`\n[Done] Run completed`);
        break;
      case "run_failed":
        console.error(`\n[Error] ${event.error}`);
        process.exitCode = 1;
        break;
    }
  }
  provider?.kill();
}, 100);
