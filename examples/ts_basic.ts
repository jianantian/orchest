/**
 * Basic example: register tools and run an agent.
 *
 * Usage: npx ts-node examples/ts_basic.ts
 * (requires building the native addon first with @napi-rs/cli)
 */

// In a real setup, import from the built native addon:
// import { Agent } from '@orchest/agent-runtime';

console.log("TypeScript SDK Basic Example");
console.log("============================\n");

// This example shows the intended API. To actually run it,
// build the native addon with @napi-rs/cli first.

/*
const agent = new Agent({
  model: "claude-sonnet-4-20250514",
  systemPrompt: "You are a helpful assistant with access to tools.",
});

agent.registerTool({
  name: "get_weather",
  description: "Get the current weather for a city",
  inputSchema: {
    type: "object",
    properties: {
      city: { type: "string", description: "City name" },
    },
    required: ["city"],
  },
});

agent.registerTool({
  name: "get_time",
  description: "Get the current time in a timezone",
  inputSchema: {
    type: "object",
    properties: {
      timezone: { type: "string", description: "IANA timezone" },
    },
    required: ["timezone"],
  },
});

const events = agent.runSync("What's the weather in Tokyo?");
for (const event of events) {
  const e = event as Record<string, unknown>;
  console.log(`[${e.type}]`, JSON.stringify(e, null, 2));
}
*/

console.log("Build the native addon to run this example.");
console.log("See crates/agent-runtime-node/README.md for instructions.");
