> ## Documentation Index
> Fetch the complete documentation index at: https://platform.minimaxi.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Messages API

> 使用 Anthropic API 兼容 Messages 格式调用 MiniMax 模型。

<Note>
  ✨ **全新模型 `MiniMax-M3`**

  **核心能力**：**Coding/Agentic SOTA**、**1M 超长上下文**、**多模态**。
</Note>

<Tip>
  **`MiniMax-M3` 新特性：**

  1. 支持图片、视频理解，可参考右方示例代码
  2. 支持通过 `thinking` 参数控制思考
</Tip>


## OpenAPI

````yaml /api-reference/text/api/openapi-chat-anthropic.json POST /anthropic/v1/messages
openapi: 3.1.0
info:
  title: MiniMax Text API Anthropic
  description: |
    MiniMax 文本生成 API，支持对话补全与流式输出。

    > ⚡ **`MiniMax-M3` 亮点** —— **Coding/Agentic SOTA**、**1M 超长上下文**、**多模态**。
  license:
    name: MIT
  version: 1.0.0
servers:
  - url: https://api.minimaxi.com
security:
  - bearerAuth: []
  - apiKeyAuth: []
paths:
  /anthropic/v1/messages:
    post:
      tags:
        - Text Generation
      summary: Text Generation Anthropic
      operationId: chatCompletionAnthropic
      parameters:
        - name: Content-Type
          in: header
          required: true
          description: 请求体的媒介类型，请设置为 `application/json`，确保请求数据的格式为 JSON
          schema:
            type: string
            enum:
              - application/json
            default: application/json
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateMessageReq'
            examples:
              图片理解:
                value:
                  model: MiniMax-M3
                  messages:
                    - role: user
                      content:
                        - type: text
                          text: 这张图片的内容是什么？
                        - type: image
                          source:
                            type: url
                            url: >-
                              https://filecdn.minimax.chat/public/fe9d04da-f60e-444d-a2e0-18ae743add33.jpeg
                  max_tokens: 500
                  thinking:
                    type: adaptive
              视频理解:
                value:
                  model: MiniMax-M3
                  thinking:
                    type: adaptive
                  messages:
                    - role: user
                      content:
                        - type: text
                          text: 这个视频里发生了什么？
                        - type: video
                          source:
                            type: url
                            url: >-
                              https://filecdn.minimax.chat/public/ee8c1648-21f1-41b7-8397-65022d22ffe5.mp4
                  max_tokens: 1024
              深度思考:
                value:
                  model: MiniMax-M3
                  thinking:
                    type: adaptive
                  messages:
                    - role: user
                      content: 9.11 和 9.9 哪个更大？
                  max_tokens: 1024
              流式:
                value:
                  model: MiniMax-M3
                  messages:
                    - role: user
                      content:
                        - type: text
                          text: 这张图片的内容是什么？
                        - type: image
                          source:
                            type: url
                            url: >-
                              https://filecdn.minimax.chat/public/fe9d04da-f60e-444d-a2e0-18ae743add33.jpeg
                  stream: true
                  max_tokens: 500
                  thinking:
                    type: adaptive
              工具调用:
                value:
                  model: MiniMax-M3
                  messages:
                    - role: user
                      content: 旧金山现在天气怎么样？
                  max_tokens: 1024
                  tools:
                    - name: get_weather
                      description: Get the current weather for a given location.
                      input_schema:
                        type: object
                        properties:
                          location:
                            type: string
                            description: The city and state/country, e.g. San Francisco, US
                        required:
                          - location
                  tool_choice:
                    type: auto
        required: true
      responses:
        '200':
          description: ''
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/CreateMessageResp'
              examples:
                图片理解:
                  value:
                    id: 066a381bdc3c0ded310e27c9a46d16e7
                    type: message
                    role: assistant
                    model: MiniMax-M3
                    content:
                      - thinking: >-
                          The user is asking in Chinese what the content of the
                          image is. I should describe the image in Chinese.
                        signature: >-
                          6b19dfedcc8d3e065eb0ebe4262efd4a69e3e73ad69eeb7e5e629e552c40f969
                        type: thinking
                      - text: >-
                          这张图片是一张温馨的儿童肖像照片，内容如下：


                          **主体**：一个大约4-6岁的小女孩，有着棕色的卷曲长发和齐刘海，扎着蓬松的发型。她有一双大大的浅棕色（琥珀色）眼睛，长长的睫毛，粉嫩的嘴唇微微上扬，露出甜美自然的微笑。


                          **服饰**：她穿着一件米白色或浅奶油色的连衣裙，领口和肩部有精致的蕾丝花边装饰，袖子是蓬松的荷叶边设计，整体风格显得优雅可爱。


                          **光影与背景**：照片采用了柔和的暖色调光线，背景是朦胧的米灰色/暖灰色，营造出温馨柔和的氛围。光线从侧面打来，突出了她脸部的轮廓和头发的质感。


                          **整体风格**：这是一张高质量的人像摄影作品，采用了浅景深的虚化效果（背景模糊），突出了小女孩纯真可爱的形象，画面充满童真与温暖感。
                        type: text
                    usage:
                      input_tokens: 1209
                      output_tokens: 211
                      cache_creation_input_tokens: 0
                      cache_read_input_tokens: 156
                    stop_reason: end_turn
                视频理解:
                  value:
                    id: 066b4573467fa2bb299cee687b68873d
                    type: message
                    role: assistant
                    model: MiniMax-M3
                    content:
                      - thinking: >-
                          The user is asking in Chinese: "What happened in this
                          video?"


                          I need to describe the events in the video clip
                          provided.


                          1.  **Analyze the initial state (0.0s - 0.2s):** A
                          spotted cat, resembling a Bengal cat, is walking
                          confidently on a springboard, which looks like a
                          diving board. The background shows an Olympic setting
                          ("Lausanne 2022" and the Olympic rings are visible).
                          There's a crowd watching in the background. The cat is
                          moving towards the end of the board.

                          2.  **Observe the action (0.2s - 0.4s):** The cat
                          reaches the end of the springboard and leaps off.

                          3.  **Follow the trajectory (0.4s - 0.5s):** The cat
                          is in mid-air, flying over a swimming pool. It looks
                          like it's doing a dive.

                          4.  **Analyze the final state (0.5s - 0.6s):** The cat
                          splashes into the blue water of the swimming pool,
                          creating a white splash. A person standing by the side
                          of the pool appears to be reacting or clapping.


                          **Synthesize the description:**

                          - Start with the subject: A cat.

                          - Describe its initial action: Walking on a diving
                          board (springboard).

                          - Describe the setting: It looks like an Olympic
                          swimming event (Lausanne 2022).

                          - Describe the main event: The cat jumps off the
                          diving board into the air.

                          - Describe the conclusion: The cat lands in the
                          swimming pool, making a splash.


                          **Translate to Chinese (as requested implicitly by the
                          prompt language, but I should respond in the language
                          the user used. The user asked in Chinese, so I will
                          respond in Chinese. Wait, let me double-check standard
                          practices. Usually, I should answer in the same
                          language as the prompt. Prompt is Chinese. So, answer
                          in Chinese.)


                          **Drafting the Chinese response:**

                          - 视频里，一只猫正在走路。

                          - 它走到跳板的尽头。

                          - 然后它从跳板上跳了下去。

                          - 它在空中飞了一会儿。

                          - 最后，它一头扎进了游泳池里，溅起了一片水花。

                          - 背景看起来像是在洛桑2022年奥运会。


                          Let's refine it into a concise and accurate
                          description.

                          "视频显示，一只猫在跳水板上行走，然后从跳水板上跳下，扎进游泳池里。" (The video shows a
                          cat walking on a diving board, then jumping off it and
                          plunging into a swimming pool.)


                          Let's add a bit more detail based on the visual
                          evidence.

                          "在洛桑2022年奥运会的背景下，一只猫在跳板上走到尽头，然后纵身一跃，跳入泳池中。" (Against
                          the backdrop of the Lausanne 2022 Olympics, a cat
                          walks to the end of a diving board, then leaps and
                          jumps into a swimming pool.)


                          Let's stick to a simple, direct description of the
                          main action.

                          "一只猫在跳板上行走，随后从跳板上跳下，扎进了身后的游泳池中。" (A cat walks on a
                          diving board, then jumps off the board and plunges
                          into the swimming pool behind it.)


                          This is a good, concise summary of the key events. It
                          covers the initial state, the action, and the final
                          state.
                        signature: >-
                          3ebe06a965f7edbb379552ee8f6485100f473eb42f12fb3541d952d5f5908d46
                        type: thinking
                      - text: 视频显示，一只猫在跳板上行走，随后从跳板上跳下，扎进了身后的游泳池中。
                        type: text
                    usage:
                      input_tokens: 349
                      output_tokens: 686
                      cache_creation_input_tokens: 0
                      cache_read_input_tokens: 1894
                    stop_reason: end_turn
                深度思考:
                  value:
                    id: 066b367547c2650d17dc215f503da551
                    type: message
                    role: assistant
                    model: MiniMax-M3
                    content:
                      - thinking: |-
                          The user is asking which is larger: 9.11 or 9.9.

                          Comparing 9.11 and 9.9:
                          9.9 = 9.90
                          9.11 = 9.11

                          9.90 > 9.11, so 9.9 is larger.
                        signature: >-
                          7564f4e0e54b5c08b380d0b800aeb5463ea050e546cec7722e08ef6e912f6c67
                        type: thinking
                      - text: >-
                          **9.9 更大。**


                          比较方法很简单：将两个数的小数位数对齐后比较：


                          - 9.11 = 9.**11**

                          - 9.9 = 9.**90**


                          因为 90 > 11，所以 **9.9 > 9.11**。


                          这是一个常见的思维陷阱——虽然 11 看起来比 9 大，但在比较小数时，应该先比较整数部分（都是
                          9），然后再比较小数部分，而小数部分需要**补齐位数**后再比较。
                        type: text
                    usage:
                      input_tokens: 13
                      output_tokens: 172
                      cache_creation_input_tokens: 0
                      cache_read_input_tokens: 157
                    stop_reason: end_turn
                流式:
                  value:
                    - type: message_start
                      message:
                        id: 066a38a98a4ffbf5554a468fe0cbedf6
                        type: message
                        role: assistant
                        content: []
                        model: MiniMax-M3
                        stop_reason: null
                        stop_sequence: null
                        usage:
                          input_tokens: 0
                          output_tokens: 0
                          cache_creation_input_tokens: 0
                          cache_read_input_tokens: 1365
                        service_tier: standard
                    - type: ping
                    - type: content_block_start
                      index: 0
                      content_block:
                        type: thinking
                        thinking: ''
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: The user
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: |2-
                           is asking in Chinese what the content of this image is. Let me describe it in Chinese.

                          The
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' image shows a close-up portrait of a young girl with curly/wavy brown hair and bangs.'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' She has brown eyes and is smiling slightly at the camera. She''s wearing a cream/iv'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: >-
                          ory colored dress with lace details and ruffles. The
                          lighting is soft and warm, creating a classic
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' portrait photography feel. The background is a neutral olive/grayish color, blurred to keep focus'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' on the subject.'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: signature_delta
                        signature: >-
                          6deea14a039646c64cff2cdc13db8e1aa093c3f7334bfd4941ee1eee8663b214
                    - type: content_block_stop
                      index: 0
                    - type: content_block_start
                      index: 1
                      content_block:
                        type: text
                        text: ''
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: 这张图片是一张温馨的儿童人像摄影作品，主要内容如下
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          ：

                          **人物：**
                          - 一个大约5-7岁的小女孩
                          - 有着卷曲蓬
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          松的棕色头发，留着刘海
                          - 有着明亮的棕色大眼睛
                          - 微微
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          带着甜美的微笑看向镜头

                          **服装：**
                          - 身穿一件米白色/象牙色的
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          连衣裙
                          - 衣领和肩部有精致的蕾丝花边和褶
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          皱装饰

                          **拍摄风格：**
                          - 经典的近距离肖像构图
                          - 柔和的暖色调
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          光线，营造出温馨的氛围
                          - 背景是模糊的橄榄灰色调，使人物更加突出
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-

                          - 整体呈现出专业的人像摄影质感，光影处理细腻

                          这是一张
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: 很有艺术感的儿童写真照，捕捉到了小女孩天真可爱的瞬间。
                    - type: content_block_stop
                      index: 1
                    - type: message_delta
                      delta:
                        stop_reason: end_turn
                      usage:
                        input_tokens: 1251
                        output_tokens: 264
                        cache_creation_input_tokens: 0
                        cache_read_input_tokens: 114
                    - type: message_stop
                工具调用:
                  value:
                    id: 066b13de1224099141792cd806ed4f56
                    type: message
                    role: assistant
                    model: MiniMax-M3
                    content:
                      - thinking: >-
                          The user is asking about the current weather in San
                          Francisco. I should use the get_weather tool to fetch
                          this information. The location parameter requires city
                          and state/country format, so "San Francisco, US" would
                          be appropriate.
                        signature: >-
                          ca8010ef1c5606561a17158c090bd20f33a8a8ac5f3b6cfb4326408395cbff39
                        type: thinking
                      - type: tool_use
                        id: call_function_debp7sfnk20c_1
                        name: get_weather
                        input:
                          location: San Francisco, US
                    usage:
                      input_tokens: 14
                      output_tokens: 79
                      cache_creation_input_tokens: 0
                      cache_read_input_tokens: 397
                    stop_reason: tool_use
            text/event-stream:
              schema:
                $ref: '#/components/schemas/StreamEvent'
              examples:
                流式:
                  value:
                    - type: message_start
                      message:
                        id: 066a38a98a4ffbf5554a468fe0cbedf6
                        type: message
                        role: assistant
                        content: []
                        model: MiniMax-M3
                        stop_reason: null
                        stop_sequence: null
                        usage:
                          input_tokens: 0
                          output_tokens: 0
                          cache_creation_input_tokens: 0
                          cache_read_input_tokens: 1365
                        service_tier: standard
                    - type: ping
                    - type: content_block_start
                      index: 0
                      content_block:
                        type: thinking
                        thinking: ''
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: The user
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: |2-
                           is asking in Chinese what the content of this image is. Let me describe it in Chinese.

                          The
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' image shows a close-up portrait of a young girl with curly/wavy brown hair and bangs.'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' She has brown eyes and is smiling slightly at the camera. She''s wearing a cream/iv'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: >-
                          ory colored dress with lace details and ruffles. The
                          lighting is soft and warm, creating a classic
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' portrait photography feel. The background is a neutral olive/grayish color, blurred to keep focus'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: thinking_delta
                        thinking: ' on the subject.'
                    - type: content_block_delta
                      index: 0
                      delta:
                        type: signature_delta
                        signature: >-
                          6deea14a039646c64cff2cdc13db8e1aa093c3f7334bfd4941ee1eee8663b214
                    - type: content_block_stop
                      index: 0
                    - type: content_block_start
                      index: 1
                      content_block:
                        type: text
                        text: ''
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: 这张图片是一张温馨的儿童人像摄影作品，主要内容如下
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          ：

                          **人物：**
                          - 一个大约5-7岁的小女孩
                          - 有着卷曲蓬
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          松的棕色头发，留着刘海
                          - 有着明亮的棕色大眼睛
                          - 微微
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          带着甜美的微笑看向镜头

                          **服装：**
                          - 身穿一件米白色/象牙色的
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          连衣裙
                          - 衣领和肩部有精致的蕾丝花边和褶
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          皱装饰

                          **拍摄风格：**
                          - 经典的近距离肖像构图
                          - 柔和的暖色调
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-
                          光线，营造出温馨的氛围
                          - 背景是模糊的橄榄灰色调，使人物更加突出
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: |-

                          - 整体呈现出专业的人像摄影质感，光影处理细腻

                          这是一张
                    - type: content_block_delta
                      index: 1
                      delta:
                        type: text_delta
                        text: 很有艺术感的儿童写真照，捕捉到了小女孩天真可爱的瞬间。
                    - type: content_block_stop
                      index: 1
                    - type: message_delta
                      delta:
                        stop_reason: end_turn
                      usage:
                        input_tokens: 1251
                        output_tokens: 264
                        cache_creation_input_tokens: 0
                        cache_read_input_tokens: 114
                    - type: message_stop
        '400':
          description: 请求参数非法（必填缺失、type 不在白名单、tool_use.input 非 JSON 对象等）
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: invalid_request_error
                  message: 'messages.0.content.1: unsupported content type ''foo'''
        '401':
          description: API Key 缺失/无效
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: authentication_error
                  message: API Key 缺失/无效
        '403':
          description: 无权访问该模型或该路径
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: permission_error
                  message: 无权访问该模型或该路径
        '404':
          description: 模型不存在
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: not_found_error
                  message: 模型不存在
        '413':
          description: 请求体超过 64MB，或多模态文件超出大小限制
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: request_too_large
                  message: 请求体超过 64MB，或多模态文件超出大小限制
        '429':
          description: 触发 RPM/TPM/连接数 等限流
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: rate_limit_error
                  message: 触发 RPM/TPM/连接数 等限流
        '500':
          description: 服务端内部错误
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: api_error
                  message: 服务端内部错误
        '529':
          description: 上游模型过载，可重试
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
              example:
                type: error
                request_id: req_xxxxxxxx
                error:
                  type: overloaded_error
                  message: 上游模型过载，可重试
