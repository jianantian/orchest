# OpenRouter — Image Generation

> Source: https://openrouter.ai/docs/guides/overview/multimodal/image-generation
> Retrieved: 2026-05-25
>
> OpenRouter supports image generation via the Chat Completions and Responses endpoints. Find models by filtering the [model list](https://openrouter.ai/models?output_modalities=image) by image output.

## Model Discovery

### Via API

```bash
# Image-only models
curl "https://openrouter.ai/api/v1/models?output_modalities=image"

# Text + image models
curl "https://openrouter.ai/api/v1/models?output_modalities=text,image"
```

### Via Models Page

Visit https://openrouter.ai/models and filter by output modalities. Look for `"image"` in output modalities.

## API Usage

Use the `/api/v1/chat/completions` endpoint with the `modalities` parameter:

- Text + image models (e.g. Gemini): `modalities: ["image", "text"]`
- Image-only models (e.g. Flux, Sourceful): `modalities: ["image"]`

### Basic Image Generation

```bash
curl https://openrouter.ai/api/v1/chat/completions \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "google/gemini-2.5-flash-image",
    "messages": [
      {"role": "user", "content": "Generate a beautiful sunset over mountains"}
    ],
    "modalities": ["image", "text"]
  }'
```

```python
import requests

response = requests.post(
    "https://openrouter.ai/api/v1/chat/completions",
    headers={
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    },
    json={
        "model": "google/gemini-2.5-flash-image",
        "messages": [
            {"role": "user", "content": "Generate a beautiful sunset over mountains"}
        ],
        "modalities": ["image", "text"]
    }
)

result = response.json()
if result.get("choices"):
    message = result["choices"][0]["message"]
    if message.get("images"):
        for image in message["images"]:
            image_url = image["image_url"]["url"]  # Base64 data URL
            print(f"Generated image: {image_url[:50]}...")
```

```typescript
import OpenRouter from 'openrouter';

const client = new OpenRouter({ apiKey: process.env.OPENROUTER_API_KEY });

const result = await client.chat.send({
  model: 'google/gemini-2.5-flash-image',
  messages: [
    { role: 'user', content: 'Generate a beautiful sunset over mountains' }
  ],
  modalities: ['image', 'text'],
});

if (result.choices) {
  const message = result.choices[0].message;
  if (message.images) {
    message.images.forEach((image, index) => {
      const imageUrl = image.image_url.url; // Base64 data URL
      console.log(`Image ${index + 1}: ${imageUrl.substring(0, 50)}...`);
    });
  }
}
```

## Image Configuration (`image_config`)

### Aspect Ratio

| Ratio | Resolution |
|-------|------------|
| `1:1` | 1024×1024 (default) |
| `2:3` | 832×1248 |
| `3:2` | 1248×832 |
| `3:4` | 864×1184 |
| `4:3` | 1184×864 |
| `4:5` | 896×1152 |
| `5:4` | 1152×896 |
| `9:16` | 768×1344 |
| `16:9` | 1344×768 |
| `21:9` | 1536×672 |

**Extended** (Gemini 3.1 Flash Image only): `1:4`, `4:1`, `1:8`, `8:1`

### Image Size

| Size | Description |
|------|-------------|
| `1K` | Standard (default) |
| `2K` | Higher resolution |
| `4K` | Highest resolution |
| `0.5K` | Lower, optimized (Gemini 3.1 Flash Image only) |

```json
{
  "image_config": {
    "aspect_ratio": "16:9",
    "image_size": "4K"
  }
}
```

### Strength (Recraft only)

Controls image-to-image deviation. Range `0.0`–`1.0`, default `0.2`. Lower = closer to input.

```json
{ "image_config": { "strength": 0.7 } }
```

### Text Layout (Recraft V3 only)

Position text at specific coordinates. Each entry: `text` + `bbox` (4 corner points, 0–1 normalized).

```json
{
  "image_config": {
    "text_layout": [
      {
        "text": "Hello",
        "bbox": [[0.3, 0.45], [0.6, 0.45], [0.6, 0.55], [0.3, 0.55]]
      },
      {
        "text": "World",
        "bbox": [[0.35, 0.6], [0.65, 0.6], [0.65, 0.7], [0.35, 0.7]]
      }
    ]
  }
}
```

### Style (Recraft V3 only)

```json
{ "image_config": { "style": "Photorealism" } }
```

See [Recraft styles](https://www.recraft.ai/docs/api-reference/styles#list-of-styles). Vector styles not supported.

### RGB Colors (Recraft only)

```json
{
  "image_config": {
    "rgb_colors": [[255, 0, 0], [0, 128, 0]],
    "background_rgb_color": [255, 255, 255]
  }
}
```

### Font Inputs (Sourceful only)

Max 2 fonts, $0.03 per font input.

```json
{
  "image_config": {
    "font_inputs": [
      {
        "font_url": "https://example.com/fonts/custom.ttf",
        "text": "Hello World"
      }
    ]
  }
}
```

Tips: include text in prompt, match `text` parameter exactly to prompt, use line breaks for headlines.

### Super Resolution References (Sourceful only)

Enhance low-quality elements via reference images. Max 4 refs, $0.20/ref. Image-to-image only.

```json
{
  "image_config": {
    "super_resolution_references": [
      "https://example.com/ref1.jpg",
      "https://example.com/ref2.jpg"
    ]
  }
}
```

## Streaming

```python
response = requests.post(
    "https://openrouter.ai/api/v1/chat/completions",
    headers={"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"},
    json={
        "model": "google/gemini-2.5-flash-image",
        "messages": [{"role": "user", "content": "Create a futuristic city"}],
        "modalities": ["image", "text"],
        "stream": True
    },
    stream=True
)

for line in response.iter_lines():
    if line:
        line = line.decode('utf-8')
        if line.startswith('data: ') and line[6:] != '[DONE]':
            chunk = json.loads(line[6:])
            delta = chunk.get("choices", [{}])[0].get("delta", {})
            if delta.get("images"):
                for img in delta["images"]:
                    print(f"Image: {img['image_url']['url'][:50]}...")
```

## Response Format

```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": "I've generated a beautiful sunset image for you.",
      "images": [{
        "type": "image_url",
        "image_url": {
          "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA..."
        }
      }]
    }
  }]
}
```

- Images returned as base64 data URLs (typically PNG)
- Multiple images supported by some models
- Images in `message.images` array (non-streaming) or `delta.images` (streaming)

## Compatible Models

| Model | Notes |
|-------|-------|
| `google/gemini-3.1-flash-image-preview` | Extended ratios, 0.5K resolution |
| `google/gemini-2.5-flash-image` | Text + image |
| `black-forest-labs/flux.2-pro` | Image only |
| `black-forest-labs/flux.2-flex` | Image only |
| `sourceful/riverflow-v2-standard-preview` | Font inputs, super resolution |
| `recraft/recraft-v3` | Text layout, styles, colors |

Filter full list: https://openrouter.ai/models?output_modalities=image

## Best Practices

- Provide detailed prompts
- Check `output_modalities` includes `"image"` before calling
- Always check `images` field exists before processing
- Image generation may have different rate limits
- Plan for base64 image data storage/processing
