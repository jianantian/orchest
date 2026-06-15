部分大模型具备图片视觉理解能力，支持本地文件和图片 URL 方式传入图片，适用于图片描述、分类、视觉定位等场景。

<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">方舟平台的新用户？获取 API Key 及 开通模型等准备工作，请参见 <a href="https://www.volcengine.com/docs/82379/1399008">快速入门</a>。</div>


<span id="18cf565a"></span>
# 快速开始

通过图片 URL 方式传入模型快速体验图片理解效果，Responses API 示例代码如下。


<span aceTableMode="list" aceTableWidth="1,1"></span>
|输入 |输出预览 |
|---|---|
|<span>![图片](https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/e3b9829615f54bb88a91892e2b3fb1a3~tplv-goo7wpa0wc-image.image) </span><br><br>> 支持输入图片的模型系列是哪个？ |* 思考：用户现在需要找支持输入图片的模型系列，看表格里的输入列中的图片那一行。表格里模型系列Doubao\-Seed\-1.8对应的输入图片列是√，其他DeepSeek\-V3.2和GLM\-4.7对应的输入图片都是×，所以答案应该是Doubao\-Seed\-1.8。<br><br>* 回答：支持输入图片的模型系列是Doubao\-Seed\-1.8。 |



<Tabs>
<Tab zoneid="Vc60fhWLrF" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/responses \
-H "Authorization: Bearer $ARK_API_KEY" \
-H 'Content-Type: application/json' \
-d '{
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
                },
                {
                    "type": "input_text",
                    "text": "支持输入图片的模型系列是哪个？"
                }
            ]
        }
    ]
}'
```



</Tab>
<Tab zoneid="TnzuqA95DF" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

api_key = os.getenv('ARK_API_KEY')

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [

                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                },
            ],
        }
    ]
)

print(response)
```



</Tab>
<Tab zoneid="Fwh2f4OZ8z" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"
    
    "github.com/samber/lo"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        // Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "Which model series supports image input?",
                    },
                },
            },
        },
    }

    resp, err := client.CreateResponses(ctx, &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{{
                    Union: &responses.InputItem_InputMessage{
                        InputMessage: inputMessage,
                    },
                }}},
            },
        },
    })
    if err != nil {
        fmt.Printf("response error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
<Tab zoneid="StUb3w3BI5" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.example;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemImage;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.response.ResponseObject;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;


public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService arkService = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .input(ResponsesInput.builder().addListItem(
                        ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png").build())
                                        .addListItem(InputContentItemText.builder().text("Which model series supports image input?").build())
                                        .build()
                        ).build()
                ).build())
                .build();
        ResponseObject resp = arkService.createResponse(request);
        System.out.println(resp);

        arkService.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="OGkyRwzaVR" title="OpenAI SDK">
<TabTitle>OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI

api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [

                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                },
            ],
        }
    ]
)

print(response)
```



</Tab>
</Tabs>


<span id="f8d6cc48"></span>
# 模型与API

支持的模型：


* 请参见[视觉理解能力](https://www.volcengine.com/docs/82379/1330310#ff5ef604)。


支持的 API：


* [Responses API](https://www.volcengine.com/docs/82379/1569618)：支持图片作为输入进行分析。支持文件路径上传进行图片理解，使用方式参见[文件路径上传（推荐）](https://www.volcengine.com/docs/82379/1362931#2c38c01b)。

* [Chat API](https://www.volcengine.com/docs/82379/1494384)：支持图片作为输入进行分析。


<span id="547c81e8"></span>
# 图片传入方式

支持的图片传入方式如下：


* 本地文件上传：

   * [文件路径上传（推荐）](https://www.volcengine.com/docs/82379/1362931#2c38c01b)：直接传入本地文件路径，文件大小不能超过 512 MB。

   * [Base64 编码传入](https://www.volcengine.com/docs/82379/1362931#477e51ce)：适用于图片文件体积较小的场景，单张图片小于 10 MB，请求体不能超过 64 MB。

* [图片 URL 传入](https://www.volcengine.com/docs/82379/1362931#d86010f4)：适用于图片文件已存在公网可访问 URL 的场景，单张图片小于 10 MB。


<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">Chat API 是无状态的，如需模型对同一张图片进行多轮理解，则每次请求时都需传入该图片信息。</div>


<span id="dbbdddbe"></span>
## 本地文件上传

<span id="2c38c01b"></span>
### 文件路径上传（推荐）

建议优先采用文件路径方式上传本地文件，该方式可以支持最大 512MB 文件的处理。（当前 Responses API 支持该方式）

直接向模型传入本地文件路径，会自动调用 Files API 完成文件上传，再调用 Responses API 进行图片分析。仅 Python SDK 和 Go SDK 支持该方式。具体示例如下：


> * 如果需要实时获取分析内容，或者要规避复杂任务引发的客户端超时失败问题，可采用流式输出的方式，使用方式可参见[示例代码](https://www.volcengine.com/docs/82379/2123275#9346c907)。

> * 支持直接使用 Files API 上传本地文件，具体请参见[文件输入(File API)](https://www.volcengine.com/docs/82379/1885708)。



<Tabs>
<Tab zoneid="giHLKsqBfs" title="Python">
<TabTitle>Python</TabTitle>

```Python
import asyncio
import os
from volcenginesdkarkruntime import AsyncArk

client = AsyncArk(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)
async def main():
    local_path = "/Users/doc/ark_demo_img_1.png"
    response = await client.responses.create(
        model="doubao-seed-2-0-lite-260215",
        input=[
            {"role": "user", "content": [
                {
                    "type": "input_image",
                    "image_url": f"file://{local_path}"  
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                }
            ]},
        ]
    )
    print(response)
if __name__ == "__main__":
    asyncio.run(main())
