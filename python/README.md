# orchest-py

Python SDK for [Orchest](https://github.com/jianantian/orchest), a
skill-first agent runtime. The agent loop, state, event streaming, tool
dispatch and skill loading run in the Rust core; this package embeds it
in-process.

```bash
pip install orchest-py
```

```python
from orchest import Agent

agent = Agent(
    name="assistant",
    model="anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant.",
    api_key_env="ANTHROPIC_API_KEY",
)

for event in agent.run("What is 2 + 2?"):
    if event["type"] == "model_stream_chunk":
        print(event["delta"]["Text"]["delta"], end="")
```

## Requirements

- Python 3.11 or newer.
- Prebuilt wheels for Linux x86_64, Linux arm64 (glibc 2.28+) and macOS
  arm64. On other platforms pip builds from the source distribution, which
  needs a Rust toolchain.

## Import name

The package installs as `orchest-py` and imports as `orchest`. It cannot be
installed in the same environment as the unrelated `orchest` package on
PyPI (orchest.io's SDK), which uses the same import name.

## Versioning

The SDK is 0.x and versioned independently of the Rust crates; minor
releases may change the Python API. See the
[SDK changelog](https://github.com/jianantian/orchest/blob/main/CHANGELOG-SDK.md)
and the [Python guide](https://github.com/jianantian/orchest/blob/main/docs/guide/sdk-python.md).

## License

Licensed under either of Apache License, Version 2.0 or MIT license at
your option.
