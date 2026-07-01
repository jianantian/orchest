/**
 * Async tool example: simulate a video generation task with progress.
 *
 * Usage:
 *   cargo build -p orchest-node
 *   npx ts-node --compiler-options '{"module":"CommonJS"}' examples/typescript/agents/async_tool.ts
 */

declare const __dirname: string;
declare const process: any;
declare function require(name: string): any;

const { spawn, spawnSync } = require("node:child_process");
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

const port = 8800;
const provider = process.env.ANTHROPIC_API_KEY ? undefined : startProvider(port);
process.env.ANTHROPIC_API_KEY ||= "local-demo-key";

setTimeout(async () => {
  const agent = new Agent({
    model: "anthropic/claude-sonnet-4-20250514",
    systemPrompt: "You are a helpful assistant that can generate videos.",
    apiUrl: process.env.ANTHROPIC_API_URL || `http://127.0.0.1:${port}/v1/messages`,
    budget: { maxToolCalls: 5 },
  });

  // Per-job poll state, keyed by job_id
  const pollCounts = new Map<string, number>();

  agent.registerAsyncToolWithHandler(
    "generate_video",
    "Generate a video from a text prompt. Returns immediately with a job ID.",
    {
      type: "object",
      properties: {
        prompt: { type: "string" },
        duration_seconds: { type: "number" },
      },
      required: ["prompt", "duration_seconds"],
    },
    // Initial handler: starts the job, returns { job_id, poll_interval_ms }
    (input: Record<string, unknown>) => {
      const jobId = "vid_abc123";
      pollCounts.set(jobId, 0);
      return { job_id: jobId, poll_interval_ms: 10 };
    },
    // Poll handler: called with job_id until status is "completed" or "failed"
    (jobId: string) => {
      const count = (pollCounts.get(jobId) ?? 0) + 1;
      pollCounts.set(jobId, count);
      if (count === 1) {
        return { status: "pending", progress: 0.5, message: "rendering" };
      }
      pollCounts.delete(jobId);
      return {
        status: "completed",
        result: {
          video_url: "file:///tmp/demo-video.mp4",
        },
      };
    },
  );

  const events = await agent.runSync("Generate a 5-second video of a cat playing piano");
  for (const event of events as Array<Record<string, unknown>>) {
    switch (event.type) {
      case "model_stream_chunk":
        process.stdout.write(textDelta(event.delta));
        break;
      case "tool_call_started":
        console.log(`\n[Tool] ${event.tool} started`);
        break;
      case "async_tool_progress": {
        const status = event.status as Record<string, unknown>;
        const pending = (status.Pending || status.pending) as Record<string, unknown> | undefined;
        console.log(`[Tool] Progress: ${((pending?.progress as number) ?? 0) * 100 | 0}%`);
        break;
      }
      case "async_tool_completed":
        console.log(`[Tool] Completed: ${JSON.stringify(event.output)}`);
        break;
      case "tool_call_completed":
        console.log(`[Tool] Result: ${JSON.stringify(event.output)}`);
        break;
      case "run_completed":
        console.log(`\n[Done] ${event.output}`);
        break;
      case "run_failed":
        console.error(`\n[Error] ${event.error}`);
        process.exitCode = 1;
        break;
    }
  }
  provider?.kill();
}, 100);