```



</Tab>
<Tab zoneid="K6t4M3JFNa" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main
import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)
func main() {
    client := arkruntime.NewClientWithApiKey(
        // Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()
    localPath := "/Users/doc/ark_demo_img_1.png"
    imagePath := "file://" + localPath
    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: volcengine.String(imagePath),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "Which model series supports image input?",
                    },
                },
            },
        },
    }
    createResponsesReq := &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{{
                    Union: &responses.InputItem_InputMessage{
                        InputMessage: inputMessage,
                    },
                }}},
            },
        },
    }
    resp, err := client.CreateResponses(ctx, createResponsesReq)
    if err != nil {
        fmt.Printf("stream error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
</Tabs>


<span id="477e51ce"></span>
### Base64 编码传入

将本地文件转换为 Base64 编码字符串，然后提交给大模型。该方式适用于图片文件体积较小的情况，单张图片小于 10 MB，请求体不能超过 64MB。（Responses API 和 Chat API 都支持该方式。）

<div data-tips="true" data-tips-type="warning" data-tips-is-title="true">注意</div>


<div data-tips="true" data-tips-type="warning">将图片文件转换为Base64编码字符串，然后遵循<code>data:{mime_type};base64,{base64_data}</code>格式拼接，传入模型。</div>



* <div data-tips="true" data-tips-type="warning"><code>{mime_type}</code>：文件的媒体类型，需要与文件格式mime_type对应。支持的图片格式详细见<a href="https://www.volcengine.com/docs/82379/1362931#51efc45f">图片格式说明</a>。</div>


* <div data-tips="true" data-tips-type="warning"><code>{base64_data}</code>：文件经过Base64编码后的字符串。</div>




<span aceTableMode="list" aceTableWidth="5,5"></span>
|[Chat API](https://www.volcengine.com/docs/82379/1494384) |[Responses API](https://www.volcengine.com/docs/82379/1569618) |
|---|---|
|```Python```<br>```...```<br>```model="doubao-seed-2-0-lite-260215",```<br>```messages=[```<br>```    {```<br>```        "role": "user",```<br>```        "content": [```<br>```            {```<br>```                "type": "image_url",```<br>```                "image_url": {```<br>```                    "url": f"data:image/png;base64,{base64_image}"```<br>```                }```<br>```            },```<br>```            {```<br>```                "type": "text",```<br>```                "text": "Which model series supports image input?"```<br>```            }```<br>```        ]```<br>```    }```<br>```]```<br>```...```<br> |```Python```<br>```...```<br>```model="doubao-seed-2-0-lite-260215",```<br>```input=[```<br>```    {```<br>```        "role": "user",```<br>```        "content": [```<br>```            {```<br>```                "type": "input_image",```<br>```                "image_url": f"data:image/png;base64,{base64_image}"```<br>```            },```<br>```            {```<br>```                "type": "input_text",```<br>```                "text": "Which model series supports image input?"```<br>```            }```<br>```        ]```<br>```    }```<br>```]```<br>```...```<br> |



* Responses API 示例代码：



<Tabs>
<Tab zoneid="COZh9YV5Ym" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
BASE64_IMAGE=$(base64 < demo.png) && curl https://ark.cn-beijing.volces.com/api/v3/responses \
   -H "Content-Type: application/json"  \
   -H "Authorization: Bearer $ARK_API_KEY"  \
   -d @- <<EOF
   {
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
      {
        "role": "user",
        "content": [
          {
            "type": "input_image",
            "image_url": "data:image/png;base64,$BASE64_IMAGE"
          },
          {
            "type": "input_text",
            "text": "Which model series supports image input?"
          }
        ]
      }
    ]
  }
EOF
```



</Tab>
<Tab zoneid="YYCa31yhBP" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark
import base64
# Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
api_key = os.getenv('ARK_API_KEY')

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)
# Convert local files to Base64-encoded strings.
def encode_file(file_path):
  with open(file_path, "rb") as read_file:
    return base64.b64encode(read_file.read()).decode('utf-8')
base64_file = encode_file("/Users/doc/demo.png")

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [

                {
                    "type": "input_image",
                    "image_url": f"data:image/png;base64,{base64_file}"
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                },
            ],
        }
    ]
)

print(response)
```



</Tab>
<Tab zoneid="onnugvooUX" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "encoding/base64"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
)

func main() {
    // Convert local files to Base64-encoded strings.
    fileBytes, err := os.ReadFile("/Users/doc/demo.png") 
    if err != nil {
        fmt.Printf("read file error: %v\n", err)
        return
    }
    base64File := base64.StdEncoding.EncodeToString(fileBytes)
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: fmt.Sprintf("data:image/png;base64,%s", base64File),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "Which model series supports image input?",
                    },
                },
            },
        },
    }

    resp, err := client.CreateResponses(ctx, &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{{
                    Union: &responses.InputItem_InputMessage{
                        InputMessage: inputMessage,
                    },
                }}},
            },
        },
    })
    if err != nil {
        fmt.Printf("response error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
<Tab zoneid="TRo460iQKP" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemImage;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.response.ResponseObject;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Base64;
import java.io.IOException;

public class demo {
    private static String encodeFile(String filePath) throws IOException {
        byte[] fileBytes = Files.readAllBytes(Paths.get(filePath));
        return Base64.getEncoder().encodeToString(fileBytes);
    }
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService arkService = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();
        // Convert local files to Base64-encoded strings.
        String base64Data = "";
        try {
            base64Data = "data:image/png;base64," + encodeFile("/Users/demo.png");
        } catch (IOException e) {
            System.err.println("encode error: " + e.getMessage());
        }
        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .input(ResponsesInput.builder().addListItem(
                        ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemImage.builder().imageUrl(base64Data).build())
                                        .addListItem(InputContentItemText.builder().text("Which model series supports image input?").build())
                                        .build()
                        ).build()
                ).build())
                .build();
        ResponseObject resp = arkService.createResponse(request);
        System.out.println(resp);

        arkService.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="sbty81goXU" title="OpenAI SDK">
<TabTitle>OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI
import base64
api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)
# Convert local files to Base64-encoded strings.
def encode_file(file_path):
  with open(file_path, "rb") as read_file:
    return base64.b64encode(read_file.read()).decode('utf-8')
base64_file = encode_file("/Users/doc/demo.png")

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [

                {
                    "type": "input_image",
                    "image_url": f"data:image/png;base64,{base64_file}",
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                },
            ],
        }
    ]
)

print(response)
```



</Tab>
</Tabs>



* Chat API 示例代码：



<Tabs>
<Tab zoneid="KKfunwqlX7" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
BASE64_IMAGE=$(base64 < demo.png) && curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json"  \
   -H "Authorization: Bearer $ARK_API_KEY"  \
   -d @- <<EOF
   {
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
      {
        "role": "user",
        "content": [
          {
            "type": "image_url",
            "image_url": {
              "url": "data:image/png;base64,$BASE64_IMAGE"
            }
          },
          {
            "type": "text",
            "text": "Which model series supports image input?"
          }
        ]
      }
    ],
    "max_tokens": 300
  }
EOF
```



</Tab>
<Tab zoneid="lqNi5N3GAh" title="Python">
<TabTitle>Python</TabTitle>

```Python
import base64
import os
# Install SDK:  pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

# 定义方法将指定路径图片转为Base64编码
def encode_image(image_path):
  with open(image_path, "rb") as image_file:
    return base64.b64encode(image_file.read()).decode('utf-8')

# 需传给大模型的图片
image_path = "demo.png"

# 将图片转为Base64编码
base64_image = encode_image(image_path)

completion = client.chat.completions.create(
  # Replace with Model ID
  model = "doubao-seed-2-0-lite-260215",
  messages=[
    {
      "role": "user",
      "content": [
        {
          "type": "image_url",
          "image_url": {
          # 需注意：传入Base64编码遵循格式 data:image/<IMAGE_FORMAT>;base64,{base64_image}：
          # PNG图片："url":  f"data:image/png;base64,{base64_image}"
          # JPEG图片："url":  f"data:image/jpeg;base64,{base64_image}"
          # WEBP图片："url":  f"data:image/webp;base64,{base64_image}"
            "url":  f"data:image/<IMAGE_FORMAT>;base64,{base64_image}"
          },         
        },
        {
          "type": "text",
          "text": "Which model series supports image input?",
        },
      ],
    }
  ],
)

