import { Agent } from "../js";

const agent = new Agent({
  model: "openrouter/anthropic/claude-sonnet-4",
  systemPrompt: "You are a helpful assistant.",
  apiKeyEnv: "OPENROUTER_API_KEY",
  requestOptions: {
    thinking: "high",
    includeThinking: true,
    maxTokens: 1024,
  },
});

console.log(agent);
