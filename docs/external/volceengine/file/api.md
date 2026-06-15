Files API 作为文件管理接口，提供文件的上传、检索、列表查询及删除能力。

在多模态理解场景中，Files API 与 Responses API、Chat API 结合使用，具备以下优势：


* 大文件适配：文件存储在方舟平台托管的默认存储空间中时，支持最大 512 MB 文件的上传；文件存储在用户指定的火山引擎对象存储 TOS Bucket 中时，支持最大 2 GB 的视频文件的上传，从而满足大文件处理需求。

* 重复使用：支持通过 File ID 在多次请求中重复使用文件，避免重复上传，节省公网下载时延。

* 缩短推理时长：解耦数据预处理与模型推理环节，避免每次请求时重新上传内容，减少预处理导致的时延。


<span id="62e9d75a"></span>
# 前提条件

[获取 API Key](https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey)

<span id="c4765823"></span>
# 查看 API 文档

[Files API 参考](https://www.volcengine.com/docs/82379/1870405)

<span id="821e2a5c"></span>
# Files API 使用示例

<span id="963e0807"></span>
## 上传文件

使用 Files API 可以上传图片、视频、音频、文档等类型的文件，上传成功后将返回 File ID。File ID 支持在多次请求中重复使用，而不需要重新上传内容，节省公网下载时延。如果文件大于 50 MB，或者要在多个请求中重复使用该文件，可以使用 Files API 上传文件，然后使用 File ID 发起请求。

文件存储位置由请求中 `tos` 参数决定，详情可参见[上传文件](https://www.volcengine.com/docs/82379/1870405?lang=zh)字段说明：


* 未传入 `tos` 参数：文件存储至方舟平台托管的默认存储空间。

* 传入 `tos` 参数：文件存储至用户指定的火山引擎对象存储 TOS Bucket。

> 目标存储的火山引擎对象存储 TOS Bucket 需要在控制台完成授权后才能使用，具体参考[用户对象存储（TOS）授权](https://www.volcengine.com/docs/82379/1529797#4eb1b277)。


<span id=".5paH5Lu25LiK5Lyg5pa55byP"></span>
### 文件上传方式

Files API 支持二进制文件、文件 URL 两种方式上传文件。


* `file`（二进制文件）：要上传的本地文件。文件可存储至方舟平台默认托管存储空间，或用户指定的火山引擎 TOS 存储桶。

* `url`（文件 URL）：支持 HTTP/HTTPS URL、TOS URI 两种方式。

   * HTTP/HTTPS URL：公网可直接访问的文件 URL，请确保文件 URL 可被访问。文件可存储至方舟平台默认托管存储空间，或用户指定的火山引擎 TOS 存储桶。

   * TOS URI：对象存储 TOS 专属的资源定位标识，格式为 `tos://<bucket>/<prefix>/<file_name>`。使用该方式时，文件仅支持存储至火山引擎 TOS 存储桶。


<span id=".ZmlsZS3kuIrkvKDmlrnlvI8="></span>
#### file 上传方式


* 文件存储至方舟平台默认托管存储空间



<Tabs>
<Tab zoneid="lwgrUQNwbE" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/files \
-H "Authorization: Bearer $ARK_API_KEY" \
-F 'purpose=user_data' \
-F 'file=@/Users/doc/demo.mp4' \
-F 'preprocess_configs[video][fps]=0.3'
```


响应参数如下：

```Bash
{
    "object": "file",
    "id": "file-20251018114827-6zgrb",
    "purpose": "user_data",
    "filename": "demo.mp4",
    "bytes": 695110,
    "mime_type": "video/mp4",
    "created_at": 1760759307,
    "expire_at": 1761364107,
    "status": "processing",
    "preprocess_configs": {
        "video": {
            "fps": 0.3
        }
    }
}
```



</Tab>
<Tab zoneid="ZQEi0OGWKo" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    # replace with your local video path
    file=open("/Users/doc/demo.mp4", "rb"),
    purpose="user_data",
    preprocess_configs={
        "video": {
            "fps": 0.3,  # define the sampling fps of the video, default is 1.0
        }
    }
)
print(file)
```



</Tab>
<Tab zoneid="TL8GjbLs2I" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    data, err := os.Open("/Users/doc/demo.mp4")
    if err != nil {
        fmt.Printf("read file error: %v\n", err)
        return
    }
    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        File:    data,
        Purpose: file.PurposeUserData,
        PreprocessConfigs: &file.PreprocessConfigs{
            Video: &file.Video{
                Fps: volcengine.Float64(0.3),
            },
        },
    })

    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)

}
```



</Tab>
<Tab zoneid="GqPa97cl6m" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.FileMeta;
import com.volcengine.ark.runtime.model.files.PreprocessConfigs;
import com.volcengine.ark.runtime.model.files.UploadFileRequest;
import com.volcengine.ark.runtime.model.files.Video;
import com.volcengine.ark.runtime.service.ArkService;
import java.io.File;

public class demo {

    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        System.out.println("===== Upload File Example=====");
        FileMeta fileMeta;
        fileMeta = service.uploadFile(
                UploadFileRequest.builder().
                        file(new File("/Users/doc/demo.mp4")) // replace with your image file path
                        .purpose("user_data")
                        .preprocessConfigs(PreprocessConfigs.builder().video(new Video(0.3)).build())
                        .build());
        System.out.println("Uploaded file Meta: " + fileMeta);

        service.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="G0MQrpNRSV" title="兼容 OpenAI SDK">
<TabTitle>兼容 OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    # replace with your local video path
    file=open("/Users/doc/demo.mp4", "rb"),
    purpose="user_data",
    extra_body={
        "preprocess_configs":{
            "video": {
                "fps": 0.3
            }
        }
    }
)
print(file)
```