print(completion.choices[0])
```



</Tab>
<Tab zoneid="s9BCZ2OTTm" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "encoding/base64"
    "fmt"
    "os"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    // 读取本地图片文件
    imageBytes, err := os.ReadFile("demo.png") // 替换为实际图片路径
    if err != nil {
        fmt.Printf("读取图片失败: %v\n", err)
        return
    }
    base64Image := base64.StdEncoding.EncodeToString(imageBytes)

    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        // The base URL for model invocation  .
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
        )
    ctx := context.Background()
    req := model.CreateChatCompletionRequest{
        // Replace with Model ID
        Model: "doubao-seed-2-0-lite-260215",
        Messages: []*model.ChatCompletionMessage{
            {
                Role: "user",
                Content: &model.ChatCompletionMessageContent{
                    ListValue: []*model.ChatCompletionMessageContentPart{
                        {
                            Type: "image_url",
                            ImageURL: &model.ChatMessageImageURL{
                                URL: fmt.Sprintf("data:image/png;base64,%s", base64Image),
                            },
                        },
                        {
                            Type: "text",
                            Text: "Which model series supports image input?",
                        },
                    },
                },
            },
        },
    }

    resp, err := client.CreateChatCompletion(ctx, req)
    if err != nil {
        fmt.Printf("standard chat error: %v\n", err)
        return
    }
    fmt.Println(*resp.Choices[0].Message.Content.StringValue)
}
```



</Tab>
<Tab zoneid="mKOrJRSsxi" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.completion.chat.*;
import com.volcengine.ark.runtime.model.completion.chat.ChatCompletionContentPart.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import okhttp3.ConnectionPool;
import okhttp3.Dispatcher;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.io.IOException;

public class Sample {
    static String apiKey = System.getenv("ARK_API_KEY");
    static ConnectionPool connectionPool = new ConnectionPool(5, 1, TimeUnit.SECONDS);
    static Dispatcher dispatcher = new Dispatcher();
    static ArkService service = ArkService.builder()
         .dispatcher(dispatcher)
         .connectionPool(connectionPool)
         .baseUrl("https://ark.cn-beijing.volces.com/api/v3") // The base URL for model invocation  .
         .apiKey(apiKey)
         .build();

    // Base64编码方法
    private static String encodeImage(String imagePath) throws IOException {
        byte[] imageBytes = Files.readAllBytes(Path.of(imagePath));
        return Base64.getEncoder().encodeToString(imageBytes);
    }

    public static void main(String[] args) throws Exception {

        List<ChatMessage> messagesForReqList = new ArrayList<>();

        // 本地图片路径（替换为实际路径）
        String imagePath = "demo.png";

        // 生成Base64数据URL
        String base64Data = "data:image/png;base64," + encodeImage(imagePath);

        // 构建消息内容（修复内容部分构建方式）
        List<ChatCompletionContentPart> contentParts = new ArrayList<>();

        // 图片部分使用builder模式
        contentParts.add(ChatCompletionContentPart.builder()
                 .type("image_url")
                 .imageUrl(new ChatCompletionContentPartImageURL(base64Data))
                 .build());

        // 文本部分使用builder模式
        contentParts.add(ChatCompletionContentPart.builder()
                 .type("text")
                 .text("Which model series supports image input?")
                 .build());

        // 创建消息
        messagesForReqList.add(ChatMessage.builder()
                 .role(ChatMessageRole.USER)
                 .multiContent(contentParts)
                 .build());

        ChatCompletionRequest req = ChatCompletionRequest.builder()
                 .model("doubao-seed-2-0-lite-260215") //Replace with Model ID  .
                 .messages(messagesForReqList)
                 .maxTokens(300)
                 .build();

        service.createChatCompletion(req)
                 .getChoices()
                 .forEach(choice -> System.out.println(choice.getMessage().getContent()));
        // shutdown service after all requests are finished
        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>


<span id="d86010f4"></span>
## 图片 URL 传入

如果图片已存在公网可访问URL，可以在请求中直接填入图片的公网URL，单张图片不能超过 10 MB。（Responses API 和 Chat API 都支持该方式。）

<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">如果使用 URL，建议使用火山引擎TOS（对象存储）存储图片并生成访问链接，不仅能保证图片的稳定存储，还能利用方舟与TOS的内网通信优势，有效降低模型回复的时延和公网流量费用。</div>



<span aceTableMode="list" aceTableWidth="5,5"></span>
|[Chat API](https://www.volcengine.com/docs/82379/1494384) |[Responses API](https://www.volcengine.com/docs/82379/1569618) |
|---|---|
|```Python```<br>```...```<br>```model="doubao-seed-2-0-lite-260215",```<br>```messages=[```<br>```    {```<br>```        "role": "user",```<br>```        "content": [```<br>```            {```<br>```                "type": "image_url",```<br>```                "image_url": {```<br>```                    "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"```<br>```                }```<br>```            },```<br>```            {```<br>```                "type": "text",```<br>```                "text": "Which model series supports image input?"```<br>```            }```<br>```        ]```<br>```    }```<br>```]```<br>```...```<br> |```Python```<br>```...```<br>```model="doubao-seed-2-0-lite-260215",```<br>```input=[```<br>```    {```<br>```        "role": "user",```<br>```        "content": [```<br>```            {```<br>```                "type": "input_image",```<br>```                "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"```<br>```            },```<br>```            {```<br>```                "type": "input_text",```<br>```                "text": "Which model series supports image input?"```<br>```            }```<br>```        ]```<br>```    }```<br>```]```<br>```...```<br> |



* Responses API 示例代码：[快速开始](https://www.volcengine.com/docs/82379/1362931#18cf565a)

* Chat API 示例代码：



<Tabs>
<Tab zoneid="K7CnJIag6j" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json" \
   -H "Authorization: Bearer $ARK_API_KEY" \
   -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"}},
                {"type": "text", "text": "Which model series supports image input?"}
            ]
        }
    ],
    "max_tokens": 300
  }'
```



</Tab>
<Tab zoneid="HWRAJpbs3V" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
# Install SDK:  pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

completion = client.chat.completions.create(
    # Replace with Model ID
    model = "doubao-seed-2-0-lite-260215",
    messages=[
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"}},
                {"type": "text", "text": "Which model series supports image input?"},
            ],
        }
    ],
)

print(completion.choices[0])
```



