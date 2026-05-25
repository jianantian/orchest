# Renderful — JavaScript SDK

> Source: https://renderful.ai/zh/docs/javascript-sdk
> Retrieved: 2026-05-25
>
> Single-file module for Node.js. No npm install needed — uses built-in `fetch` API. Requires Node.js 18+.

## The Module

```javascript
// renderful.mjs — copy this file into your project

export class Renderful {
  static BASE = "https://api.renderful.ai/api/v1";

  constructor(apiKey) {
    this.apiKey = apiKey || process.env.RENDERFUL_API_KEY;
    if (!this.apiKey) throw new Error("RENDERFUL_API_KEY is required");
    this.headers = {
      Authorization: `Bearer ${this.apiKey}`,
      "Content-Type": "application/json",
    };
  }

  // --- core methods ---

  async create(body) {
    const r = await fetch(`${Renderful.BASE}/generations`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify(body),
    });
    if (!r.ok) throw new Error(`API ${r.status}: ${await r.text()}`);
    return r.json();
  }

  async get(taskId) {
    const r = await fetch(`${Renderful.BASE}/generations/${taskId}`, {
      headers: this.headers,
    });
    if (!r.ok) throw new Error(`API ${r.status}: ${await r.text()}`);
    return r.json();
  }

  async poll(taskId, { interval = 3000, timeout = 300000 } = {}) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const data = await this.get(taskId);
      if (data.status === "completed" || data.status === "failed") return data;
      await new Promise((r) => setTimeout(r, interval));
    }
    throw new Error(`Task ${taskId} did not finish within ${timeout}ms`);
  }

  async run(body, pollOpts) {
    const task = await this.create(body);
    return this.poll(task.id, pollOpts);
  }

  // --- file upload ---

  async upload(filePath) {
    const { readFile } = await import("node:fs/promises");
    const { basename } = await import("node:path");
    const data = await readFile(filePath);
    const form = new FormData();
    form.append("file", new Blob([data]), basename(filePath));
    const r = await fetch(`${Renderful.BASE}/uploads`, {
      method: "POST",
      headers: { Authorization: `Bearer ${this.apiKey}` },
      body: form,
    });
    if (!r.ok) throw new Error(`Upload ${r.status}: ${await r.text()}`);
    return (await r.json()).url;
  }
}
```

## Authentication

```javascript
// Env variable (recommended)
// export RENDERFUL_API_KEY="rf_..."
const client = new Renderful(); // reads from process.env

// Direct
const client = new Renderful("rf_...");
```

## Usage Examples

### Text to Image

```javascript
import { Renderful } from "./renderful.mjs";

const client = new Renderful();
const result = await client.run({
  type: "text-to-image",
  model: "flux-dev",
  prompt: "A cyberpunk cityscape at night",
});
console.log(result.outputs); // ["https://..."]
```

### Text to Video

```javascript
const result = await client.run({
  type: "text-to-video",
  model: "wan-2.6",
  prompt: "A drone flyover of a tropical island",
});
console.log(result.outputs); // ["https://...mp4"]
```

### Image to Video

```javascript
// Upload a local image first
const imageUrl = await client.upload("photo.jpg");

const result = await client.run({
  type: "image-to-video",
  model: "wan-2.6-i2v",
  image_url: imageUrl,
  prompt: "Slow zoom into the scene",
});
console.log(result.outputs);
```

### Text to Text (LLM)

```javascript
const result = await client.run({
  type: "text-to-text",
  model: "kimi-k2.5",
  prompt: "Explain quantum computing in 3 sentences",
  max_tokens: 256,
});
console.log(result.outputs[0]);
```

## Error Handling

```javascript
try {
  const result = await client.run({
    type: "text-to-image",
    model: "flux-dev",
    prompt: "A sunset",
  });

  if (result.status === "failed") {
    console.error("Task failed:", result.error);
  } else {
    console.log("Output:", result.outputs);
  }
} catch (err) {
  console.error("API error:", err.message);
}
```

## Fire-and-Forget Mode

Use `create()` instead of `run()` if you prefer webhooks over polling:

```javascript
const task = await client.create({
  type: "text-to-image",
  model: "flux-dev",
  prompt: "A sunset",
  webhook_url: "https://your-app.com/webhooks/renderful",
});
console.log(`Task submitted: ${task.id}`);
// Your webhook endpoint will receive the result
```
