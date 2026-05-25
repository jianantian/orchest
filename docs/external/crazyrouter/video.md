# Crazyrouter — Unified Video API

> Source: https://docs.crazyrouter.com/en/video/unified
> Retrieved: 2026-05-25

## Endpoints

```
POST /v1/video/create    — Create a video generation task
GET  /v1/video/query     — Query task status
```

> `Seedance` is deprecated on the unified contract. Use the native `/volc/v1/contents/generations/tasks` surface instead.

## Supported Models

| Model | Description |
|-------|-------------|
| `veo-3.1-fast` | Google Veo 3.1 Fast |
| `veo-3.1` | Google Veo 3.1 Quality |
| `grok-video-3` | xAI Grok Video |
| `wan-ai/wan2.1-t2v-14b` | Wan text-to-video |
| `wan-ai/wan2.1-i2v-14b-720p` | Wan image-to-video |

## Create Video

```
POST /v1/video/create
```

### Request Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `model` | string | Yes | Model name |
| `prompt` | string | Yes | Video description |
| `aspect_ratio` | string | No | `16:9`, `9:16`, `1:1` |
| `size` | string | No | e.g. `1280x720` |
| `images` | array | No | Image URLs for image-to-video |
| `duration` | integer | No | Duration in seconds |

### Request Examples

```bash
curl -X POST https://crazyrouter.com/v1/video/create \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "veo-3.1",
    "prompt": "A golden retriever running on the beach, slow motion, cinematic quality",
    "aspect_ratio": "16:9"
  }'
```

```python
import requests

response = requests.post(
    "https://crazyrouter.com/v1/video/create",
    headers={
        "Content-Type": "application/json",
        "Authorization": "Bearer YOUR_API_KEY"
    },
    json={
        "model": "veo-3.1",
        "prompt": "A golden retriever running on the beach, slow motion",
        "aspect_ratio": "16:9"
    }
)

data = response.json()
task_id = data["id"]
print(f"Task ID: {task_id}")
```

### Create Response

```json
{
  "id": "video_task_abc123",
  "status": "processing",
  "status_update_time": 1709123456
}
```

## Image-to-Video

```bash
curl -X POST https://crazyrouter.com/v1/video/create \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "wan-ai/wan2.1-i2v-14b-720p",
    "prompt": "The person in the image starts smiling and waving",
    "images": ["https://example.com/portrait.jpg"],
    "aspect_ratio": "16:9"
  }'
```

## Query Task

```
GET /v1/video/query?id={task_id}
```

### Processing Response

```json
{
  "id": "video_task_abc123",
  "status": "processing",
  "status_update_time": 1709123460
}
```

### Completed Response

```json
{
  "id": "video_task_abc123",
  "status": "completed",
  "status_update_time": 1709123520,
  "video_url": "https://crazyrouter.com/files/video_abc123.mp4"
}
```

### Task Statuses

| Status | Description |
|--------|-------------|
| `processing` | In progress |
| `completed` | Done |
| `failed` | Failed |

## Complete Workflow

```python
import requests
import time

API_KEY = "YOUR_API_KEY"
BASE_URL = "https://crazyrouter.com"
headers = {
    "Content-Type": "application/json",
    "Authorization": f"Bearer {API_KEY}"
}

# 1. Create
resp = requests.post(f"{BASE_URL}/v1/video/create", headers=headers, json={
    "model": "veo-3.1",
    "prompt": "A golden retriever running on the beach, slow motion",
    "aspect_ratio": "16:9"
})
task_id = resp.json()["id"]

# 2. Poll
while True:
    resp = requests.get(f"{BASE_URL}/v1/video/query?id={task_id}", headers=headers)
    result = resp.json()
    if result["status"] == "completed":
        print(f"Video URL: {result['video_url']}")
        break
    elif result["status"] == "failed":
        print("Failed")
        break
    time.sleep(10)
```

> Video generation typically takes 1-5 minutes. Poll at 10-second intervals.