</Tab>
<Tab zoneid="e0EaAFHbYB" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        //Use os.Getenv to get ARK_API_KEY
        os.Getenv("ARK_API_KEY"),
        // The base URL for model invocation
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    // Create a context background 
    ctx := context.Background()
    // Construct the content of the message
    contentParts := []*model.ChatCompletionMessageContentPart{
        // Image
        {
            Type: "image_url",
            ImageURL: &model.ChatMessageImageURL{
                URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
            },
        },
        // Text
        {
            Type: "text",
            Text: "Which model series supports image input?",
        },
    }
    // Construct chat, specify model and message
    req := model.CreateChatCompletionRequest{
        // Replace with Model ID
       Model: "doubao-seed-2-0-lite-260215",
       Messages: []*model.ChatCompletionMessage{
          {
             // Set message role as user
             Role: model.ChatMessageRoleUser,
             Content: &model.ChatCompletionMessageContent{
                ListValue: contentParts,
             },
          },
       },
    }

    // Send chat, store result in resp and any possible error in err
    resp, err := client.CreateChatCompletion(ctx, req)
    if err!= nil {
       fmt.Printf("standard chat error: %v\n", err)
       return
    }
    // Print response
    fmt.Println(*resp.Choices[0].Message.Content.StringValue)
}
```



</Tab>
<Tab zoneid="zpODxHugnf" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.completion.chat.*;
import com.volcengine.ark.runtime.model.completion.chat.ChatCompletionContentPart.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import okhttp3.ConnectionPool;
import okhttp3.Dispatcher;

public class MultiImageSample {
  static String apiKey = System.getenv("ARK_API_KEY");
  static ConnectionPool connectionPool = new ConnectionPool(5, 1, TimeUnit.SECONDS);
  static Dispatcher dispatcher = new Dispatcher();
  static ArkService service = ArkService.builder()
       .dispatcher(dispatcher)
       .connectionPool(connectionPool)
       .baseUrl("https://ark.cn-beijing.volces.com/api/v3")  // The base URL for model invocation  .
       .apiKey(apiKey)
       .build();

  public static void main(String[] args) throws Exception {

    List<ChatMessage> messagesForReqList = new ArrayList<>();

    // Construct the content of the message
    List<ChatCompletionContentPart> contentParts = new ArrayList<>();

    // Use builder mode for the image
    contentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"))
         .build());

    // Use builder mode for text
    contentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("Which model series supports image input?")
         .build());

    // Create message
    messagesForReqList.add(ChatMessage.builder()
         .role(ChatMessageRole.USER)
         .multiContent(contentParts)
         .build());

    ChatCompletionRequest req = ChatCompletionRequest.builder()
         .model("doubao-seed-2-0-lite-260215") //Replace with Model ID  .
         .messages(messagesForReqList)
         .build();

    service.createChatCompletion(req)
         .getChoices()
         .forEach(choice -> System.out.println(choice.getMessage().getContent()));
    // shutdown service after all requests are finished
    service.shutdownExecutor();
  }
}
```



</Tab>
</Tabs>


<span id="2d7ef2c7"></span>
# 使用场景

<span id="594387aa"></span>
## 多图输入

API 可支持接受和处理多个图像输入，这些图像可通过图片可访问 URL 或图片转为 Base64 编码后输入，模型将结合所有传入的图像中的信息来回答问题。


* Responses API 示例代码：



<Tabs>
<Tab zoneid="F3dFTD7QZP" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/responses \
  -H "Authorization: Bearer $ARK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"
                },
                {
                    "type": "input_text",
                    "text": "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？"
                }
            ]
        }
    ]
  }'
```



</Tab>
<Tab zoneid="RKfiy3j7s4" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"
                },
                {
                    "type": "input_text",
                    "text": "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？"
                }
            ]
        }
    ]
)

print(response.output)
```



</Tab>
<Tab zoneid="VfJfLmiVBn" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/samber/lo"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？",
                    },
                },
            },
        },
    }

    resp, err := client.CreateResponses(ctx, &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{{
                    Union: &responses.InputItem_InputMessage{
                        InputMessage: inputMessage,
                    },
                }}},
            },
        },
    })
    if err != nil {
        fmt.Printf("response error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
<Tab zoneid="aoCGq7PZiQ" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemImage;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.response.ResponseObject;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;


public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService arkService = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .input(ResponsesInput.builder().addListItem(
                        ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png").build())
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png").build())
                                        .addListItem(InputContentItemText.builder().text("支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？").build())
                                        .build()
                        ).build()
                ).build())
                .build();
        ResponseObject resp = arkService.createResponse(request);
        System.out.println(resp);

        arkService.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>



* Chat API 示例代码：



<Tabs>
<Tab zoneid="pzP0teT28S" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json"  \
   -H "Authorization: Bearer $ARK_API_KEY"  \
   -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"}},
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"}},
                {"type": "text", "text": "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？"}
            ]
        }
    ],
    "max_tokens": 300
  }'
```



</Tab>
<Tab zoneid="O5JMnq7TIC" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
# Install SDK:  pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

completion = client.chat.completions.create(
    # Replace with Model ID
    model = "doubao-seed-2-0-lite-260215",
    messages=[
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url":  "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"}},
                {"type": "image_url","image_url": {"url":  "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"}},
                {"type": "text", "text": "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？"},
            ],
        }
    ],
)

print(completion.choices[0])
```



</Tab>
<Tab zoneid="VAAQHXNGOX" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        //Use os.Getenv to get ARK_API_KEY
        os.Getenv("ARK_API_KEY"),
        // The base URL for model invocation
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    // Create context background
    ctx := context.Background()
    // Construct message, including 2 images and a text
    contentParts := []*model.ChatCompletionMessageContentPart{
        // First image
        {
            Type: "image_url",
            ImageURL: &model.ChatMessageImageURL{
                URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
            },
        },
        // Second image
        {
            Type: "image_url",
            ImageURL: &model.ChatMessageImageURL{
                URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png",
            },
        },
        // Text
        {
            Type: "text",
            Text: "支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？",
        },
    }
    // Construct chat request
    req := model.CreateChatCompletionRequest{
        // Replace with Model ID
       Model: "doubao-seed-2-0-lite-260215",
       Messages: []*model.ChatCompletionMessage{
          {
             // Set message role as user
             Role: model.ChatMessageRoleUser,
             Content: &model.ChatCompletionMessageContent{
                ListValue: contentParts, // Use ListValue for multi-type content
             },
          },
       },
       MaxTokens: volcengine.Int(300), // Set max output token count
    }

    // Send the chat completion request
    resp, err := client.CreateChatCompletion(ctx, req)
    if err!= nil {
       fmt.Printf("standard chat error: %v\n", err)
       return
    }
    // Print response
    fmt.Println(*resp.Choices[0].Message.Content.StringValue)
}
```



</Tab>
<Tab zoneid="w8dwaenHGi" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.completion.chat.*;
import com.volcengine.ark.runtime.model.completion.chat.ChatCompletionContentPart.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import okhttp3.ConnectionPool;
import okhttp3.Dispatcher;

public class MultiImageSample {
  static String apiKey = System.getenv("ARK_API_KEY");
  static ConnectionPool connectionPool = new ConnectionPool(5, 1, TimeUnit.SECONDS);
  static Dispatcher dispatcher = new Dispatcher();
  static ArkService service = ArkService.builder()
       .dispatcher(dispatcher)
       .connectionPool(connectionPool)
       .baseUrl("https://ark.cn-beijing.volces.com/api/v3") // The base URL for model invocation
       .apiKey(apiKey)
       .build();

  public static void main(String[] args) throws Exception {

    List<ChatMessage> messagesForReqList = new ArrayList<>();

    // Construct content of the message
    List<ChatCompletionContentPart> contentParts = new ArrayList<>();

    // Use builder mode for the first image
    contentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"))
         .build());

    // 
    contentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_2.png"))
         .build());

    contentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("支持输入图片的模型系列是哪个？同时，豆包应用场景有哪些？")
         .build());

    messagesForReqList.add(ChatMessage.builder()
         .role(ChatMessageRole.USER)
         .multiContent(contentParts)
         .build());

    ChatCompletionRequest req = ChatCompletionRequest.builder()
         .model("doubao-seed-2-0-lite-260215") //Replace with Model ID
         .messages(messagesForReqList)
         .maxTokens(300)
         .build();

    service.createChatCompletion(req)
         .getChoices()
         .forEach(choice -> System.out.println(choice.getMessage().getContent()));
    // shutdown service after all requests are finished
    service.shutdownExecutor();
  }
}
```



