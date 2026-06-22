> ## Documentation Index
> Fetch the complete documentation index at: https://platform.minimaxi.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# 对话模型

> M2-her 对话模型，专为角色扮演、多轮对话等场景设计。

## 模型概览

**M2-her** 是 MiniMax 专为对话场景优化的语言模型，支持丰富的角色设定和对话历史管理能力。

### 支持模型

| 模型名称   | 上下文窗口 | 模型介绍                     |
| :----- | :---: | :----------------------- |
| M2-her |  64 K | **专为对话场景设计，支持角色扮演和多轮对话** |

### **M2-her** 核心特性

<AccordionGroup>
  <Accordion title="丰富的角色设定能力">
    M2-her 支持多种角色类型配置，包括模型角色（system）、用户角色（user\_system）、对话分组（group）等，让您可以灵活构建复杂的对话场景。
  </Accordion>

  <Accordion title="示例对话学习">
    通过 sample\_message\_user 和 sample\_message\_ai，您可以为模型提供示例对话，帮助模型更好地理解期望的对话风格和回复模式。
  </Accordion>

  <Accordion title="上下文记忆">
    模型支持完整的对话历史管理，能够基于前文内容进行连贯的多轮对话，提供更自然的交互体验。
  </Accordion>
</AccordionGroup>

***

## 调用示例

<Steps>
  <Step title="安装 SDK">
    <CodeGroup>
      ```bash Python theme={null}
      pip install openai
      ```

      ```bash Node.js theme={null}
      npm install openai
      ```
    </CodeGroup>
  </Step>

  <Step title="设置环境变量">
    ```bash theme={null}
    export OPENAI_BASE_URL=https://api.minimaxi.com/v1
    export OPENAI_API_KEY=${YOUR_API_KEY}
    ```
  </Step>

  <Step title="调用 M2-her">
    <CodeGroup>
      ```python Python theme={null}
      from openai import OpenAI

      client = OpenAI()

      response = client.chat.completions.create(
          model="M2-her",
          messages=[
              {
                  "role": "system",
                  "name": "AI助手",
                  "content": "你是一个友好、专业的AI助手"
              },
              {
                  "role": "user",
                  "name": "用户",
                  "content": "你好，请介绍一下你自己"
              }
          ],
          temperature=1.0,
          top_p=0.95,
          max_completion_tokens=2048
      )

      print(response.choices[0].message.content)
      ```

      ```javascript Node.js theme={null}
      import OpenAI from "openai";

      const client = new OpenAI();

      const response = await client.chat.completions.create({
        model: "M2-her",
        messages: [
          {
            role: "system",
            name: "AI助手",
            content: "你是一个友好、专业的AI助手"
          },
          {
            role: "user",
            name: "用户",
            content: "你好，请介绍一下你自己"
          }
        ],
        temperature: 1.0,
        top_p: 0.95,
        max_tokens: 2048
      });

      console.log(response.choices[0].message.content);
      ```

      ```bash cURL theme={null}
      curl https://api.minimaxi.com/v1/text/chatcompletion_v2 \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${YOUR_API_KEY}" \
        -d '{
          "model": "M2-her",
          "messages": [
            {
              "role": "system",
              "name": "AI助手",
              "content": "你是一个友好、专业的AI助手"
            },
            {
              "role": "user",
              "name": "用户",
              "content": "你好，请介绍一下你自己"
            }
          ],
          "temperature": 1.0,
          "top_p": 0.95,
          "max_completion_tokens": 2048
        }'
      ```
    </CodeGroup>
  </Step>
</Steps>

***

## 角色类型说明

M2-her 支持以下几种消息角色类型：

### 基础角色

| 角色类型        | 说明         | 使用场景               |
| :---------- | :--------- | :----------------- |
| `system`    | 设定模型的角色和行为 | 定义 AI 的身份、性格、知识范围等 |
| `user`      | 用户的输入      | 用户发送的消息            |
| `assistant` | 模型的历史回复    | AI 之前的回复，用于多轮对话    |

### 高级角色

| 角色类型                  | 说明         | 使用场景          |
| :-------------------- | :--------- | :------------ |
| `user_system`         | 设定用户的角色和人设 | 角色扮演场景中定义用户身份 |
| `group`               | 对话的名称      | 标识对话分组或场景名称   |
| `sample_message_user` | 示例的用户输入    | 提供用户消息的示例     |
| `sample_message_ai`   | 示例的模型输出    | 提供期望的 AI 回复示例 |

