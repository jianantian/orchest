`POST https://ark.cn-beijing.volces.com/api/v3/files`

本文介绍使用 Files API 上传文件请求时的输入输出参数，供您使用接口时查阅字段含义。

<span id="LGrPmcsa"></span>
## 请求参数

> 跳转 [响应参数](https://www.volcengine.com/docs/82379/1870405#NtL4xXlS)


<span id="5Q8CpXQq"></span>
### 请求体


---



**file** `file` `条件必填`

需要上传的文件，要求为二进制文件。具体限制请参见 [Files API教程](https://www.volcengine.com/docs/82379/1885708)。

<div data-tips="true" data-tips-type="warning" data-tips-is-title="true">注意</div>


<div data-tips="true" data-tips-type="warning">文件上传支持二进制文件和文件URL两种传入方式，<strong>file</strong>、<strong>url</strong> 为二选一必填参数，不可同时传参。</div>



---



**purpose**  `string` `默认值：user_data` <span data-api-tag="require|zgNDAb">必选</span>

文件用途。

`user_data`：可以灵活使用的文件，能够用于任意用途。


---



**url ** `string` `条件必填`

文件 URL，支持以下两种 URL 类型：


* HTTP/HTTPS URL：公网可直接访问的文件 URL，请确保文件 URL 可被访问。

* TOS URI：对象存储 TOS 专属的资源定位标识，格式为`tos://<bucket>/<prefix>/<file_name>`。其中，<bucket\> 、 <prefix\> 和 <file_name\> 分别表示存储桶名称、存储路径和文件名。详细使用说明可参考 [TOS URI 上传方式](https://docs.volcengine.com/docs/82379/1885708?lang=zh#.dG9zLXVyaS3kuIrkvKDmlrnlvI8=) 部分，也可前往 [火山引擎对象存储 TOS Bucket ](https://console.volcengine.com/tos/bucket?projectName=default)中管理存储桶。


<div data-tips="true" data-tips-type="warning" data-tips-is-title="true">注意</div>



* <div data-tips="true" data-tips-type="warning">文件上传支持二进制文件和文件 URL 两种传入方式，<strong>file</strong>、<strong>url</strong> 为二选一必填参数，不可同时传参。</div>


* <div data-tips="true" data-tips-type="warning">传入 TOS URI 时，必须将文件存储到火山引擎对象存储 TOS Bucket 中，此时 <code>tos.bucket</code> 、<code>tos.prefix</code> 为必填参数。</div>




---



**tos** `object`

文件存储的 TOS 信息。TOS 即火山引擎对象存储服务；配置本参数后，上传文件将存储至用户指定的火山引擎对象存储 TOS Bucket 中，不再存入方舟平台默认托管存储空间。

<div data-tips="true" data-tips-type="default" data-tips-is-title="true">说明</div>



* <div data-tips="true" data-tips-type="default">不传 tos 参数：文件存储在方舟平台托管的默认存储空间，单文件不超过 512 MB。已存入方舟平台托管存储的历史文件，不会自动迁移至其他存储位置。</div>


* <div data-tips="true" data-tips-type="default">传 tos 参数：文件存储在用户指定的火山引擎对象存储 TOS Bucket 中，视频文件不超过 2 GB，其他类型文件不超过 512 MB，支持格式详见<a href="https://docs.volcengine.com/docs/82379/1885708?lang=zh#81920512">文件类型</a>。授权写入用户指定 TOS Bucket 中的文件，不支持在 TOS 侧删除或修改；如需删除文件，可调用 Files API 删除。参考<a href="https://docs.volcengine.com/docs/82379/1870408?lang=zh">删除文件</a>。</div>




属性


---



tos.**bucket** `string` `条件必填`

目标文件待写入的 TOS 存储桶名称，该存储桶需提前完成创建，参考[创建存储桶](https://www.volcengine.com/docs/6349/75024?lang=zh)。


---



tos.**prefix** `string` `条件必填`

文件在目标 TOS 存储桶内的存放前缀路径，入参填写相对路径格式，上传后文件将落地至该前缀目录下。示例：`arkfiles/`。



---



**preprocess_configs** `object / null` 

用于设置不同文件类型的预处理规则。


属性


---



preprocess_configs.**video** `object`

视频预处理相关配置。

<div data-tips="true" data-tips-type="default" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="default">配置 <code>model</code> 字段后，系统将根据对应模型的抽帧策略，自动填充 <code>max_video_tokens</code>、<code>min_frame_tokens</code>、<code>max_frame_tokens</code>、<code>min_frames</code> 四个字段的默认值，具体取值参考<a href="https://www.volcengine.com/docs/82379/1895586?lang=zh#203baa92">模型抽帧策略说明</a>。</div>



属性

preprocess_configs.video.**fps** `float / null` `默认值：1`

取值范围：`[0.2，5]`。

每秒钟从视频中抽取指定数量的图像。取值越高，对于视频中画面变化理解越精细；取值越低，对于视频中画面变化感知减弱，但是使用的token花费少，速度也更快。单视频token 用量范围在[10k, 80k]，具体参见[视频理解](https://www.volcengine.com/docs/82379/1895586?lang=zh#.55So6YeP6K-05piO)。


---



preprocess_configs.video.**model** `string` 

使用该文件进行推理时，要使用的视频理解模型 ID （Model ID）或 Endpoint ID。

<div data-tips="true" data-tips-type="default" data-tips-is-title="true">说明</div>


<div data-tips="true" data-tips-type="default">Files API 中设置的模型 ID 与推理使用的模型 ID 不强耦合，只影响上传视频文件时预处理抽帧策略。关于预处理抽帧策略，参见<a href="https://www.volcengine.com/docs/82379/1895586?lang=zh#203baa92">抽帧策略</a>。</div>



* 传入模型 ID：传入不同的模型 ID 会采用不同的抽帧策略。

* 传入 Endpoint ID：会按照上传时 Endpoint ID 映射的模型对应的抽帧策略进行抽帧。

* 不传该参数时：默认采用`doubao-seed-1.8`之前的模型对应的抽帧策略。


<div data-tips="true" data-tips-type="warning" data-tips-is-title="true">注意</div>



* <div data-tips="true" data-tips-type="warning"><code>doubao-seed-1.8</code>及后续模型支持更长的视频理解能力，抽帧数已从 640 帧提升至 1280 帧。</div>


* <div data-tips="true" data-tips-type="warning">如果要使用<code>doubao-seed-1-8-251228</code>进行视频理解，但通过 Files API 上传文件时未设置该模型 ID，则采用的是<code>doubao-seed-1.8</code>之前模型对应的抽帧策略，模型实际理解的帧数会减少。</div>


* <div data-tips="true" data-tips-type="warning">如需处理视频中的音频内容，该字段需指定为支持音频理解的模型，具体支持的模型可参见<a href="https://docs.volcengine.com/docs/82379/1330310?lang=zh#9619c0ba">模型列表</a>。</div>




---



preprocess_configs.video.**max_video_tokens** `Integer`

取值范围：[10240, 204800]。

视频最大 tokens 数，值越大保留的视频信息越多。


---



preprocess_configs.video.**min_frame_tokens** `Integer`

取值范围：[16, 128]。

单帧最小 tokens 数，用于控制单帧压缩的下限。值越小，允许对单帧进行更大的压缩，但单帧细节下降。


---



preprocess_configs.video.**max_frame_tokens** `Integer`

取值范围：[128, 640]。

单帧最大 tokens 数，用于控制单帧质量的上限。值越大，允许对单帧保留更多的细节。


---



preprocess_configs.video.**min_frames** `Integer` 

取值范围：[5, 16]。

最小抽帧数，用于保证短视频具备足够的处理帧数。若 `fps` 字段抽帧总数小于 `min_frames`，则优先按照 `min_frames` 配置要求抽帧。




---



**expire_at** `integer` `默认值：当前时刻+604800` 

取值范围：`[当前时刻+86400, 当前时刻+2592000]`，即最少保留1天，最多保留30天。

设置存储的有效期，需要传入UTC Unix时间戳（单位：秒）。 

<span id="NtL4xXlS"></span>
## 响应参数

> 跳转 [请求参数](https://www.volcengine.com/docs/82379/1870405#LGrPmcsa)


模型会返回对应的 [file](https://www.volcengine.com/docs/82379/1873424)[ object](https://www.volcengine.com/docs/82379/1873424?type=preview&lang=zh)。