</Tab>
</Tabs>


<span id="bf4d9224"></span>
## 控制图片理解的精细度

控制图片理解的精细度（指对画面的精细）： **image_pixel_limit 、detail** 字段，2个字段若同时配置，则生效逻辑如下：


* 生效前提：图片像素范围在 [196, 36,000,000] px，否则直接报错。

* 生效优先级：**image_pixel_limit** 高于 **detail** 字段，即同时配置 **detail** 与 **image_pixel_limit** 字段时，生效 **image_pixel_limit** 字段配置。

* 缺省时生效：**image_pixel_limit** 字段的 **min_pixels** / **max_pixels** 字段未设置，则使用 **detail** 默认值配置所对应的值。具体范围参见[通过 detail 字段（图片理解）](https://www.volcengine.com/docs/82379/1362931#885d96dc)。


下面分别介绍如何通过 **detail** 、 **image_pixel_limit** 控制视觉理解的精度。

<span id="885d96dc"></span>
### 通过 detail 字段（图片理解）

通过`detail`参数来控制模型理解图片的精细度， 不同模型支持的 detail 模式、token 用量、图片像素区间如下：

<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">doubao\-seed\-2.0 模型 detail 默认值为 <code>high</code>，单图固定 1280 个 tokens，在不牺牲效果的同时消耗的 tokens 更少。</div>



<span aceTableMode="list" aceTableWidth="2,2.5,3,2.5,3,3,3"></span>
|detail模式 |doubao\-seed\-1.8 之前的模型<br><br>> detail 默认值为`low` ||doubao\-seed\-1.8 模型<br><br>> detail 默认值为 `high` ||doubao\-seed\-2.0 模型<br><br>> detail 默认值为 `high` ||
|---|---|---|---|---|---|---|
||单图token范围 |图片像素区间 |单图token范围 |图片像素区间 |单图token范围 |图片像素区间 |
|low |[4, 1312] |[3136, 1048576] |[1, 1213] |[1764, 2139732] |[1, 1280] |[1764, 2257920] |
|high |[4, 5120] |[3136, 4014080] |[1, 5120] |[1764, 9031680] |1280 |2257920 |
|xhigh |\- |\- |\- |\- |[1280, 5120] |[2257920, 9031680] |



* detail 为 `low` 时，图片处理速度会提高，适合图片本身细节较少或者只需模型理解图片大致信息或者对速度有要求的场景。

* detail 为 `high` 或 `xhigh` 时，模型可感知图片更多的细节，但是图片处理速度会降低，适合图像像素值高且需关注细节信息的场景，如街道地图分析等。


**图片缩放规则**：不在指定模式对应的图片像素区间时，方舟会等比例缩放至范围内。


* Responses API 示例代码：



<Tabs>
<Tab zoneid="lFmh5pDItH" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/responses \
-H "Authorization: Bearer $ARK_API_KEY" \
-H 'Content-Type: application/json' \
-d '{
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                    "detail": "high"
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                }
            ]
        }
    ]
}'
```



</Tab>
<Tab zoneid="Ehy8r58Hmv" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                    "detail": "high"
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                }
            ]
        }
    ]
)

print(response.output)
```



</Tab>
<Tab zoneid="ocPY0b3Pms" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/samber/lo"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png"),
                        Detail:   lo.ToPtr(responses.ContentItemImageDetail_high),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "Which model series supports image input?",
                    },
                },
            },
        },
    }

    resp, err := client.CreateResponses(ctx, &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{{
                    Union: &responses.InputItem_InputMessage{
                        InputMessage: inputMessage,
                    },
                }}},
            },
        },
    })
    if err != nil {
        fmt.Printf("response error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
<Tab zoneid="IbvlPidJni" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemImage;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.response.ResponseObject;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;


public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService arkService = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .input(ResponsesInput.builder().addListItem(
                        ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png").detail("high").build())
                                        .addListItem(InputContentItemText.builder().text("Which model series supports image input?").build())
                                        .build()
                        ).build()
                ).build())
                .build();
        ResponseObject resp = arkService.createResponse(request);
        System.out.println(resp);

        arkService.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>



* Chat API 示例代码：



<Tabs>
<Tab zoneid="xsYquExPJ2" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json" \
   -H "Authorization: Bearer $ARK_API_KEY" \
   -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png","detail": "high"}},
                {"type": "text", "text": "Which model series supports image input?"}
            ]
        }
    ]
  }'
```



</Tab>
<Tab zoneid="WcVRkv1pTY" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
# Install SDK:  pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

completion = client.chat.completions.create(
    # Replace with Model ID
    model = "doubao-seed-2-0-lite-260215",
    messages=[
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png","detail": "high"}},
                {"type": "text", "text": "Which model series supports image input?"},
            ],
        }
    ],
)

print(completion.choices[0])
```



