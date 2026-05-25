# 阿里云百炼 — 图像生成 API 参考

> Source: https://help.aliyun.com/zh/model-studio/ (Qwen-Image / Z-Image / 万相)
> Retrieved: 2026-05-25

## 模型系列总览

| 系列 | 代表模型 | 特点 |
|------|----------|------|
| **千问-Image** | qwen-image-2.0-pro, qwen-image-max, qwen-image-plus | 文字渲染强，支持生图+编辑融合 |
| **Z-Image** | z-image-turbo | 轻量快速，支持 prompt_extend 思考模式 |
| **万相 (Wan)** | wan2.7-image-pro, wan2.6-t2i, wan2.5-t2i-preview | 写实风格，组图生成，交互式编辑 |

---

## 通用信息

### 地域端点

| 地域 | 多模态端点（同步） | 异步创建端点 | 任务查询端点 |
|------|-------------------|-------------|-------------|
| 北京 | `https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation` | `.../image-generation/generation` 或 `.../text2image/image-synthesis`（旧版） | `GET .../api/v1/tasks/{task_id}` |
| 新加坡 | `dashscope-intl.aliyuncs.com` | 同上 | 同上 |
| 弗吉尼亚 | `dashscope-us.aliyuncs.com` | 同上 | 同上 |

> 北京和新加坡的 API Key **不可混用**。图像 URL 有效期 **24 小时**。

### 认证

```
Authorization: Bearer sk-xxxx
```

---

## 千问-Image (Qwen-Image)

擅长**复杂文本渲染**、多行布局、段落级文字生成。支持文生图、图像编辑、多图融合。

### 模型列表

| 模型 | 说明 | 输出 |
|------|------|------|
| `qwen-image-2.0-pro` | Pro 系列，文字渲染+真实质感最强 | 512²–2048² 像素，1-6张，PNG |
| `qwen-image-2.0` | 加速版，兼顾效果与速度 | 同上 |
| `qwen-image-max` | Max 系列，真实感强、AI 痕迹低 | 固定分辨率（16:9/4:3/1:1/3:4/9:16），1张 |
| `qwen-image-plus` | Plus 系列，多样化艺术风格 | 同上 |
| `qwen-image` | 基础版 | 同上 |

### 同步接口（推荐）

**端点**：`POST .../api/v1/services/aigc/multimodal-generation/generation`

**请求体**：
```json
{
  "model": "qwen-image-2.0-pro",
  "input": {
    "messages": [{
      "role": "user",
      "content": [
        {"text": "正向提示词（必填，支持中英文）"},
        {"image": "https://...（可选，1-3张）"}
      ]
    }]
  },
  "parameters": {
    "negative_prompt": "反向提示词",
    "size": "2048*2048",
    "n": 1,
    "prompt_extend": true,
    "watermark": false,
    "seed": 12345
  }
}
```

**关键参数**：

| 参数 | 类型 | 说明 |
|------|------|------|
| `model` | string | 模型名称 |
| `input.messages[].content[].text` | string | 正向提示词（qwen-image-2.0 上限 1300 Token，其他 800） |
| `input.messages[].content[].image` | string | 输入图像 URL/Base64（1-3张） |
| `size` | string | `宽*高`，如 `2048*2048`、`2688*1536`（16:9） |
| `n` | integer | 输出数量（qwen-image-2.0: 1-6；其他: 固定1） |
| `prompt_extend` | boolean | 是否开启 Prompt 智能改写（默认 true） |
| `negative_prompt` | string | 反向提示词（最长 500 字符） |
| `watermark` | boolean | 是否加水印 "Qwen-Image"（默认 false） |
| `seed` | integer | 随机种子，范围 [0, 2147483647] |

**响应**：
```json
{
  "output": {
    "choices": [{
      "finish_reason": "stop",
      "message": {
        "role": "assistant",
        "content": [
          {"image": "https://dashscope-result-xxx.oss-xxx.aliyuncs.com/xxx.png?Expires=xxx"}
        ]
      }
    }]
  },
  "usage": { "height": 2048, "image_count": 1, "width": 2048 },
  "request_id": "..."
}
```

### 图像编辑

支持单图编辑和多图融合（1-3 张输入）。请求格式同上，在 `content` 中传入 `image` + `text`。

**size 推荐分辨率**：
- 1:1 → 1024\*1024, 1536\*1536
- 2:3 → 768\*1152, 1024\*1536
- 3:2 → 1152\*768, 1536\*1024
- 16:9 → 1280\*720, 1920\*1080
- 9:16 → 720\*1280, 1080\*1920

### 异步接口（仅 qwen-image-plus / qwen-image 支持旧版）

1. **创建任务**：`POST .../text2image/image-synthesis` + Header `X-DashScope-Async: enable`
2. **查询结果**：`GET .../api/v1/tasks/{task_id}`

响应包含 `orig_prompt`（原始）和 `actual_prompt`（智能改写后的实际提示词）。

---

## Z-Image

轻量级文生图模型，支持 `prompt_extend=true` 时返回优化提示词和思考过程。

### 模型

| 模型 | 说明 | 输出 |
|------|------|------|
| `z-image-turbo` | 轻量快速 | 512²–2048² 像素，1张，PNG |

### 同步接口

**端点**：`POST .../api/v1/services/aigc/multimodal-generation/generation`

**关键参数**：

| 参数 | 说明 |
|------|------|
| `size` | `宽*高`，默认 `1024*1536`。推荐总像素在 1024²–1536² 之间 |
| `prompt_extend` | `false`（默认）：关闭；`true`：开启智能思考，返回优化提示词+推理过程，价格更高 |
| `seed` | 随机种子 |

