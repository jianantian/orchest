"""DeepSeek provider configuration with thinking options."""

from agent_runtime import Agent


agent = Agent(
    model="deepseek/deepseek-reasoner",
    system_prompt="You are a concise reasoning assistant.",
    api_key_env="DEEPSEEK_API_KEY",
    request_options={
        "thinking": "high",
        "include_thinking": True,
        "max_tokens": 1024,
    },
)

print(agent)