</Tab>
<Tab zoneid="Ceed1bCmOa" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        //Use os.Getenv to get ARK_API_KEY
        os.Getenv("ARK_API_KEY"),
        // The base URL for model invocation
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    // Construct a context background
    ctx := context.Background()
    // Message content
    contentParts := []*model.ChatCompletionMessageContentPart{
        // Image
        {
            Type: "image_url",
            ImageURL: &model.ChatMessageImageURL{
                URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                Detail: model.ImageURLDetailHigh,
            },
        },
        // Text
        {
            Type: "text",
            Text: "Which model series supports image input?",
        },
    }
    req := model.CreateChatCompletionRequest{
        // Replace with Model ID
       Model: "doubao-seed-2-0-lite-260215",
       Messages: []*model.ChatCompletionMessage{
          {
             Role: model.ChatMessageRoleUser,
             Content: &model.ChatCompletionMessageContent{
                ListValue: contentParts, // Use ListValue for multi-type content
             },
          },
       },
       MaxTokens: volcengine.Int(300), // Max output token
    }

    resp, err := client.CreateChatCompletion(ctx, req)
    if err!= nil {
       fmt.Printf("standard chat error: %v\n", err)
       return
    }
    fmt.Println(*resp.Choices[0].Message.Content.StringValue)
}
```



</Tab>
<Tab zoneid="vSFmt1JvVX" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.completion.chat.*;
import com.volcengine.ark.runtime.model.completion.chat.ChatCompletionContentPart.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import okhttp3.ConnectionPool;
import okhttp3.Dispatcher;

public class MultiImageSample {
  static String apiKey = System.getenv("ARK_API_KEY");
  static ConnectionPool connectionPool = new ConnectionPool(5, 1, TimeUnit.SECONDS);
  static Dispatcher dispatcher = new Dispatcher();
  static ArkService service = ArkService.builder()
       .dispatcher(dispatcher)
       .connectionPool(connectionPool)
       .baseUrl("https://ark.cn-beijing.volces.com/api/v3")  // The base URL for model invocation  .
       .apiKey(apiKey)
       .build();

  public static void main(String[] args) throws Exception {

    List<ChatMessage> messagesForReqList = new ArrayList<>();

    List<ChatCompletionContentPart> contentParts = new ArrayList<>();

    contentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png","high"))
         .build());

    contentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("Which model series supports image input?")
         .build());

    messagesForReqList.add(ChatMessage.builder()
         .role(ChatMessageRole.USER)
         .multiContent(contentParts)
         .build());

    ChatCompletionRequest req = ChatCompletionRequest.builder()
         .model("doubao-seed-2-0-lite-260215") //Replace with Model ID  .
         .messages(messagesForReqList)
         .maxTokens(300)
         .build();

    service.createChatCompletion(req)
         .getChoices()
         .forEach(choice -> System.out.println(choice.getMessage().getContent()));
    // shutdown service after all requests are finished
    service.shutdownExecutor();
  }
}
```



</Tab>
</Tabs>


<span id="d2b576dd"></span>
### **通过 image_pixel_limit 结构体**

控制传入给方舟的图像像素大小范围，如果不在此范围，则会等比例放大或者缩小至该范围内，后传给模型进行理解。你可通过 **image_pixel_limit** 结构体，精细控制模型可理解的图片像素多少。

对应结构体如下：

```Bash
"image_pixel_limit": {
    "max_pixels": 3014080,   # 图片最大像素
    "min_pixels": 3136       # 图片最小像素
}
```


示例代码如下：

> Java SDK、 Go SDK 不支持此字段。


* Responses API 示例代码：



<Tabs>
<Tab zoneid="vDyQU0wgkb" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/responses \
-H "Authorization: Bearer $ARK_API_KEY" \
-H 'Content-Type: application/json' \
-d '{
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                    "image_pixel_limit":  {
                        "max_pixels": 3014080,
                        "min_pixels": 3136
                     }
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                }
            ]
        }
    ]
}'
```



</Tab>
<Tab zoneid="fcWynoT0AK" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                    "image_pixel_limit": {
                        "max_pixels": 3014080,
                        "min_pixels": 3136,
                    }
                },
                {
                    "type": "input_text",
                    "text": "Which model series supports image input?"
                }
            ]
        }
    ]
)

print(response.output)
```



</Tab>
</Tabs>



* Chat API 示例代码：



<Tabs>
<Tab zoneid="Wigv6XmrLr" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json" \
   -H "Authorization: Bearer $ARK_API_KEY" \
   -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
        {
            "role": "user",
            "content": [                
                {"type": "image_url","image_url": {"url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png","image_pixel_limit": {"max_pixels": 3014080,"min_pixels": 3136}}},
                {"type": "text", "text": "Which model series supports image input?"}
            ]
        }
    ],
    "max_tokens": 300
  }'
```



</Tab>
<Tab zoneid="ENVYf82BPe" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
# Install SDK: pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

completion = client.chat.completions.create(
    # Replace with Model ID
    model = "doubao-seed-2-0-lite-260215",
    messages=[
        {
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": {
                        "url":  "https://ark-project.tos-cn-beijing.volces.com/doc_image/ark_demo_img_1.png",
                        "image_pixel_limit": {
                            "max_pixels": 3014080,
                            "min_pixels": 3136,
                        },
                    },
                 },
                {"type": "text", "text": "Which model series supports image input?"},
            ],
        }
    ],
)

print(completion.choices[0])
```



</Tab>
</Tabs>


<span id="474e4601"></span>
## 图文混排

支持灵活地传入提示词和图片信息的方式，你可任意调整传入图片和文本的顺序，以及在`system message`或者`User message`传入图文信息。模型会根据顺序返回处理信息的结果，示例如下。

<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">图文混排场景，图文顺序可能影响模型输出效果，若结果不符预期，可调整顺序。当多图+一段文字时，建议将文字放在图片之后。</div>



* Responses API 示例代码：



<Tabs>
<Tab zoneid="Qax2vBVjwg" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/responses \
-H "Authorization: Bearer $ARK_API_KEY" \
-H 'Content-Type: application/json' \
-d '{
    "model": "doubao-seed-2-0-lite-260215",
    "input": [
        {
            "role": "system",
            "content": [
                {
                    "type": "input_text",
                    "text": "下面人物是目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"
                },
                {
                    "type": "input_text",
                    "text": "请确认下面图片中是否含有目标人物"
                }
            ]
        },
        {
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": "图片1中是否含有目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"
                },
                {
                    "type": "input_text",
                    "text": "图片2中是否含有目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"
                }
            ]
        }
    ]
}'
```



</Tab>
<Tab zoneid="S2EnlL44qX" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "system",
            "content": [
                {
                    "type": "input_text",
                    "text": "下面人物是目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"
                },
                {
                    "type": "input_text",
                    "text": "请确认下面图片中是否含有目标人物"
                }
            ]
        },
        {
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": "图片1中是否含有目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"
                },
                {
                    "type": "input_text",
                    "text": "图片2中是否含有目标人物"
                },
                {
                    "type": "input_image",
                    "image_url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"
                }
            ]
        }
    ]
)

print(response.output)
```



