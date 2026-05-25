# Crazyrouter — GPT Image API

> Source: https://docs.crazyrouter.com/en/images/gpt-image
> Retrieved: 2026-05-25

## Endpoints

```
POST /v1/images/generations
POST /v1/images/edits
```

Model: `gpt-image-2`

> Use `https://cn.crazyrouter.com/v1` for image routes. Account, billing, and console remain on `https://crazyrouter.com`.

## Generate Image

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `model` | string | Yes | Fixed: `gpt-image-2` |
| `prompt` | string | Yes | Image description |
| `n` | integer | No | Default `1`, range `1-10` |
| `size` | string | No | `auto` or `WxH`; multiples of 16, max 3840 per side, 655360–8294400 total pixels, ratio ≤ 3:1 |
| `quality` | string | No | `auto`, `low`, `medium`, `high`; `hd` → `high`; `standard` rejected |
| `background` | string | No | `auto` or `opaque` (no transparent) |
| `output_format` | string | No | `png`, `jpeg`, `webp` |
| `output_compression` | integer | No | `0-100` (jpeg/webp only) |
| `moderation` | string | No | `auto` or `low` |
| `stream` | boolean | No | SSE stream |
| `partial_images` | integer | No | `0-3` (stream=true only) |
| `user` | string | No | End-user identifier |

**Rejected parameters**: `response_format`, `style`, `input_fidelity`, `background=transparent`, `quality=standard`, `output_format=png` with `output_compression`.

> Do not send `response_format`. Use `output_format` to choose file format. Read `data[0].url` from the response.

### Request Examples

```bash
curl -X POST https://cn.crazyrouter.com/v1/images/generations \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-image-2",
    "prompt": "A cat wearing a spacesuit walking on the moon",
    "n": 1,
    "size": "1024x1024",
    "quality": "low",
    "output_format": "png"
  }'
```

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="https://cn.crazyrouter.com/v1",
)

response = client.images.generate(
    model="gpt-image-2",
    prompt="A cat wearing a spacesuit walking on the moon",
    n=1,
    size="1024x1024",
    quality="low",
    output_format="png",
)

print(response.data[0].url)
```

### Streaming

```bash
curl -N -X POST https://cn.crazyrouter.com/v1/images/generations \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-image-2",
    "prompt": "A simple blue verification icon on white background",
    "size": "1024x1024",
    "quality": "high",
    "stream": true,
    "partial_images": 2
  }'
```

> `quality=high` (or `hd`) synchronous requests can take a long time. Use `stream=true` or set client timeout above 180s.

### Response

```json
{
  "created": 1778990000,
  "data": [{
    "url": "https://media.crazyrouter.com/task-artifacts/.../request-id-1.png"
  }],
  "output_format": "png",
  "quality": "low",
  "size": "1024x1024"
}
```

## Edit Image

Edit with mask-based region editing. Supports up to 16 reference images for blending/composition.

### Parameters (beyond generation params)

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `image` / `image[]` | file / file[] | Yes | Original or reference images (multipart); up to 16 |
| `prompt` | string | Yes | Edit description |
| `mask` | file | No | Mask image; transparent areas = regions to edit |

### Single-Image Edit

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="https://cn.crazyrouter.com/v1",
)

response = client.images.edit(
    model="gpt-image-2",
    image=open("original.png", "rb"),
    mask=open("mask.png", "rb"),
    prompt="Add a rainbow in the sky",
    n=1,
    size="1024x1024",
    quality="low",
)
print(response.data[0].url)
```

### Multi-Reference Edit

```bash
curl -X POST https://cn.crazyrouter.com/v1/images/edits \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -F "model=gpt-image-2" \
  -F "prompt=Blend person from first image with background from second" \
  -F "size=1024x1024" \
  -F "quality=low" \
  -F "n=1" \
  -F "image[]=@person.png" \
  -F "image[]=@background.png"
```
