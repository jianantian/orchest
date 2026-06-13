/**
 * Anthropic provider example: tool calling with Claude.
 *
 * Falls back to a local mock provider when ANTHROPIC_API_KEY is not set,
 * so this example always runs out of the box.
 *
 * Usage:
 *   cargo build -p agent-runtime-node
 *   npx ts-node --compiler-options '{"module":"CommonJS"}' examples/typescript/providers/anthropic.ts
 *
 * Requires (optional): ANTHROPIC_API_KEY
 */

declare const __dirname: string;
declare const process: any;
declare function require(name: string): any;

const { spawn, spawnSync } = require("node:child_process");
const { copyFileSync, existsSync } = require("node:fs");
const { join, resolve } = require("node:path");

const repoRoot = resolve(__dirname, "../../..");
const nativeSource = join(repoRoot, "target/debug/libagent_runtime_node.dylib");
const nativeAddon = join(repoRoot, "target/debug/agent_runtime_node.node");

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

const port = 8799;
const provider = process.env.ANTHROPIC_API_KEY ? undefined : startProvider(port);
process.env.ANTHROPIC_API_KEY ||= "local-demo-key";

setTimeout(async () => {
  const agent = new Agent({
    model: "anthropic/claude-sonnet-4-20250514",
    systemPrompt: "You are a helpful assistant. Answer concisely.",
    apiKeyEnv: "ANTHROPIC_API_KEY",
    apiUrl: process.env.ANTHROPIC_API_URL || (provider ? `http://127.0.0.1:${port}/v1/messages` : undefined),
    requestOptions: { thinking: "off", maxTokens: 512 },
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
  provider?.kill();
}, 100);