</Tab>
<Tab zoneid="M6OTCebvFM" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/samber/lo"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/responses"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    systemMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_system,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "下面人物是目标人物",
                    },
                },
            },
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "请确认下面图片中是否含有目标人物",
                    },
                },
            },
        },
    }

    userMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "图片1中是否含有目标人物",
                    },
                },
            },
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "图片2中是否含有目标人物",
                    },
                },
            },
            {
                Union: &responses.ContentItem_Image{
                    Image: &responses.ContentItemImage{
                        Type:     responses.ContentItemType_input_image,
                        ImageUrl: lo.ToPtr("https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"),
                    },
                },
            },
        },
    }

    resp, err := client.CreateResponses(ctx, &responses.ResponsesRequest{
        Model: "doubao-seed-2-0-lite-260215",
        Input: &responses.ResponsesInput{
            Union: &responses.ResponsesInput_ListValue{
                ListValue: &responses.InputItemList{ListValue: []*responses.InputItem{
                    {
                        Union: &responses.InputItem_InputMessage{
                            InputMessage: systemMessage,
                        },
                    },
                    {
                        Union: &responses.InputItem_InputMessage{
                            InputMessage: userMessage,
                        },
                    },
                }},
            },
        },
    })
    if err != nil {
        fmt.Printf("response error: %v\n", err)
        return
    }
    fmt.Println(resp)
}
```



</Tab>
<Tab zoneid="nIzNRMQ1LX" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemImage;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.response.ResponseObject;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;


public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService arkService = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .input(ResponsesInput.builder()
                        .addListItem(ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_SYSTEM).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemText.builder().text("下面人物是目标人物").build())
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png").build())
                                        .addListItem(InputContentItemText.builder().text("请确认下面图片中是否含有目标人物").build())
                                        .build()
                        ).build())
                        .addListItem(ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemText.builder().text("图片1中是否含有目标人物").build())
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png").build())
                                        .addListItem(InputContentItemText.builder().text("图片2中是否含有目标人物").build())
                                        .addListItem(InputContentItemImage.builder().imageUrl("https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png").build())
                                        .build()
                        ).build())
                        .build()
                ).build();

        ResponseObject resp = arkService.createResponse(request);
        System.out.println(resp);

        arkService.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>



* Chat API 示例代码：



<Tabs>
<Tab zoneid="h0jQleySLd" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/chat/completions \
   -H "Content-Type: application/json" \
   -H "Authorization: Bearer $ARK_API_KEY" \
   -d '{
    "model": "doubao-seed-2-0-lite-260215",
    "messages": [
        {
            "role": "system",
            "content": [
                {"type": "text", "text": "下面人物是目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"
                    }
                },
                {"type": "text", "text": "请确认下面图片中是否含有目标人物"}
            ]
        },
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "图片1中是否含有目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"
                    }
                },
                {"type": "text", "text": "图片2中是否含有目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"
                    }
                }
            ]
        }
    ],
    "max_tokens": 300
  }'
```



</Tab>
<Tab zoneid="vAGZSfuQld" title="Python">
<TabTitle>Python</TabTitle>

```Python
import os
# Install SDK:  pip install 'volcengine-python-sdk[ark]'
from volcenginesdkarkruntime import Ark 

client = Ark(
    # The base URL for model invocation
    base_url="https://ark.cn-beijing.volces.com/api/v3", 
    # Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
    api_key=os.getenv('ARK_API_KEY'), 
)

completion = client.chat.completions.create(
    # Replace with Model ID
    model = "doubao-seed-2-0-lite-260215",
    messages=[
        {
            "role": "system",
            "content": [
                {"type": "text", "text": "下面人物是目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"
                    },
                },
                {"type": "text", "text": "请确认下面图片中是否含有目标人物"},
            ],
        },
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "图片1中是否含有目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"
                    },
                },
                {"type": "text", "text": "图片2中是否含有目标人物"},
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"
                    },
                },
            ],
        },
    ],
)


print(completion.choices[0].message.content)
```



</Tab>
<Tab zoneid="fwWUveShBC" title="Go">
<TabTitle>Go</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        // The base URL for model invocation
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    // Create a context, typically used to pass request context information, such as timeouts and cancellations
  ctx := context.Background()

  // Build the system message content
  systemContentParts := []*model.ChatCompletionMessageContentPart{
    // Text content
    {
      Type: "text",
      Text: "下面人物是目标人物",
    },
    // Target person image
    {
      Type: "image_url",
      ImageURL: &model.ChatMessageImageURL{
        URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png",
      },
    },
    // Text content
    {
      Type: "text",
      Text: "请确认下面图片中是否含有目标人物",
    },
  }

  // Build the user message content
  userContentParts := []*model.ChatCompletionMessageContentPart{
    // Text
    {
      Type: "text",
      Text: "图片1中是否含有目标人物",
    },
    // First scene image
    {
      Type: "image_url",
      ImageURL: &model.ChatMessageImageURL{
        URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png",
      },
    },
    // Text
    {
      Type: "text",
      Text: "图片2中是否含有目标人物",
    },
    // Second scene image
    {
      Type: "image_url",
      ImageURL: &model.ChatMessageImageURL{
        URL: "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png",
      },
    },
  }

  // Build a chat completion request and set the model and message content
  req := model.CreateChatCompletionRequest{
    // Replace with Model ID
    Model: "doubao-seed-2-0-lite-260215",
    Messages: []*model.ChatCompletionMessage{
      {
        // The message role is system
        Role: model.ChatMessageRoleSystem,
        Content: &model.ChatCompletionMessageContent{
          ListValue: systemContentParts,
        },
      },
      {
        // The message role is user
        Role: model.ChatMessageRoleUser,
        Content: &model.ChatCompletionMessageContent{
          ListValue: userContentParts,
        },
      },
    },
    MaxTokens: volcengine.Int(300),
  }

    // Send the chat completion request, store the result in resp, and store any possible errors in err
    resp, err := client.CreateChatCompletion(ctx, req)
    if err!= nil {
       fmt.Printf("standard chat error: %v\n", err)
       return
    }
    fmt.Println(*resp.Choices[0].Message.Content.StringValue)
}
```



</Tab>
<Tab zoneid="BHIriHJeeT" title="Java">
<TabTitle>Java</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.completion.chat.*;
import com.volcengine.ark.runtime.model.completion.chat.ChatCompletionContentPart.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import okhttp3.ConnectionPool;
import okhttp3.Dispatcher;

public class MultiImageSample {
  static String apiKey = System.getenv("ARK_API_KEY");
  static ConnectionPool connectionPool = new ConnectionPool(5, 1, TimeUnit.SECONDS);
  static Dispatcher dispatcher = new Dispatcher();
  static ArkService service = ArkService.builder()
       .dispatcher(dispatcher)
       .connectionPool(connectionPool)
       .baseUrl("https://ark.cn-beijing.volces.com/api/v3")  // The base URL for model invocation
       .apiKey(apiKey)
       .build();

  public static void main(String[] args) throws Exception {
    List<ChatMessage> messagesForReqList = new ArrayList<>();
    
    // Build the system message content
    List<ChatCompletionContentPart> systemContentParts = new ArrayList<>();
    systemContentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("下面人物是目标人物")
         .build());
    systemContentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/target.png"))
         .build());
    systemContentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("请确认下面图片中是否含有目标人物")
         .build());

    // Create the system message
    messagesForReqList.add(ChatMessage.builder()
         .role(ChatMessageRole.SYSTEM)
         .multiContent(systemContentParts)
         .build());

    // Build the user message content
    List<ChatCompletionContentPart> userContentParts = new ArrayList<>();
    userContentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("图片1中是否含有目标人物")
         .build());
    userContentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_01.png"))
         .build());
    userContentParts.add(ChatCompletionContentPart.builder()
         .type("text")
         .text("图片2中是否含有目标人物")
         .build());
    userContentParts.add(ChatCompletionContentPart.builder()
         .type("image_url")
         .imageUrl(new ChatCompletionContentPartImageURL(
            "https://ark-project.tos-cn-beijing.volces.com/doc_image/scene_02.png"))
         .build());

    // Create user message
    messagesForReqList.add(ChatMessage.builder()
         .role(ChatMessageRole.USER)
         .multiContent(userContentParts)
         .build());
    ChatCompletionRequest req = ChatCompletionRequest.builder()
         .model("doubao-seed-2-0-lite-260215") //Replace with Model ID
         .messages(messagesForReqList)
         .maxTokens(300)
         .build();

    service.createChatCompletion(req)
         .getChoices()
         .forEach(choice -> System.out.println(choice.getMessage().getContent()));
    // shutdown service after all requests are finished
    service.shutdownExecutor();
  }
}
```