</Tab>
</Tabs>



* 文件存储至火山引擎对象存储 TOS Bucket：



<Tabs>
<Tab zoneid="C36O6JiWcP" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/files \
-H "Authorization: Bearer $ARK_API_KEY" \
-F 'purpose=user_data' \
-F 'file=@/Users/doc/demo.mp4' \
-F 'preprocess_configs[video][fps]=0.3' \
-F "tos[bucket]=my-bucket" \
-F "tos[prefix]=ark-files/"
```



</Tab>
<Tab zoneid="fxilQD7luK" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    file=open("/Users/doc/demo.mp4", "rb"),
    purpose="user_data",
    preprocess_configs={
        "video": {
            "fps": 0.3
        }
    },
    tos={
        "bucket": "my-bucket",
        "prefix": "ark-files/"
    }
)
print(file)
```



</Tab>
<Tab zoneid="Zr7jSMfBtW" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    data, err := os.Open("/Users/doc/demo.mp4")
    if err != nil {
        fmt.Printf("read file error: %v\n", err)
        return
    }

    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        File:    data,
        Purpose: file.PurposeUserData,
        Tos: &file.TosStorage{
            Bucket: volcengine.String("my-bucket"),
            Prefix: volcengine.String("ark-files/"),
        },
        PreprocessConfigs: &file.PreprocessConfigs{
            Video: &file.Video{
                Fps: volcengine.Float64(0.3),
            },
        },
    })
    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)
}
```



</Tab>
<Tab zoneid="QgWwSNcBiD" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.*;
import com.volcengine.ark.runtime.service.ArkService;
import java.io.File;

public class demo {

    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder()
                .apiKey(apiKey)
                .baseUrl("https://ark.cn-beijing.volces.com/api/v3")
                .build();

        System.out.println("===== Upload File Example =====");
        FileMeta fileMeta;
        fileMeta = service.uploadFile(
                UploadFileRequest.builder()
                        .file(new File("/Users/doc/demo.mp4"))
                        .purpose("user_data")
                        .tos(TosStorage.builder()
                                .bucket("my-bucket")
                                .prefix("ark-files/")
                                .build())
                        .preprocessConfigs(PreprocessConfigs.builder()
                                .video(new Video(0.3))
                                .build())
                        .build());
        System.out.println("Uploaded file Meta: " + fileMeta);

        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>


<span id=".aHR0cC1odHRwcy11cmwt5LiK5Lyg5pa55byP"></span>
#### HTTP/HTTPS URL 上传方式


* 文件存储至方舟平台默认托管存储空间



<Tabs>
<Tab zoneid="DxKPhcDOZ6" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl -X POST "https://ark.cn-beijing.volces.com/api/v3/files" \
  -H "Authorization: Bearer $ARK_API_KEY" \
  -F "purpose=user_data" \
  -F "url=https://example.com/docs/demo_img.png"
```



</Tab>
<Tab zoneid="jUxYTomtOT" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    purpose="user_data",
    url="https://example.com/docs/demo_img.png"
)
print(file)

```



</Tab>
<Tab zoneid="iY1cwIg8RQ" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        Purpose: file.PurposeUserData,
        URL:     volcengine.String("https://example.com/docs/demo_img.png"),
    })

    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)
}

```



</Tab>
<Tab zoneid="icQRtKICdi" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.*;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder()
                .apiKey(apiKey)
                .baseUrl("https://ark.cn-beijing.volces.com/api/v3")
                .build();

        System.out.println("===== Upload File Example =====");
        FileMeta fileMeta;
        try {
            fileMeta = service.uploadFile(
                    UploadFileRequest.builder()
                            .url("https://example.com/docs/demo_img.png")
                            .purpose("user_data")
                            .build());
            System.out.println("Uploaded file Meta: " + fileMeta);
        } catch (Exception e) {
            e.printStackTrace();
        }

        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>



* 文件存储至火山引擎对象存储 TOS Bucket



<Tabs>
<Tab zoneid="OnvxeFGbbC" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl -X POST "https://ark.cn-beijing.volces.com/api/v3/files" \
  -H "Authorization: Bearer $ARK_API_KEY" \
  -F "purpose=user_data" \
  -F "url=https://example.com/docs/demo_img.png" \
  -F "tos[bucket]=my-bucket" \
  -F "tos[prefix]=ark-files/"
```



</Tab>
<Tab zoneid="vhlf4ZoVUZ" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    url="https://example.com/docs/demo_img.png",
    purpose="user_data",
    tos={
        "bucket": "my-bucket",
        "prefix": "ark-files/"
    }
)
print(file)
```



