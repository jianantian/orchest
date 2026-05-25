# Renderful — Python SDK

> Source: https://renderful.ai/zh/docs/python-sdk
> Retrieved: 2026-05-25
>
> Single-file module. No pip install needed (requires `requests`).

## Installation

Copy the `Renderful` class below into your project, or save as `renderful.py`. Requires `pip install requests`.

## The Module

```python
import os, time, requests

class Renderful:
    """Renderful API client — copy this class into your project."""

    BASE = "https://api.renderful.ai/api/v1"

    def __init__(self, api_key: str | None = None):
        self.api_key = api_key or os.environ["RENDERFUL_API_KEY"]
        self.session = requests.Session()
        self.session.headers.update({
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
        })

    # --- core methods ---

    def create(self, body: dict) -> dict:
        """Submit a generation task. Returns the task object."""
        r = self.session.post(f"{self.BASE}/generations", json=body)
        r.raise_for_status()
        return r.json()

    def get(self, task_id: str) -> dict:
        """Fetch current status of a task."""
        r = self.session.get(f"{self.BASE}/generations/{task_id}")
        r.raise_for_status()
        return r.json()

    def poll(self, task_id: str, interval: float = 3, timeout: float = 300) -> dict:
        """Poll until completed or failed. Returns final task object."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            data = self.get(task_id)
            if data["status"] in ("completed", "failed"):
                return data
            time.sleep(interval)
        raise TimeoutError(f"Task {task_id} did not finish within {timeout}s")

    def run(self, body: dict, **poll_kw) -> dict:
        """Create + poll in one call. Returns completed task."""
        task = self.create(body)
        return self.poll(task["id"], **poll_kw)

    # --- file upload ---

    def upload(self, file_path: str) -> str:
        """Upload a local file and return its URL for use in requests."""
        with open(file_path, "rb") as f:
            r = self.session.post(
                f"{self.BASE}/uploads",
                files={"file": f},
                headers={"Authorization": f"Bearer {self.api_key}"},
            )
        r.raise_for_status()
        return r.json()["url"]
```

## Authentication

```python
# Env variable (recommended)
# export RENDERFUL_API_KEY="rf_..."
client = Renderful()  # reads from env

# Direct
client = Renderful(api_key="rf_...")
```

## Usage Examples

### Text to Image

```python
from renderful import Renderful

client = Renderful()
result = client.run({
    "type": "text-to-image",
    "model": "flux-dev",
    "prompt": "A cyberpunk cityscape at night",
})
print(result["outputs"])  # ["https://..."]
```

### Text to Video

```python
result = client.run({
    "type": "text-to-video",
    "model": "wan-2.6",
    "prompt": "A drone flyover of a tropical island",
})
print(result["outputs"])  # ["https://...mp4"]
```

### Image to Video

```python
# Upload a local image first
image_url = client.upload("photo.jpg")

result = client.run({
    "type": "image-to-video",
    "model": "wan-2.6-i2v",
    "image_url": image_url,
    "prompt": "Slow zoom into the scene",
})
print(result["outputs"])
```

### Text to Text (LLM)

```python
result = client.run({
    "type": "text-to-text",
    "model": "kimi-k2.5",
    "prompt": "Explain quantum computing in 3 sentences",
    "max_tokens": 256,
})
print(result["outputs"][0])
```

## Error Handling

```python
from requests.exceptions import HTTPError

try:
    result = client.run({
        "type": "text-to-image",
        "model": "flux-dev",
        "prompt": "A sunset",
    })
except HTTPError as e:
    print(f"API error {e.response.status_code}: {e.response.text}")
except TimeoutError:
    print("Generation took too long")

# Check for failed tasks
if result["status"] == "failed":
    print("Task failed:", result.get("error"))
```

## Fire-and-Forget Mode

Use `create()` instead of `run()` if you prefer webhooks over polling:

```python
task = client.create({
    "type": "text-to-image",
    "model": "flux-dev",
    "prompt": "A sunset",
    "webhook_url": "https://your-app.com/webhooks/renderful",
})
print(f"Task submitted: {task['id']}")
# Your webhook endpoint will receive the result
```