components:
  schemas:
    CreateMessageReq:
      type: object
      required:
        - model
        - messages
      properties:
        model:
          type: string
          description: >-
            模型 ID。MiniMax-M3 是多模态模型，原生支持文本、图片和视频输入，并兼容工具调用与 thinking
            内容块；M2.7、M2.5、M2.1 和 M2 系列仅支持文本与工具调用，不支持图片和视频输入。
          enum:
            - MiniMax-M3
            - MiniMax-M2.7
            - MiniMax-M2.7-highspeed
            - MiniMax-M2.5
            - MiniMax-M2.5-highspeed
            - MiniMax-M2.1
            - MiniMax-M2.1-highspeed
            - MiniMax-M2
        service_tier:
          type: string
          description: >-
            请求准入服务层级。支持的取值为 `standard` 和 `priority`。省略时默认使用
            `standard`。`priority` 的[价格](/guides/pricing-paygo)为 `standard` 的 1.5
            倍，并会确保请求获得优先准入，使其排在其他请求之前处理，从而带来更快响应并减少失败。
          enum:
            - standard
            - priority
          default: standard
        system:
          description: 设置模型角色与行为。
          oneOf:
            - type: string
              description: 纯文本系统提示词
            - type: array
              description: 内容块数组格式的系统提示词。text 块可携带 cache_control。
              items:
                type: object
                properties:
                  type:
                    type: string
                    enum:
                      - text
                    description: 内容块类型
                  text:
                    type: string
                    description: 文本内容
                  cache_control:
                    $ref: '#/components/schemas/CacheControl'
                required:
                  - type
                  - text
        messages:
          type: array
          description: >-
            对话历史。MiniMax-M3 支持文本、图片、视频、工具调用、工具结果和 thinking 内容块。M2.7、M2.5、M2.1 和
            M2 系列仅支持文本与工具调用相关内容块，不支持图片和视频输入。
          items:
            $ref: '#/components/schemas/Message'
        stream:
          type: boolean
          description: 是否使用流式传输，默认为 `false`。设置为 `true` 后，响应将分批返回
          default: false
        max_tokens:
          type: integer
          format: int64
          description: >-
            指定生成内容长度的上限（Token 数）。MiniMax-M3 推荐值为 131072（128K），上限为
            524288（512K）；其他模型推荐值为 65536（64K），上限为 204800（200K）。超过上限的内容会被截断。如果生成因
            `length` 原因中断，请尝试调高此值
          minimum: 1
        temperature:
          type: number
          format: double
          description: 温度系数，影响输出随机性，取值范围 [0, 2]，默认值为 1。值越高，输出越随机；值越低，输出越确定。
          minimum: 0
          maximum: 2
          default: 1
        top_p:
          type: number
          format: double
          description: 核采样参数，取值范围 [0, 1]。MiniMax-M3 默认值为 0.95，M2.x 系列模型默认值为 0.9。
          minimum: 0
          maximum: 1
          default: 0.95
        tools:
          type: array
          description: Anthropic 兼容工具调用的工具定义。
          items:
            $ref: '#/components/schemas/Tool'
        tool_choice:
          $ref: '#/components/schemas/ToolChoice'
        thinking:
          type: object
          description: >-
            控制 MiniMax-M3 thinking。省略时默认关闭 thinking，响应不会包含 thinking 块。对于 M2.x
            模型，thinking 无法关闭。
          properties:
            type:
              type: string
              enum:
                - disabled
                - adaptive
              default: disabled
              description: >-
                thinking 控制类型。

                - `disabled`：关闭 MiniMax-M3 的 thinking 输出。省略 `thinking`
                时默认使用该值。对于 M2.x 模型，thinking 仍会保持开启。

                - `adaptive`：开启 MiniMax-M3 的 thinking 输出，并返回 thinking 块。
          required: []
        metadata:
          type: object
          description: 请求元信息。建议对 to-C 业务传入 user_id，便于按终端用户聚合限流和计费分析。
          properties:
            user_id:
              type: string
              description: 终端用户 ID。
    CreateMessageResp:
      type: object
      properties:
        id:
          type: string
          description: 本次响应的唯一 ID
        type:
          type: string
          description: 对象类型，固定为 `message`
          enum:
            - message
        role:
          type: string
          description: 角色，固定为 `assistant`
          enum:
            - assistant
        model:
          type: string
          description: 本次请求使用的模型 ID
        content:
          type: array
          description: 响应内容块列表
          items:
            $ref: '#/components/schemas/ResponseContentBlock'
        stop_reason:
          type: string
          description: |-
            模型停止生成的原因：
            - end_turn：模型自然结束
            - max_tokens：达到 max_tokens 限制
            - tool_use：模型请求工具调用
          enum:
            - end_turn
            - max_tokens
            - tool_use
        usage:
          $ref: '#/components/schemas/Usage'
    StreamEvent:
      type: object
      description: ''
      required:
        - type
      properties:
        type:
          type: string
          description: |-
            事件类型：
            - `message_start`: 消息开始，包含完整的消息元数据
            - `ping`: 心跳事件
            - `content_block_start`: 内容块开始
            - `content_block_delta`: 内容块增量更新
            - `content_block_stop`: 内容块结束
            - `message_delta`: 消息级别的增量更新（如 stop_reason）
            - `message_stop`: 消息结束
          enum:
            - message_start
            - ping
            - content_block_start
            - content_block_delta
            - content_block_stop
            - message_delta
            - message_stop
        message:
          type: object
          description: 消息对象（`type` 为 `message_start` 时返回）
          properties:
            id:
              type: string
              description: 消息的唯一 ID
            type:
              type: string
              enum:
                - message
            role:
              type: string
              enum:
                - assistant
            content:
              type: array
              description: 内容块列表，初始为空数组
              items:
                $ref: '#/components/schemas/ResponseContentBlock'
            model:
              type: string
              description: 模型 ID
            stop_reason:
              type: string
              nullable: true
              description: 停止原因，流式开始时为 null
            stop_sequence:
              type: string
              nullable: true
              description: 停止序列，流式开始时为 null
            usage:
              $ref: '#/components/schemas/Usage'
        index:
          type: integer
          description: >-
            内容块的索引（`content_block_start`、`content_block_delta`、`content_block_stop`
            时返回）
        content_block:
          $ref: '#/components/schemas/ResponseContentBlock'
          description: 内容块对象（`type` 为 `content_block_start` 时返回）
        delta:
          type: object
          description: 增量更新内容（`content_block_delta` 或 `message_delta` 时返回）
          properties:
            type:
              type: string
              description: 增量类型，例如 text_delta、thinking_delta 或 signature_delta。
              enum:
                - text_delta
                - thinking_delta
                - signature_delta
            text:
              type: string
              description: 增量文本内容
            stop_reason:
              type: string
              description: 模型停止生成的原因。
              enum:
                - end_turn
                - max_tokens
                - tool_use
        usage:
          $ref: '#/components/schemas/Usage'
          description: Token 使用情况（`message_delta` 时返回）
    ErrorResponse:
      type: object
      description: >-
        错误响应。统一使用 HTTP 状态码 + JSON body。流式过程中出现错误时，会以 `event: error` SSE
        事件下发，body 结构与此一致；客户端应在收到 error 后停止读取并清理本次会话状态。
      properties:
        type:
          type: string
          enum:
            - error
          description: 固定为 `error`。
        request_id:
          type: string
          description: 本次请求的唯一标识，便于排查问题。
        error:
          type: object
          properties:
            type:
              type: string
              description: 错误类型。
              enum:
                - invalid_request_error
                - authentication_error
                - permission_error
                - not_found_error
                - request_too_large
                - rate_limit_error
                - api_error
                - overloaded_error
            message:
              type: string
              description: 错误详情。
    CacheControl:
      type: object
      description: 提示词缓存标记。
      properties:
        type:
          type: string
          enum:
            - ephemeral
          description: 提示词缓存标记。
      required:
        - type
    Message:
      type: object
      required:
        - role
        - content
      properties:
        role:
          type: string
          enum:
            - user
            - assistant
            - user_system
            - group
            - sample_message_user
            - sample_message_ai
          description: 消息发送方角色。MiniMax-M3 使用 user / assistant 交替消息。
        content:
          description: 消息内容。MiniMax-M3 支持文本、图片、视频、工具调用、工具结果和 thinking 内容块。
          oneOf:
            - type: string
              description: 纯文本消息
            - type: array
              description: 消息内容。MiniMax-M3 支持文本、图片、视频、工具调用、工具结果和 thinking 内容块。
              items:
                $ref: '#/components/schemas/RequestContentBlock'
    Tool:
      type: object
      description: Anthropic 兼容工具调用的工具定义。
      properties:
        name:
          type: string
          description: 工具名称。
        description:
          type: string
          description: 工具描述。
        input_schema:
          type: object
          description: 工具输入参数 JSON Schema。
        cache_control:
          $ref: '#/components/schemas/CacheControl'
      required:
        - name
        - input_schema
    ToolChoice:
      type: object
      description: 工具选择策略。仅支持 auto 和 none。
      properties:
        type:
          type: string
          enum:
            - auto
            - none
          description: 工具选择类型。
      required:
        - type
    ResponseContentBlock:
      type: object
      description: >-
        响应消息中的内容块。模型输出 text、tool_use（调用工具时），以及 thinking（开启深度思考时）。响应中不会出现
        image、video、tool_result 或 mid_conv_system 块。
      required:
        - type
      properties:
        type:
          type: string
          description: |-
            内容块类型：
            - text：文本内容
            - tool_use：模型工具调用
            - thinking：模型思考过程
          enum:
            - text
            - tool_use
            - thinking
        text:
          type: string
          description: 文本内容，type=text 时使用。
        id:
          type: string
          description: 工具调用 ID，type=tool_use 时返回。
        name:
          type: string
          description: 工具名称，type=tool_use 时返回。
        input:
          type: object
          description: 工具输入参数，type=tool_use 时返回。
        thinking:
          type: string
          description: 模型思考过程内容，type=thinking 时使用。
        signature:
          type: string
          description: thinking 内容签名，多轮续写时需要原样回带。
    Usage:
      type: object
      description: 本次请求的 token 用量，包含适用时的 prompt cache 用量。
      properties:
        input_tokens:
          type: integer
          description: 输入消耗的 Token 数
        output_tokens:
          type: integer
          description: 输出消耗的 Token 数
        cache_creation_input_tokens:
          type: integer
          description: 创建 prompt cache 的输入 token 数。
        cache_read_input_tokens:
          type: integer
          description: 命中 prompt cache 的输入 token 数。
    RequestContentBlock:
      type: object
      description: >-
        请求消息中可用的内容块。text 与工具相关块所有模型均支持；image 与 video 块仅 MiniMax-M3 支持。thinking
        仅在多轮对话中将上一轮 assistant 输出原样回带时使用。
      required:
        - type
      properties:
        type:
          type: string
          description: |-
            内容块类型：
            - text：文本内容
            - image：图片输入，仅 MiniMax-M3 支持
            - video：视频输入，仅 MiniMax-M3 支持
            - tool_use：回带上一轮的 assistant 工具调用
            - tool_result：工具执行结果
            - thinking：回带上一轮的 assistant 思考内容
            - mid_conv_system：对话中途插入的系统指令
          enum:
            - text
            - image
            - video
            - tool_use
            - tool_result
            - thinking
            - mid_conv_system
        text:
          type: string
          description: 文本内容，type=text 时使用。
        source:
          $ref: '#/components/schemas/MediaSource'
          description: 图片或视频来源，type=image 或 type=video 时使用，仅 MiniMax-M3 支持。
        id:
          type: string
          description: 回带 type=tool_use 时使用的工具调用 ID。
        name:
          type: string
          description: 回带 type=tool_use 时使用的工具名称。
        input:
          type: object
          description: 回带 type=tool_use 时使用的工具输入。
        tool_use_id:
          type: string
          description: 对应 tool_use 块的 ID，type=tool_result 时使用。
        content:
          oneOf:
            - type: string
            - type: array
              items:
                $ref: '#/components/schemas/RequestContentBlock'
          description: 工具执行结果，可为字符串或 text/image 内容块数组。
        thinking:
          type: string
          description: 回带的 assistant 思考过程内容，type=thinking 时使用。
        signature:
          type: string
          description: 回带的 thinking 内容签名，多轮续写时需要原样回带。
        cache_control:
          $ref: '#/components/schemas/CacheControl'
    MediaSource:
      type: object
      description: >-
        图片或视频内容块的媒体来源。


        **支持的图片格式**


        | 格式 | 常见拓展名 | MIME Type |

        | :-- | :-- | :-- |

        | JPEG | .jpg, .jpeg | image/jpeg |

        | PNG | .png | image/png |

        | GIF | .gif | image/gif |

        | WEBP | .webp | image/webp |


        **支持的视频格式**


        | 格式 | 常见拓展名 | MIME Type |

        | :-- | :-- | :-- |

        | MP4 | .mp4 | video/mp4 |

        | AVI | .avi | video/avi 或 video/x-msvideo |

        | MOV | .mov | url 传入视频：对象存储请设置 Content-Type 为 video/quicktime；base64
        编码：请使用 video/mov，即 data:video/mov;base64,<BASE64_ENCODING> |

        | MKV | .mkv | video/x-matroska |


        **多模态文件大小及容量限制**

        - URL / Base64 方式传入：视频文件 ≤ 50 MB，图片文件 ≤ 10 MB，请求体 ≤ 64 MB。

        - 使用 Files API 上传：以 `mm_file://{file_id}` 形式引用，单个视频最大 512 MB。
      properties:
        type:
          type: string
          enum:
            - base64
            - url
          description: 媒体来源类型。
        media_type:
          type: string
          description: base64 输入时必填，例如 image/png 或 video/mp4。
        data:
          type: string
          description: base64 编码后的媒体字节。
        url:
          type: string
          description: >-
            公网 URL。视频还可使用 Files API 引用 `mm_file://{file_id}`，`file_id`
            需先通过[文件上传](/api-reference/file-management-upload)接口上传视频后获取。
        detail:
          type: string
          enum:
            - low
            - default
            - high
          default: default
          description: >-
            理解精细度，默认 default。


            图片输入时，单张图片粗略 token 用量估算：


            | detail | 粗略 token 用量 |

            | :-- | :-- |

            | low | 通常为几百 token，最高约 600 |

            | default | 通常约 1k-3k token，最高约 5k |

            | high | 通常为数千 token，最高约 15k+ |


            实际用量取决于图片尺寸和内容；请以 `POST /anthropic/v1/messages/count_tokens` 或响应中的
            usage 为准。
        fps:
          type: number
          minimum: 0.2
          maximum: 5
          default: 1
          description: |-
            视频抽帧频率，默认 1，范围 [0.2, 5]。
            - 取值越高：对画面变化越敏感，token 花费高、速度慢。
            - 取值越低：token 花费少、速度快，但对画面变化迟钝。
        max_long_side_pixel:
          type: integer
          minimum: 1
          description: 图片或视频帧最长边像素约束。
      required:
        - type
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      description: >-
        Bearer API Key 鉴权。发送 Authorization: Bearer <API_KEY>。如果 Authorization 和
        x-api-key 同时存在，优先使用 Authorization。
    apiKeyAuth:
      type: apiKey
      in: header
      name: x-api-key
      description: >-
        Anthropic 兼容 API Key 鉴权。发送 x-api-key: <API_KEY>。推荐使用 Authorization:
        Bearer <API_KEY>。

````
