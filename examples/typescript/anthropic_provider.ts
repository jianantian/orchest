import { Agent } from "../../js";

const agent = new Agent({
  model: "anthropic/claude-sonnet-4-20250514",
  systemPrompt: "You are a helpful assistant.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
  requestOptions: {
    thinking: "medium",
    includeThinking: false,
    maxTokens: 1024,
  },
});

console.log(agent);