</Tab>
<Tab zoneid="ph8Gam1tf5" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        Purpose: file.PurposeUserData,
        URL:     volcengine.String("https://example.com/docs/demo_img.png"),
        Tos: &file.TosStorage{
            Bucket: volcengine.String("my-bucket"),
            Prefix: volcengine.String("ark-files/"),
        },
    })
    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)
}
```



</Tab>
<Tab zoneid="gTNu1ptGYa" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.*;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {

    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder()
                .apiKey(apiKey)
                .baseUrl("https://ark.cn-beijing.volces.com/api/v3")
                .build();

        System.out.println("===== Upload File Example =====");
        FileMeta fileMeta;
        try {
            fileMeta = service.uploadFile(
                    UploadFileRequest.builder()
                            .url("https://example.com/docs/demo_img.png")
                            .purpose("user_data")
                            .tos(TosStorage.builder()
                                    .bucket("my-bucket")
                                    .prefix("ark-files/")
                                    .build())
                            .build());
            System.out.println("Uploaded file Meta: " + fileMeta);
        } catch (Exception e) {
            e.printStackTrace();
        }

        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>


<span id=".dG9zLXVyaS3kuIrkvKDmlrnlvI8="></span>
#### TOS URI 上传方式

传入 TOS URI 时，必须将文件存储到火山引擎对象存储 TOS Bucket 中。其中：


* `url` 用于指定待读取的源文件位置，即火山引擎对象存储 TOS Bucket 中的已有文件。

* `tos[bucket]` 和 `tos[prefix]` 用于指定上传文件至火山引擎对象存储 TOS Bucket 中的存储桶和存储路径。



<Tabs>
<Tab zoneid="ZEhNSlji6A" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl -X POST "https://ark.cn-beijing.volces.com/api/v3/files" \
  -H "Authorization: Bearer $ARK_API_KEY" \
  -F "purpose=user_data" \
  -F "url=tos://my-bucket/source/raw_video.mp4" \
  -F "tos[bucket]=my-bucket" \
  -F "tos[prefix]=ark-files/"
```



</Tab>
<Tab zoneid="A89B8TrF3x" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    url="tos://my-bucket/source/raw_video.mp4",
    purpose="user_data",
    tos={
        "bucket": "my-bucket",
        "prefix": "ark-files/"
    }
)
print(file)
```



</Tab>
<Tab zoneid="qdXN22XVbo" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        Purpose: file.PurposeUserData,
        URL:     volcengine.String("tos://my-bucket/source/raw_video.mp4"),
        Tos: &file.TosStorage{
            Bucket: volcengine.String("my-bucket"),
            Prefix: volcengine.String("ark-files/"),
        },
    })
    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)
}
```



</Tab>
<Tab zoneid="wer7ulQR1k" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.*;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {

    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder()
                .apiKey(apiKey)
                .baseUrl("https://ark.cn-beijing.volces.com/api/v3")
                .build();

        System.out.println("===== Upload File Example =====");
        FileMeta fileMeta;
        try {
            fileMeta = service.uploadFile(
                    UploadFileRequest.builder()
                            .url("tos://my-bucket/source/raw_video.mp4")
                            .purpose("user_data")
                            .tos(TosStorage.builder()
                                    .bucket("my-bucket")
                                    .prefix("ark-files/")
                                    .build())
                            .build());
            System.out.println("Uploaded file Meta: " + fileMeta);
        } catch (Exception e) {
            e.printStackTrace();
        }

        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>


在大视频理解场景下，通过 Files API 以 `url` 方式上传，并传入 `tos` 参数时，视频文件单文件最大支持 2GB （该能力仅适用于视频文件，其他类型文件容量限制仍保持 512 MB）。文件上传后会进入异步处理流程，需待文件状态变为 active 后方可使用。


<Tabs>
<Tab zoneid="uT2kirQXk7" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl -X POST "https://ark.cn-beijing.volces.com/api/v3/files" \
  -H "Authorization: Bearer $ARK_API_KEY" \
  -F "purpose=user_data" \
  -F "url=tos://my-bucket/videos/long-video.mp4" \
  -F "tos[bucket]=my-bucket" \
  -F "tos[prefix]=ark-files/" \
  -F "preprocess_configs[video][max_video_tokens]=200000" \
  -F "preprocess_configs[video][min_frames]=16"
```



</Tab>
<Tab zoneid="U11VTfsDer" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

file = client.files.create(
    url="tos://my-bucket/videos/long-video.mp4",
    purpose="user_data",
    tos={
        "bucket": "my-bucket",
        "prefix": "ark-files/"
    },
    preprocess_configs={
        "video": {
            "max_video_tokens": 200000,
            "min_frames": 16
        }
    }
)
print(file)
```



</Tab>
<Tab zoneid="DydJO4SjN7" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
    "github.com/volcengine/volcengine-go-sdk/volcengine"
)

func main() {
    client := arkruntime.NewClientWithApiKey(
        os.Getenv("ARK_API_KEY"),
        arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),
    )
    ctx := context.Background()

    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        Purpose: file.PurposeUserData,
        URL:     volcengine.String("tos://my-bucket/videos/long-video.mp4"),
        Tos: &file.TosStorage{
            Bucket: volcengine.String("my-bucket"),
            Prefix: volcengine.String("ark-files/"),
        },
        PreprocessConfigs: &file.PreprocessConfigs{
            Video: &file.Video{
                MaxVideoTokens: volcengine.Int64(200000),
                MinFrames:      volcengine.Int64(16),
            },
        },
    })
    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }
    fmt.Printf("file info: %v\n", fileInfo)
}
```



</Tab>
<Tab zoneid="TBS6vC6bdm" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.*;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {

    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder()
                .apiKey(apiKey)
                .baseUrl("https://ark.cn-beijing.volces.com/api/v3")
                .build();

        System.out.println("===== Upload File Example =====");
        FileMeta fileMeta;
        try {
            fileMeta = service.uploadFile(
                    UploadFileRequest.builder()
                            .url("tos://my-bucket/videos/long-video.mp4")
                            .purpose("user_data")
                            .tos(TosStorage.builder()
                                    .bucket("my-bucket")
                                    .prefix("ark-files/")
                                    .build())
                            .preprocessConfigs(PreprocessConfigs.builder()
                                    .video(Video.builder()
                                            .maxVideoTokens(200000L)
                                            .minFrames(16L)
                                            .build())
                                    .build())
                            .build());
            System.out.println("Uploaded file Meta: " + fileMeta);
        } catch (Exception e) {
            e.printStackTrace();
        }

        service.shutdownExecutor();
    }
}
```