***

## 使用场景示例

### 场景 1：基础对话

```python theme={null}
messages = [
    {
        "role": "system",
        "content": "你是一个专业的编程助手"
    },
    {
        "role": "user",
        "content": "如何学习 Python？"
    }
]
```

### 场景 2：角色扮演对话

```python theme={null}
messages = [
    {
        "role": "system",
        "content": "你是《三国演义》中的诸葛亮，智慧、沉稳、善于谋略"
    },
    {
        "role": "user_system",
        "content": "你是一位来自现代的穿越者"
    },
    {
        "role": "group",
        "content": "三国时期的隆中对话"
    },
    {
        "role": "user",
        "content": "军师，我有一些现代的想法想和您探讨"
    }
]
```

### 场景 3：示例学习对话

```python theme={null}
messages = [
    {
        "role": "system",
        "content": "你是一个幽默风趣的聊天伙伴"
    },
    {
        "role": "sample_message_user",
        "content": "今天天气真好"
    },
    {
        "role": "sample_message_ai",
        "content": "是啊！阳光明媚的日子总让人心情愉悦，就像你的笑容一样灿烂~"
    },
    {
        "role": "user",
        "content": "明天准备去爬山"
    }
]
```

***

## 参数说明

### 核心参数

| 参数                      | 类型      | 默认值   | 说明                                           |
| :---------------------- | :------ | :---- | :------------------------------------------- |
| `model`                 | string  | -     | 模型名称，固定为 `M2-her`                            |
| `messages`              | array   | -     | 对话消息列表，详见 [API 参考](/api-reference/text-chat) |
| `temperature`           | number  | 1.0   | 温度系数，控制输出随机性                                 |
| `top_p`                 | number  | 0.95  | 采样策略参数                                       |
| `max_completion_tokens` | integer | -     | 生成内容的最大长度，上限 2048                            |
| `stream`                | boolean | false | 是否使用流式输出                                     |

***

## 最佳实践

<AccordionGroup>
  <Accordion title="合理设置角色">
    使用 `system` 定义 AI 的基本行为，使用 `user_system` 定义用户身份，可以让对话更加自然和符合场景设定。
  </Accordion>

  <Accordion title="提供示例对话">
    通过 `sample_message_user` 和 `sample_message_ai` 提供 1-3 个示例对话，可以有效引导模型的回复风格。
  </Accordion>

  <Accordion title="维护对话历史">
    保留完整的对话历史（包括 `user` 和 `assistant` 消息），让模型能够基于上下文进行连贯回复。
  </Accordion>

  <Accordion title="控制对话长度">
    根据场景需求设置合适的 `max_completion_tokens`，避免回复过长或被截断。
  </Accordion>
</AccordionGroup>

***

## 常见问题

<AccordionGroup>
  <Accordion title="如何实现多轮对话？">
    在每次请求中包含完整的对话历史，按时间顺序排列 `user` 和 `assistant` 消息。
  </Accordion>

  <Accordion title="user_system 和 system 有什么区别？">
    `system` 定义 AI 的角色，`user_system` 定义用户的角色。在角色扮演场景中，两者配合使用可以创建更丰富的对话体验。
  </Accordion>

  <Accordion title="示例消息会占用 token 吗？">
    是的，所有消息（包括示例消息）都会计入输入 token。建议提供 1-3 个精炼的示例即可。
  </Accordion>

  <Accordion title="是否支持图片输入？">
    M2-her 当前仅支持文本输入，不支持图文混合输入。
  </Accordion>
</AccordionGroup>

***

## 相关链接

<CardGroup cols={2}>
  <Card title="API 参考" icon="book-open" href="/api-reference/text-chat">
    查看完整的 API 接口文档
  </Card>

  <Card title="定价说明" icon="book-open" href="/guides/pricing-paygo#文本">
    了解 M2-her 的定价详情
  </Card>

  <Card title="错误码" icon="book-open" href="/api-reference/errorcode">
    查看 API 错误码说明
  </Card>

  <Card title="快速开始" icon="rocket" href="/guides/quickstart">
    快速上手 MiniMax API
  </Card>
</CardGroup>
