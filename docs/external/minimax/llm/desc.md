> ## Documentation Index
> Fetch the complete documentation index at: https://platform.minimaxi.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# 模型调用

> MiniMax 语言模型，支持多语言编程、Agent 工作流等复杂任务场景。

<Note>
  订阅 [Token Plan](https://platform.minimaxi.com/subscribe/token-plan) ，即可以超低价格使用 MiniMax 全模态模型!
</Note>

## 模型概览

MiniMax 提供多款语言模型，满足不同场景需求。**MiniMax-M3** 是最新 M 系列语言模型，适用于 Agent 推理、工具调用、代码和长上下文任务；**MiniMax-M2.7** 与 **MiniMax-M2** 继续支持已有工作流。

### 支持模型

| 模型名称                        |   上下文窗口   | 模型介绍                                    |
| :-------------------------- | :-------: | :-------------------------------------- |
| MiniMax-M3                  | 1,000,000 | **原生多模态、1M 上下文的 Frontier Coding 模型**    |
| MiniMax-M2.7                |  204,800  | **开启模型的自我迭代**（输出速度约 60 TPS）             |
| MiniMax-M2.7-highspeed      |  204,800  | **M2.7 极速版：效果不变，更快，更敏捷**（输出速度约 100 TPS） |
| MiniMax-M2.5                |  204,800  | **顶尖性能与极致性价比，轻松驾驭复杂任务**（输出速度约 60 TPS）   |
| MiniMax-M2.5-highspeed      |  204,800  | **M2.5 极速版：效果不变，更快，更敏捷**（输出速度约 100 TPS） |
| MiniMax-M2.1                |  204,800  | **强大多语言编程能力，全面升级编程体验**（输出速度约 60 TPS）    |
| MiniMax-M2.1-highspeed      |  204,800  | **M2.1 极速版：效果不变，更快，更敏捷**（输出速度约 100 TPS） |
| MiniMax-M2                  |  204,800  | **专为高效编码与 Agent 工作流而生**                 |
| [M2-her](/guides/text-chat) |    64 K   | **专为对话场景设计，支持角色扮演和多轮对话**                |

<Note>
  TPS（Tokens Per Second）的计算方式详见[常见问题 > 接口相关](/faq/about-apis#%E9%97%AE%E6%96%87%E6%9C%AC%E6%A8%A1%E5%9E%8B%E7%9A%84-tpstokens-per-second%E6%98%AF%E5%A6%82%E4%BD%95%E8%AE%A1%E7%AE%97%E7%9A%84)。
</Note>

### **MiniMax M3** 核心亮点

<AccordionGroup>
  <Accordion title="1M 上下文">
    MiniMax-M3 支持最高 1,000,000 token 上下文，适用于长文档、代码库和多步骤 Agent 会话。
  </Accordion>

  <Accordion title="Agent 与代码场景">
    MiniMax-M3 面向 Agent 推理、工具调用、代码和结构化任务执行优化。
  </Accordion>

  <Accordion title="多模态 Chat 输入">
    OpenAI 兼容 Chat Completions 支持文本、图片和视频输入，可使用 `image_url` 与 `video_url` 内容块。
  </Accordion>
</AccordionGroup>

<Note>
  更多模型介绍请参考 [MiniMax M3](https://www.minimaxi.com/models/text/m3)。
</Note>

***

## URL 配置

调用 MiniMax 模型前，请先准备好以下信息：

| 字段                          | 值                                                                        |
| :-------------------------- | :----------------------------------------------------------------------- |
| `base_url`（Anthropic 兼容，推荐） | `https://api.minimaxi.com/anthropic`                                     |
| `base_url`（OpenAI 兼容）       | `https://api.minimaxi.com/v1`                                            |
| `api_key`                   | [获取订阅 Key](https://platform.minimaxi.com/user-center/payment/token-plan) |
| `model`                     | 见上方[支持模型](#支持模型)表                                                        |

***

## 调用示例

MiniMax 同时兼容 Anthropic 和 OpenAI 两种 API 协议格式，下面给出两套等价的非流式样例。需要流式响应时，把请求里的 `stream` 改成 `true` 即可。

### Anthropic 兼容（推荐）

支持 thinking 块、interleaved thinking 等高级特性，是默认推荐路径。

<CodeGroup>
  ```bash curl theme={null}
  curl https://api.minimaxi.com/anthropic/v1/messages \
    -H "Authorization: Bearer <MINIMAX_API_KEY>" \
    -H "Content-Type: application/json" \
    -d '{
      "model": "MiniMax-M3",
      "max_tokens": 1000,
      "messages": [
        {"role": "user", "content": "Hi, how are you?"}
      ]
    }'
  ```

  ```python Python theme={null}
  # 首次使用前请先安装 Anthropic SDK：`pip install anthropic`
  import anthropic

  client = anthropic.Anthropic(
      base_url="https://api.minimaxi.com/anthropic",
      api_key="<MINIMAX_API_KEY>",
  )

  message = client.messages.create(
      model="MiniMax-M3",
      max_tokens=1000,
      messages=[
          {"role": "user", "content": "Hi, how are you?"}
      ],
  )

  for block in message.content:
      if block.type == "thinking":
          print(f"Thinking:\n{block.thinking}\n")
      elif block.type == "text":
          print(f"Text:\n{block.text}\n")
  ```

  ```javascript Node.js theme={null}
  // 首次使用前请先安装 Anthropic SDK：`npm install @anthropic-ai/sdk`
  import Anthropic from "@anthropic-ai/sdk";

  const client = new Anthropic({
    baseURL: "https://api.minimaxi.com/anthropic",
    apiKey: "<MINIMAX_API_KEY>",
  });

  const message = await client.messages.create({
    model: "MiniMax-M3",
    max_tokens: 1000,
    messages: [
      { role: "user", content: "Hi, how are you?" },
    ],
  });

  for (const block of message.content) {
    if (block.type === "thinking") {
      console.log(`Thinking:\n${block.thinking}\n`);
    } else if (block.type === "text") {
      console.log(`Text:\n${block.text}\n`);
    }
  }
  ```
</CodeGroup>

### OpenAI 兼容

如果你的项目已经接入 OpenAI SDK，把 `base_url` 和 `model` 换成下方的值即可直接复用，无需迁移到新 SDK。

<CodeGroup>
  ```bash curl theme={null}
  curl https://api.minimaxi.com/v1/chat/completions \
    -H "Authorization: Bearer <MINIMAX_API_KEY>" \
    -H "Content-Type: application/json" \
    -d '{
      "model": "MiniMax-M3",
      "messages": [
        {"role": "user", "content": "Hi, how are you?"}
      ]
    }'
  ```

  ```python Python theme={null}
  # 首次使用前请先安装 OpenAI SDK：`pip install openai`
  from openai import OpenAI

  client = OpenAI(
      base_url="https://api.minimaxi.com/v1",
      api_key="<MINIMAX_API_KEY>",
  )

  response = client.chat.completions.create(
      model="MiniMax-M3",
      messages=[
          {"role": "user", "content": "Hi, how are you?"},
      ],
  )

  print(response.choices[0].message.content)
  ```

  ```javascript Node.js theme={null}
  // 首次使用前请先安装 OpenAI SDK：`npm install openai`
  import OpenAI from "openai";

  const client = new OpenAI({
    baseURL: "https://api.minimaxi.com/v1",
    apiKey: "<MINIMAX_API_KEY>",
  });

  const response = await client.chat.completions.create({
    model: "MiniMax-M3",
    messages: [
      { role: "user", content: "Hi, how are you?" },
    ],
  });

  console.log(response.choices[0].message.content);
  ```
</CodeGroup>

***

## API 参考

<Columns cols={2}>
  <Card title="Anthropic API 兼容（推荐）" icon="book-open" href="/api-reference/text-anthropic-api" cta="查看文档">
    通过 Anthropic SDK 调用 MiniMax 模型，支持流式输出和 Interleaved Thinking
  </Card>

  <Card title="OpenAI API 兼容" icon="book-open" href="/api-reference/text-openai-api" cta="查看文档">
    通过 OpenAI SDK 调用 MiniMax 模型
  </Card>

  <Card title="在 AI 编程工具里使用 M3" icon="code" href="/token-plan/openclaw" cta="查看文档">
    在 Claude Code、Cursor 等工具中使用 M3
  </Card>

  <Card title="Chat Model" icon="messages-square" href="/guides/text-chat" cta="查看文档">
    M2-her 对话模型，专为角色扮演、多轮对话等场景设计
  </Card>
</Columns>

***

## 联系我们

如果在使用 MiniMax 模型过程中遇到任何问题：

* 通过邮箱 [Model@minimaxi.com](mailto:Model@minimaxi.com) 等官方渠道联系我们的技术支持团队
* 在我们的 [Github](https://github.com/MiniMax-AI/MiniMax-M2.7/issues) 仓库提交 Issue

## 相关链接

* [Anthropic SDK 文档](https://docs.anthropic.com/en/api/client-sdks)
* [OpenAI SDK 文档](https://platform.openai.com/docs/libraries)
* [MiniMax M3](https://www.minimaxi.com/models/text/m3)