</Tab>
</Tabs>


<span id="d75377d6"></span>
### 文件存储限制


<span aceTableMode="list" aceTableWidth="1,2,2"></span>
|对比项 |方舟平台托管的默认存储空间 |火山引擎对象存储 TOS Bucket |
|---|---|---|
|授权要求 |无需额外授权。 |目标存储 Bucket 需要在控制台完成授权后才能使用，参考[用户对象存储（TOS）授权](https://www.volcengine.com/docs/82379/1529797#4eb1b277)。 |
|单文件大小 |512 MB |视频文件 2 GB，其他类型文件 512 MB |
|总存储容量 |20 GB |无限制 |
|存储时间 |默认存储7天，支持通过 **expire_at** 参数自定义存储有效期，取值范围为1\-30天。<br><br>> 针对高频存储场景，建议通过缩短文件存储时长、主动调用删除接口清理低频文件这两种方式，做好存储空间的主动管理。 |默认存储7天，支持通过 **expire_at** 参数自定义存储有效期，取值范围为1\-30天。 |
|对象操作限制 |如需删除文件，请调用 Files API 删除，参考[删除文件](https://docs.volcengine.com/docs/82379/1870408?lang=zh)。 |对象被托管后，仅支持读取，不支持通过 TOS 控制台或 TOS API 删除、覆盖或修改对象。命中该对象的生命周期删除、覆盖式复制等异步写、删、改操作，同样受到托管保护限制。如需删除文件，请调用 Files API 删除。参考[删除文件](https://docs.volcengine.com/docs/82379/1870408?lang=zh)。 |


<span id="fd98059d"></span>
### 文件预处理

使用 Files API 上传文件时，接口会根据上传的文件类型进行预处理。


* 视频文件：默认会按1帧/秒（FPS）的速率提取选段，可通过 **preprocess_configs.video.fps** 设置自定义帧速率。长视频且画面变化较小时，可设置较低的 FPS 值；需精细捕捉画面变化时，可设置较高的 FPS 值。同时可通过`min_frame_tokens`、`max_frame_tokens`、`max_video_tokens` 用于控制单帧压缩与整体视频信息保留策略。文件预处理后，在 Responses API 中使用 File ID，可以缩短推理时长。

* PDF文件：会分页来处理成多图，在预处理时不会对拆分的图片做分辨率缩放，以确保图片能够完整且清晰地保留 PDF 文件中的原始信息。


<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">传入 <code>tos</code> 参数时，文件预处理产物将保存至参数指定的 TOS 路径。例如，若请求中配置 <code>tos[bucket]=my-bucket</code>、<code>tos[prefix]=ark-files/</code>，则预处理产物存储路径为 <code>my-bucket/ark-files/ark_processed/{file_id}/</code> 目录下，其中 <code>{file_id}</code> 为本次上传文件的 ID。</div>


<span id="82fa7a9c"></span>
### 预处理超时限制

使用 Files API 进行文件预处理的超时限制为 5 min，超时通常会受视频时长、PDF 页数、单页像素、单帧像素、音频时长等因素影响。

**超时解决方案**：

优先检查是否存在像素过大的问题，其中对 1080p 视频抽帧操作容易导致超时，建议压缩至 720p 及以下。

> 模型推理阶段会压缩分辨率，所以提升原始像素对最终效果无增益。


**视频压缩工具及命令**

将视频文件压缩至 720p 的命令示例如下。FFmpeg 工具下载可参见[下载FFmpeg](http://ffmpeg.org/download.html)。

```Bash
ffmpeg -i input.mp4 \
  -vf "scale=1280:720" \
  -c:v libx264 -crf 23 \
  -c:a aac -b:a 128k \
  output_720p.mp4
```


<span id="81920512"></span>
### 文件类型

Files API 支持多种文件类型，具体如下。


|文件类型 |文件格式 |MIME类型 |
|---|---|---|
|图片 |.jpg、.jpeg、.png、.gif、.webp、.bmp、.tiff、.ico、.icns、.sgi、.jp2、.heic、.heif |`image/jpeg`、`image/png`、`image/gif`、`image/webp`、`image/bmp`、`image/tiff`、`image/x-icon`、`image/icns`、`image/sgi`、`image/jp2`、`image/heic`、`image/heif` |
|视频 |.mp4、.avi、.mov |`video/mp4`、`video/avi`、`video/mov` |
|文档 |.pdf |`application/pdf` |
|音频 |.mp3、.wav、.aac、.m4a |`audio/mpeg`、`audio/wav`、`audio/aac`、`audio/m4a` |


<div data-tips="true" data-tips-type="tip" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="tip">常见问题及解决方案参见<a href="https://www.volcengine.com/docs/82379/1359411#85251eec">支持 TS 格式的视频文件吗？</a></div>


<span id="91473606"></span>
## 管理文件

<span id=".5qOA57Si5paH5Lu2"></span>
### 检索文件

通过 File ID 检索文件信息，如文件大小、过期时间、MIME类型及文件处理状态等信息，可参见[检索文件](https://www.volcengine.com/docs/82379/1870406?lang=zh)接口说明。

> 文件上传成功后将自动触发预处理流程。接口返回 `file_id` 后，可通过调用检索文件接口查询文件处理状态；当状态 `status` 变为 `active` 时，才可以在 Responses API、Chat API 中通过 `file_id` 方式实现多模态理解。



<Tabs>
<Tab zoneid="rhTESOWoqW" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/files/file-20251014**** \
-H "Authorization: Bearer $ARK_API_KEY"
```



</Tab>
<Tab zoneid="ec0AYJyG9M" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

# Get API Key：https://console.volcengine.com/ark/region:ark+cn-beijing/apikey
api_key = os.getenv('ARK_API_KEY')

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

# Retrieve file
response = client.files.retrieve(
    file_id="file-2025******"
)

print(response)
```



</Tab>
<Tab zoneid="GoJQUprhtn" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
)

func main() {
    client := arkruntime.NewClientWithApiKey(os.Getenv("ARK_API_KEY"),arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"))
    ctx := context.Background()

    fileInfo, err := client.RetrieveFile(ctx, "file-20251114****") // update file info
    if err != nil {
        fmt.Printf("get file status error: %v", err)
        return
    }
    fmt.Printf("file info: %v", fileInfo)

}
```



</Tab>
<Tab zoneid="KR5oXCF5cl" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.FileMeta;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        // Retrieve file
        FileMeta fileMeta = service.retrieveFile("file-20251117****");
        System.out.println("Retrieve File:" + fileMeta);

        service.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="zaJYbuKsXT" title="兼容 OpenAI SDK">
<TabTitle>兼容 OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI

api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

response = client.files.retrieve(
    file_id="file-20251117****"
)

print(response)
```



</Tab>
</Tabs>


<span id="34f747b5"></span>
### 查询文件列表

通过 Files API 查询已上传的文件列表，可参考[查询文件列表](https://www.volcengine.com/docs/82379/1870407?lang=zh)接口说明。


<Tabs>
<Tab zoneid="xr557lO7ph" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/files \
-H "Authorization: Bearer $ARK_API_KEY"
```



</Tab>
<Tab zoneid="hLxYrx7S1h" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

api_key = os.getenv('ARK_API_KEY')

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

response = client.files.list()

print(response)
```



</Tab>
<Tab zoneid="grvAs0cN5n" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
)

func main() {
    client := arkruntime.NewClientWithApiKey(os.Getenv("ARK_API_KEY"),arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),)
    ctx := context.Background()

    fileInfo, err := client.ListFiles(ctx, &file.ListFilesRequest{}) 
    if err != nil {
        fmt.Printf("get file List error: %v", err)
        return
    }
    fmt.Printf("file List: %v", fileInfo)
}
```



</Tab>
<Tab zoneid="jJosiFfzaK" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.ListFilesResponse;
import com.volcengine.ark.runtime.model.files.ListFilesRequest;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        ListFilesRequest request = new ListFilesRequest();
        ListFilesResponse ListFiles = service.listFiles(request);
        System.out.println("List Files:" + ListFiles);

        service.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="cu6Zu7DXEx" title="兼容 OpenAI SDK">
<TabTitle>兼容 OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI

api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

response = client.files.list()

print(response)
```



</Tab>
</Tabs>


<span id="9eb4f3d2"></span>
### 删除文件

删除文件接口用于根据文件 ID 删除文件，并将文件从存储空间中移除，可参考[删除文件](https://docs.volcengine.com/docs/82379/1870408?lang=zh)接口说明。

针对高频存储场景，做好存储空间管理的方式如下。存储限制，具体参见[文件存储限制](https://www.volcengine.com/docs/82379/1885708#d75377d6)。


* 缩短文件存储时长：上传成功的文件默认存储7天，可以通过 **expire_at** 参数自定义存储有效期，取值范围为1\-30天。文件超过存储有效期后会自动删除，参数设置请参见[上传文件](https://www.volcengine.com/docs/82379/1870405)。

* 主动调用删除接口清理低频文件：通过 Files API 删除已上传的文件，使用示例如下。



<Tabs>
<Tab zoneid="CEwMhFCg1A" title="Curl">
<TabTitle>Curl</TabTitle>

```Bash
curl https://ark.cn-beijing.volces.com/api/v3/files/file-20251014**** \
-X DELETE \
-H "Authorization: Bearer $ARK_API_KEY"
```



</Tab>
<Tab zoneid="ofFENfG3qv" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import os
from volcenginesdkarkruntime import Ark

api_key = os.getenv('ARK_API_KEY')

client = Ark(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

if __name__ == "__main__":
    try:
        client.files.delete(
            file_id="file-20251014****"
        )
    except Exception as e:
        print(f"failed to delete response: {e}")
```



</Tab>
<Tab zoneid="GtT1vMHxJ8" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
)

func main() {
    client := arkruntime.NewClientWithApiKey(os.Getenv("ARK_API_KEY"),arkruntime.WithBaseUrl("https://ark.cn-beijing.volces.com/api/v3"),)
    ctx := context.Background()

    fileInfo, err := client.DeleteFile(ctx, "file-20251114****") 
    if err != nil {
        fmt.Printf("delete file error: %v", err)
        return
    }
    fmt.Printf(" delete file: %v", fileInfo)
}
```



</Tab>
<Tab zoneid="GdBM0RXLoy" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.DeleteFileResponse;
import com.volcengine.ark.runtime.service.ArkService;

public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        // delete file
        DeleteFileResponse deleteFile = service.deleteFile("file-20251117****");
        System.out.println("Delete File:" + deleteFile);

        service.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="mdaikWm6pk" title="兼容 OpenAI SDK">
<TabTitle>兼容 OpenAI SDK</TabTitle>

```Python
import os
from openai import OpenAI

api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

if __name__ == "__main__":
    try:
        response = client.files.delete(
            file_id="file-20251119****"
        )
        print(response)
    except Exception as e:
        print(f"failed to delete response: {e}")
```



</Tab>
</Tabs>


<span id="8a45d4bd"></span>
# 使用 File ID 实现多模态理解

针对文件较大或需在多次请求中重复使用该文件的场景，建议通过 Files API 上传文件，然后在 Responses API、Chat API 中使用 File ID 的方式实现多模态理解。具体示例参见 [视频理解](https://www.volcengine.com/docs/82379/1958521#098ef3d4)、[图片理解](https://www.volcengine.com/docs/82379/1958521#70e09284)、[文档理解](https://www.volcengine.com/docs/82379/1958521#18a762a5)、[音频理解](https://www.volcengine.com/docs/82379/2377589)。

上传文件后，需等待文件处理完成后（即 **status** 为 active 时）才能在 Responses API、Chat API 中使用对应的 File ID 进行分析。下面是视频理解的示例代码。


<Tabs>
<Tab zoneid="z6pbe6SIg2" title="Curl">
<TabTitle>Curl</TabTitle>

1. 上传视频文件获取File ID。

   ```Bash
   curl https://ark.cn-beijing.volces.com/api/v3/files \
   -H "Authorization: Bearer $ARK_API_KEY" \
   -F 'purpose=user_data' \
   -F 'file=@/Users/doc/demo.mp4' \
   -F 'preprocess_configs[video][fps]=0.3'
   ```
   

2. 在Responses API中引用File ID。

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
                       "type": "input_file",
                       "file_id": "file-20251018****"
                   },
                   {
                       "type": "input_text",
                   "text": "请你描述下视频中的人物的一系列动作，以JSON格式输出开始时间（start_time）、结束时间（end_time）、事件（event）、是否危险（danger），请使用HH:mm:ss表示时间戳。"
                   }
               ]
           }
       ]
   }'
   ```
   


</Tab>
<Tab zoneid="Uk7URdxpyp" title="Python SDK">
<TabTitle>Python SDK</TabTitle>

```Python
import asyncio
import os
from volcenginesdkarkruntime import AsyncArk
from volcenginesdkarkruntime.types.responses.response_completed_event import ResponseCompletedEvent
from volcenginesdkarkruntime.types.responses.response_reasoning_summary_text_delta_event import ResponseReasoningSummaryTextDeltaEvent
from volcenginesdkarkruntime.types.responses.response_output_item_added_event import ResponseOutputItemAddedEvent
from volcenginesdkarkruntime.types.responses.response_text_delta_event import ResponseTextDeltaEvent
from volcenginesdkarkruntime.types.responses.response_text_done_event import ResponseTextDoneEvent

client = AsyncArk(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=os.getenv('ARK_API_KEY')
)

async def main():
    # upload video file
    print("Upload video file")
    file = await client.files.create(
        # replace with your local video path
        file=open("/Users/doc/demo.mp4", "rb"),
        purpose="user_data",
        preprocess_configs={
            "video": {
                "fps": 0.3,  # define the sampling fps of the video, default is 1.0
            }
        }
    )
    print(f"File uploaded: {file.id}")

    # Wait for the file to finish processing
    await client.files.wait_for_processing(file.id)
    print(f"File processed: {file.id}")

    stream = await client.responses.create(
        model="doubao-seed-2-0-lite-260215",
        input=[
            {"role": "user", "content": [
                {
                    "type": "input_video",
                    "file_id": file.id  # ref video file id
                },
                {
                    "type": "input_text",
                    "text": "请你描述下视频中的人物的一系列动作，以JSON格式输出开始时间（start_time）、结束时间（end_time）、事件（event）、是否危险（danger），请使用HH:mm:ss表示时间戳。"
                }
            ]},
        ],
        caching={
            "type": "enabled",
        },
        store=True,
        stream=True
    )
    
    async for event in stream:
        if isinstance(event, ResponseReasoningSummaryTextDeltaEvent):
            print(event.delta, end="")
        if isinstance(event, ResponseOutputItemAddedEvent):
            print("\noutPutItem " + event.type + " start:")
        if isinstance(event, ResponseTextDeltaEvent):
            print(event.delta,end="")
        if isinstance(event, ResponseTextDoneEvent):
            print("\noutPutTextDone.")
        if isinstance(event, ResponseCompletedEvent):
            print("Response Completed. Usage = " + event.response.usage.model_dump_json())

if __name__ == "__main__":
    asyncio.run(main())
```



</Tab>
<Tab zoneid="fwMRisVNVO" title="Go SDK">
<TabTitle>Go SDK</TabTitle>

```Go
package main

import (
    "context"
    "fmt"
    "io"
    "os"
    "time"

    "github.com/volcengine/volcengine-go-sdk/service/arkruntime"
    "github.com/volcengine/volcengine-go-sdk/service/arkruntime/model/file"
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

    fmt.Println("----- upload video data -----")
    data, err := os.Open("/Users/doc/demo.mp4")
    if err != nil {
        fmt.Printf("read file error: %v\n", err)
        return
    }
    fileInfo, err := client.UploadFile(ctx, &file.UploadFileRequest{
        File:    data,
        Purpose: file.PurposeUserData,
        PreprocessConfigs: &file.PreprocessConfigs{
            Video: &file.Video{
                Fps: volcengine.Float64(0.3),
            },
        },
    })

    if err != nil {
        fmt.Printf("upload file error: %v", err)
        return
    }

    // Wait for the file to finish processing
    for fileInfo.Status == file.StatusProcessing {
        fmt.Println("Waiting for video to be processed...")
        time.Sleep(2 * time.Second)
        fileInfo, err = client.RetrieveFile(ctx, fileInfo.ID) // update file info
        if err != nil {
            fmt.Printf("get file status error: %v", err)
            return
        }
    }
    fmt.Printf("Video processing completed: %s, status: %s\n", fileInfo.ID, fileInfo.Status)
    inputMessage := &responses.ItemInputMessage{
        Role: responses.MessageRole_user,
        Content: []*responses.ContentItem{
            {
                Union: &responses.ContentItem_Video{
                    Video: &responses.ContentItemVideo{
                        Type:   responses.ContentItemType_input_video,
                        FileId: volcengine.String(fileInfo.ID),
                    },
                },
            },
            {
                Union: &responses.ContentItem_Text{
                    Text: &responses.ContentItemText{
                        Type: responses.ContentItemType_input_text,
                        Text: "请你描述下视频中的人物的一系列动作，以JSON格式输出开始时间（start_time）、结束时间（end_time）、事件（event）、是否危险（danger），请使用HH:mm:ss表示时间戳。",
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
        Caching: &responses.ResponsesCaching{Type: responses.CacheType_enabled.Enum()},
    }

    resp, err := client.CreateResponsesStream(ctx, createResponsesReq)
    if err != nil {
        fmt.Printf("stream error: %v\n", err)
        return
    }
    var responseId string
    for {
        event, err := resp.Recv()
        if err == io.EOF {
            break
        }
        if err != nil {
            fmt.Printf("stream error: %v\n", err)
            return
        }
        handleEvent(event)
        if responseEvent := event.GetResponse(); responseEvent != nil {
            responseId = responseEvent.GetResponse().GetId()
            fmt.Printf("Response ID: %s", responseId)
        }
    }
}

func handleEvent(event *responses.Event) {
    switch event.GetEventType() {
    case responses.EventType_response_reasoning_summary_text_delta.String():
        print(event.GetReasoningText().GetDelta())
    case responses.EventType_response_reasoning_summary_text_done.String(): // aggregated reasoning text
        fmt.Printf("\nAggregated reasoning text: %s\n", event.GetReasoningText().GetText())
    case responses.EventType_response_output_text_delta.String():
        print(event.GetText().GetDelta())
    case responses.EventType_response_output_text_done.String(): // aggregated output text
        fmt.Printf("\nAggregated output text: %s\n", event.GetTextDone().GetText())
    default:
        return
    }
}
```



</Tab>
<Tab zoneid="l21ckflC96" title="Java SDK">
<TabTitle>Java SDK</TabTitle>

```Java
package com.ark.sample;

import com.volcengine.ark.runtime.model.files.FileMeta;
import com.volcengine.ark.runtime.model.files.PreprocessConfigs;
import com.volcengine.ark.runtime.model.files.UploadFileRequest;
import com.volcengine.ark.runtime.model.files.Video;
import com.volcengine.ark.runtime.service.ArkService;
import com.volcengine.ark.runtime.model.responses.request.*;
import com.volcengine.ark.runtime.model.responses.item.ItemEasyMessage;
import com.volcengine.ark.runtime.model.responses.constant.ResponsesConstants;
import com.volcengine.ark.runtime.model.responses.item.MessageContent;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemVideo;
import com.volcengine.ark.runtime.model.responses.content.InputContentItemText;

import com.volcengine.ark.runtime.model.responses.event.functioncall.FunctionCallArgumentsDoneEvent;
import com.volcengine.ark.runtime.model.responses.event.outputitem.OutputItemAddedEvent;
import com.volcengine.ark.runtime.model.responses.event.outputitem.OutputItemDoneEvent;
import com.volcengine.ark.runtime.model.responses.event.outputtext.OutputTextDeltaEvent;
import com.volcengine.ark.runtime.model.responses.event.outputtext.OutputTextDoneEvent;
import com.volcengine.ark.runtime.model.responses.event.reasoningsummary.ReasoningSummaryTextDeltaEvent;
import com.volcengine.ark.runtime.model.responses.event.response.ResponseCompletedEvent;
import java.io.File;
import java.util.concurrent.TimeUnit;

public class demo {
    public static void main(String[] args) {
        String apiKey = System.getenv("ARK_API_KEY");
        ArkService service = ArkService.builder().apiKey(apiKey).baseUrl("https://ark.cn-beijing.volces.com/api/v3").build();

        System.out.println("===== Upload File Example=====");
        // upload a video for responses
        FileMeta fileMeta;
        fileMeta = service.uploadFile(
                UploadFileRequest.builder().
                        file(new File("/Users/doc/demo.mp4")) // replace with your image file path
                        .purpose("user_data")
                        .preprocessConfigs(PreprocessConfigs.builder().video(new Video(0.3)).build())
                        .build());
        System.out.println("Uploaded file Meta: " + fileMeta);
        System.out.println("status:" + fileMeta.getStatus());

        try {
            while (fileMeta.getStatus().equals("processing")) {
                System.out.println("Waiting for video to be processed...");
                TimeUnit.SECONDS.sleep(2);
                fileMeta = service.retrieveFile(fileMeta.getId());
            }
        } catch (Exception e) {
            System.err.println("get file status error：" + e.getMessage());
        }
        System.out.println("Uploaded file Meta: " + fileMeta);

        CreateResponsesRequest request = CreateResponsesRequest.builder()
                .model("doubao-seed-2-0-lite-260215")
                .stream(true)
                .input(ResponsesInput.builder().addListItem(
                        ItemEasyMessage.builder().role(ResponsesConstants.MESSAGE_ROLE_USER).content(
                                MessageContent.builder()
                                        .addListItem(InputContentItemVideo.builder().fileId(fileMeta.getId()).build())
                                        .addListItem(InputContentItemText.builder().text("请你描述下视频中的人物的一系列动作，以JSON格式输出开始时间（start_time）、结束时间（end_time）、事件（event）、是否危险（danger），请使用HH:mm:ss表示时间戳。").build())
                                        .build()
                        ).build()
                ).build())
                .build();

        service.streamResponse(request)
                .doOnError(Throwable::printStackTrace)
                .blockingForEach(event -> {
                    if (event instanceof ReasoningSummaryTextDeltaEvent) {
                        System.out.print(((ReasoningSummaryTextDeltaEvent) event).getDelta());
                    }
                    if (event instanceof OutputItemAddedEvent) {
                        System.out.println("\nOutputItem " + (((OutputItemAddedEvent) event).getItem().getType()) + " Start: ");
                    }
                    if (event instanceof OutputTextDeltaEvent) {
                        System.out.print(((OutputTextDeltaEvent) event).getDelta());
                    }
                    if (event instanceof OutputTextDoneEvent) {
                        System.out.println("\nOutputText End.");
                    }
                    if (event instanceof OutputItemDoneEvent) {
                        System.out.println("\nOutputItem " + ((OutputItemDoneEvent) event).getItem().getType() + " End.");
                    }
                    if (event instanceof FunctionCallArgumentsDoneEvent) {
                        System.out.println("\nFunctionCall Arguments: " + ((FunctionCallArgumentsDoneEvent) event).getArguments());
                    }
                    if (event instanceof ResponseCompletedEvent) {
                        System.out.println("\nResponse Completed. Usage = " + ((ResponseCompletedEvent) event).getResponse().getUsage());
                    }
                });


        service.shutdownExecutor();
    }
}
```



</Tab>
<Tab zoneid="pA4j1bH9lL" title="兼容 OpenAI SDK">
<TabTitle>兼容 OpenAI SDK</TabTitle>

```Python
import os
import time
from openai import OpenAI

api_key = os.getenv('ARK_API_KEY')

client = OpenAI(
    base_url='https://ark.cn-beijing.volces.com/api/v3',
    api_key=api_key,
)

file = client.files.create(
    file=open("/Users/doc/demo.mp4", "rb"),
    purpose="user_data"
)
# Wait for the file to finish processing
while (file.status == "processing"):
    time.sleep(2)
    file = client.files.retrieve(file.id)
print(f"File processed: {file}")
    
response = client.responses.create(
    model="doubao-seed-2-0-lite-260215",
    input=[
        {
            "role": "user",
            "content": [
                {
                    "type": "input_video",
                    "file_id": file.id,
                },
                {
                    "type": "input_text",
                    "text": "请你描述下视频中的人物的一系列动作，以JSON格式输出开始时间（start_time）、结束时间（end_time）、事件（event）、是否危险（danger），请使用HH:mm:ss表示时间戳。",
                },
            ]
        }
    ],
    stream=True
)


for event in response:
    if event.type == "response.reasoning_summary_text.delta":
        print(event.delta, end="")
    if event.type == "response.output_item.added":
        print("\noutPutItem " + event.type + " start:")
    if event.type == "response.output_text.delta":
        print(event.delta,end="")
    if event.type == "response.output_item.done":
        print("\noutPutTextDone.")
    if event.type == "response.completed":
        print("\nResponse Completed. Usage = " + event.response.usage.model_dump_json())
```



</Tab>
</Tabs>


<span id="5a0c8d52"></span>
# 计费说明

Files API 提供的上传文件、管理文件等能力均不会产生费用。

上传的文件根据存储位置的不同，计费信息如下：


* 文件存储在方舟平台托管的默认存储空间：每个账户有 20 GB 免费存储额度，超出后无法上传文件，删除文件释放存储空间后可继续上传文件。

* 文件存储在用户指定的火山引擎对象存储 TOS Bucket：将产生存储容量、流量、请求次数、数据取回等费用，详细计费规则参考[对象托管](https://www.volcengine.com/docs/6349/2532373?lang=zh#Mv2UN2Bn)。

    &nbsp;


<span id=".5L2_55So6ZmQ5Yi25Y-K6ZSZ6K-v56CB"></span>
# 使用限制及错误码


* Files API QPS限流及带宽限制如下。

   * 上传文件：20 QPS、100 Mbps带宽

   * 检索文件：20 QPS

   * 查询文件列表：20 QPS

   * 删除文件：20 QPS

* 错误码：单击[错误码](https://www.volcengine.com/docs/82379/1299023?lang=zh)，获取相关信息。
