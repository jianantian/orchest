"""Legacy Anthropic shorthand remains supported."""

from agent_runtime import Agent


agent = Agent(
    model="claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant.",
    api_key_env="ANTHROPIC_API_KEY",
)

print(agent)
