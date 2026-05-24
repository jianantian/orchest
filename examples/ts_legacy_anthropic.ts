import { Agent } from "../js";

const agent = new Agent({
  model: "claude-sonnet-4-20250514",
  systemPrompt: "You are a helpful assistant.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
});

console.log(agent);