**prompt_extend=true 时的响应**：额外返回 `reasoning_content`（思考过程）和优化后的 `text`。

**推荐分辨率**（总像素 1024²）：
1:1: 1024\*1024 / 2:3: 832\*1248 / 3:2: 1248\*832 / 16:9: 1280\*720 / 9:16: 720\*1280

---

## 万相 (Wan) 文生图

写实风格图像生成，支持自由尺寸、组图生成、交互式编辑。

### 模型版本对比

| 模型 | 同步接口 | 异步接口 | 分辨率 | 组图 | 4K |
|------|---------|---------|--------|------|-----|
| `wan2.7-image-pro` | ✅ | ✅ | 768²–4096² | ✅ | ✅（仅文生图） |
| `wan2.7-image` | ✅ | ✅ | 768²–2048² | ✅ | ❌ |
| `wan2.6-t2i` | ✅ | ✅ | 1280²–1440² | ❌ | ❌ |
| `wan2.5-t2i-preview` | ❌ | ✅（旧版） | 1280²–1440² | ❌ | ❌ |
| `wan2.2 / 2.1 / 2.0` | ❌ | ✅（旧版） | 512–1440 单边 | ❌ | ❌ |

### wan2.7 同步接口

**端点**：`POST .../api/v1/services/aigc/multimodal-generation/generation`

**关键参数**：

| 参数 | 说明 |
|------|------|
| `model` | `wan2.7-image-pro` 或 `wan2.7-image` |
| `size` | 简写 `"1K"` / `"2K"` / `"4K"`（推荐），或像素值如 `"1280*1280"` |
| `n` | 普通：1-4；组图模式：1-12 |
| `enable_sequential` | `true` 开启组图模式 |
| `thinking_mode` | `true`（默认）：增强推理，仅文生图无图片输入时生效 |
| `color_palette` | 自定义颜色主题（3-10 色，hex+ratio 总和 100.00%） |
| `bbox_list` | 交互式编辑框选（每张图最多 2 个框，格式 `[[x1,y1,x2,y2],...]`） |
| `watermark` | 是否加水印 "AI生成" |

**图像编辑**：传入 0-9 张图片，支持 `image` URL/Base64。

**组图生成示例**（`enable_sequential: true`）：
```json
{
  "model": "wan2.7-image-pro",
  "input": {
    "messages": [{
      "role": "user",
      "content": [{"text": "电影感组图，记录同一只流浪橘猫。第一张：春天樱花树下...第四张：冬天雪地足迹。"}]
    }]
  },
  "parameters": {
    "enable_sequential": true,
    "n": 4,
    "size": "2K"
  }
}
```

**交互式编辑**（`bbox_list`）：在原图上指定矩形区域进行局部编辑。
```json
{
  "parameters": {
    "bbox_list": [
      [[989, 515, 1138, 681]],   // 图1: 1个框
      []                           // 图2: 无框选
    ]
  }
}
```

### wan2.6 同步/异步

**同步端点**：`POST .../api/v1/services/aigc/multimodal-generation/generation`
**异步端点**：`POST .../api/v1/services/aigc/image-generation/generation` + Header `X-DashScope-Async: enable`

参数类似 wan2.7，但无组图/编辑/4K。`size` 总像素 1280²–1440²，宽高比 [1:4, 4:1]。

### 旧版异步接口（wan2.5 及以下）

**创建**：`POST .../text2image/image-synthesis` + `X-DashScope-Async: enable`
**查询**：`GET .../api/v1/tasks/{task_id}`

请求体使用旧格式：
```json
{
  "model": "wan2.5-t2i-preview",
  "input": { "prompt": "...", "negative_prompt": "..." },
  "parameters": { "size": "1280*1280", "n": 1 }
}
```

响应包含 `task_metrics`（TOTAL/SUCCEEDED/FAILED）和 `actual_prompt`。

---

## Python SDK 调用模式

所有图像模型通过 DashScope SDK 支持两种调用模式：

```python
# 同步（阻塞等待）
rsp = ImageGeneration.call(model="wan2.7-image-pro", ...)
# 或
rsp = MultiModalConversation.call(model="qwen-image-2.0-pro", ...)

# 异步（创建 → 轮询）
rsp = ImageGeneration.async_call(model="wan2.7-image-pro", ...)
status = ImageGeneration.wait(task=rsp, api_key=api_key)
# 或
status = ImageGeneration.fetch(task=rsp, api_key=api_key)
rsp = ImageGeneration.cancel(task=rsp, api_key=api_key)
```

SDK 版本要求：
- wan2.7: Python ≥ 1.25.15, Java ≥ 2.22.13
- wan2.6: Python ≥ 1.25.7, Java ≥ 2.22.6
- wan2.5 及以下: Python ≥ 1.25.2, Java ≥ 2.22.2

---

## 图像输入方式

所有模型支持三种图片传入方式：

1. **公网 URL**：`"https://example.com/image.png"`（HTTP/HTTPS）
2. **Base64 编码**：`"data:image/png;base64,iVBORw0KGgo..."`
3. **OSS 临时 URL**：通过上传接口获取

Python Base64 编码示例：
```python
import base64, mimetypes

def encode_file(file_path):
    mime_type, _ = mimetypes.guess_type(file_path)
    with open(file_path, "rb") as f:
        encoded = base64.b64encode(f.read()).decode("utf-8")
    return f"data:{mime_type};base64,{encoded}"
```

---

## 计费

- 按**成功生成的图像张数**计费
- 调用失败或内容审核不通过不产生费用
- `prompt_extend=true` 时 Z-Image 价格高于 `false`
- 各种 `n`（图片数量）参数直接影响费用
