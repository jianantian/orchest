"""Anthropic provider configuration using a canonical provider model string."""

from agent_runtime import Agent


agent = Agent(
    model="anthropic/claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant.",
    api_key_env="ANTHROPIC_API_KEY",
    request_options={
        "thinking": "medium",
        "include_thinking": False,
        "max_tokens": 1024,
    },
)

print(agent)
