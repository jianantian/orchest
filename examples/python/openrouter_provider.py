"""OpenRouter provider configuration with a nested routed model string."""

from agent_runtime import Agent


agent = Agent(
    model="openrouter/anthropic/claude-sonnet-4",
    system_prompt="You are a helpful assistant.",
    api_key_env="OPENROUTER_API_KEY",
    request_options={
        "thinking": "high",
        "include_thinking": True,
        "max_tokens": 1024,
    },
)

print(agent)