</Tab>
</Tabs>


<span id="5fdeb294"></span>
## 视觉定位（Visual Grounding）

请参见教程 [视觉定位 Grounding](https://www.volcengine.com/docs/82379/1616136)。

<span id="52afa2e1"></span>
## GUI任务处理

请参见教程 [GUI 任务处理](https://www.volcengine.com/docs/82379/1584296)。

<span id="7a123cd1"></span>
# 使用说明

<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">处理完图片/视频后，文件会从方舟服务器删除。方舟不会保留你提交的图片、视频以及文本信息等用户数据来训练模型。</div>


<span id="f141b9ef"></span>
## 图片像素说明


1. 传入图片像素要求如下，超出限制后会直接报错。

   * 宽 \> 14px 且高 \> 14px

   * 宽\*高范围：[196px, 36000000px]

   * 宽高比范围：[1/150, 150]

2. 图片预处理：

   根据使用的模型、设置的 detail 模式，将图片等比例缩放至相应的范围（具体见[通过 detail 字段（图片理解）](https://www.volcengine.com/docs/82379/1362931#885d96dc)），可降低模型响应时延及 token 消耗。


<span id="57188ace"></span>
## 图片 token 用量说明

根据图片宽高像素计算得到 token 用量。不同模型的图片 token 用量估算逻辑如下。单图 token 范围参见[通过 detail 字段（图片理解）](https://www.volcengine.com/docs/82379/1362931#885d96dc)。


<span aceTableMode="list" aceTableWidth="1,1"></span>
|doubao\-seed\-1.8 之前的模型 |doubao\-seed\-1.8 模型、doubao\-seed\-2.0 模型 |
|---|---|
|```JSON```<br>```min(image_width * image_hight ÷ 784, max_image_tokens)```<br> |```JSON```<br>```min(image_width * image_hight ÷ 1764, max_image_tokens)```<br> |


以传入模型的单图 token 最大值为 1312 为例，计算图片消耗的 token 数的逻辑如下：


* 图片尺寸为 `1280 px × 720 px`：理解这张图消耗的 token 为`1280×720÷784=1176`，该值小于 1312，根据公式计算消耗 token 数为 1176。

* 图片尺寸为 `1920 px × 1080 px`：理解这张图消耗的 token 为`1920×1080÷784=2645`，该值大于 1312，根据公式计算消耗 token 数为 1312。

   这种情况会对图片进行压缩，即图片会丢失部分细节。譬如字体很小的图片，模型可能会无法识别文字内容。


<span id="4ecbf924"></span>
## 图片数量说明

单次请求传入图片数量受限于模型上下文窗口。当输入过长，触发模型上下文窗口，信息会被截断。

> 模型上下文窗口请参见[模型列表](https://www.volcengine.com/docs/82379/1330310)。

> 举例说明：

> * 当图片总像素值大，使用的模型上下文窗口为 32k token，每张图片转为 1312 token ，单次请求可传入的图片数量为 `32000 ÷ 1312 = 24`张。

> * 当图片总像素值小，使用的模型上下文窗口为 32k token，每张图片转为 256 token，单次请求可传入的数量为 `32000 ÷ 256 = 125` 张。


<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">模型回复的质量，受输入图片信息量影响。过多的图片会导致模型回复质量下滑，请合理控制单次请求传入图片的数量。</div>


<span id="3d62f9e9"></span>
## 图片文件容量

使用 URL 方式传入图片，单张图片不能超过 10MB。

使用 Base64 编码传入图片，单张图片不能超过 10MB，请求体不能超过 64MB。

使用文件路径传入图片，图片不能超过 512 MB。

<span id="51efc45f"></span>
## 图片格式说明

支持的图片格式如下表，注意文件后缀匹配图片格式，即图片文件扩展名（URL传入时）、图片格式声明（Base64 编码传入时）需与图片实际信息一致。


<span aceTableMode="list" aceTableWidth="1,1,2"></span>
|**图片格式** |**文件扩展名** |**内容格式** **Content Type** |
|---|---|---|
|JPEG |.jpg, .jpeg |`image/jpeg` |
|PNG |.png |`image/png` |
|GIF |.gif |`image/gif` |
|WEBP |.webp |`image/webp` |
|BMP |.bmp |`image/bmp` |
|TIFF |.tiff, .tif |`image/tiff` |
|ICO |.ico |`image/ico` |
|DIB |.dib |`image/bmp` |
|ICNS |.icns |`image/icns` |
|SGI |.sgi |`image/sgi` |
|JPEG2000 |.j2c, .j2k, .jp2, .jpc, .jpf, .jpx |`image/jp2` |
|HEIC |.heic |`image/heic`<br><br>> doubao\-1.5\-vision\-pro及以后模型支持 |
|HEIF |.heif |`image/heif`<br><br>> doubao\-1.5\-vision\-pro及以后模型支持 |


<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>



* <div data-tips="true" data-tips-type="tip">上传文件至对象存储时设置，详情请参见<a href="https://www.volcengine.com/docs/6349/145523#%E8%AE%BE%E7%BD%AE%E6%96%87%E4%BB%B6%E5%85%83%E6%95%B0%E6%8D%AE">文档</a>。</div>


* <div data-tips="true" data-tips-type="tip">传入 Base64编码时使用：<a href="https://www.volcengine.com/docs/82379/1362931#f6222fec">Base64 编码输入</a>。</div>


* <div data-tips="true" data-tips-type="tip">图片格式需小写。</div>


* <div data-tips="true" data-tips-type="tip">TIFF、 SGI、ICNS、JPEG2000 几种格式图片，需保证和元数据对齐，如在对象存储中正确设置文件元数据，否则会解析失败，详细请参见 <a href="https://www.volcengine.com/docs/82379/1359411#effccb14">使用视觉理解模型时，报错InvalidParameter？</a></div>



<span id="c1f33d37"></span>
## API 参数字段说明

以下字段视觉理解暂不支持。


* 不支持设置频率惩罚系数，无 **frequency_penalty** 字段。

* 不支持设置存在惩罚系数，**presence_penalty** 字段。

* 不支持为单个请求生成多个返回，无 **n** 字段。


<span id="b867b8aa"></span>
# 常见问题


* [使用视觉理解模型时，报错InvalidParameter？](https://www.volcengine.com/docs/82379/1359411#effccb14)
