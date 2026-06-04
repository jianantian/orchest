<span id="c00407ce"></span>
# 1 接口功能
火山控制台开启试用：https://console.volcengine.com/speech/service/10028

对提供的长文本或网页链接进行分析总结，也可以对一个特定话题做联网总结，最终流式生成双人播客音频。
支持断点重试。

功能更新：

| | | \
|日期 |功能更新介绍 |
|---|---|
| | | \
| 2026 年 3 月 20 日 |1. 播客专属音色之外，支持 TTS 和 ICL复刻音色（复刻音色购买的 APPID 要和开通播客是同一个才能通过鉴权），支持 1.0 和 2.0 版本音色。（除了 mix 音色，多情感音色） |\
| |2. PodcastRoundEnd 事件加入本轮播客在完整播客的开始时间和结束时间。 |
| | | \
|2026 年 5 月 12 日 |1. 新增参数 strict_audit，用于声明安全审核等级，true代表严格审核、false代表普通审核，默认为false。会对action = 0 的 inpu_text 和 action = 4 的 prompt_text 生效。 |

<span id="1669cae7"></span>
# 2 接口说明
<span id="3c1a3afe"></span>
## 2.1 请求Request
<span id="1aaa2304"></span>
### 请求路径
`wss://openspeech.bytedance.com/api/v3/sami/podcasttts`
<span id="42b98f65"></span>
### 建连&鉴权
<span id="a0c6593e"></span>
#### Request Headers

| | | | | \
|Key |说明 |是否必须 |Value示例 |
|---|---|---|---|
| | | | | \
|X-Api-App-Id |\
| |使用火山引擎控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F) |是 |\
| | | |your-app-id |\
| | | | |
| | | | | \
|X-Api-Access-Key |\
| |使用火山引擎控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F) |是 |\
| | | |your-access-key |\
| | | | |
| | | | | \
|X-Api-Resource-Id |\
| |表示调用服务的资源信息 ID |\
| | |\
| |* 播客语音合成：volc.service_type.10050 |\
| | |\
| | |是 |\
| | | |* 播客语音合成：volc.service_type.10050 |
| | | | | \
|X-Api-App-Key |固定值 |是 |aGjiRDfUWi |
| | | | | \
|X-Api-Request-Id |标识客户端请求ID，uuid随机字符串 |否 |67ee89ba-7050-4c04-a3d7-ac61a63499b3 |

<span id="702d2796"></span>
#### Response Headers

| | | | \
|Key |说明 |Value示例 |
|---|---|---|
| | | | \
|X-Tt-Logid |服务端返回的 logid，建议用户获取和打印方便定位问题 |2025041513355271DF5CF1A0AE0508E78C |

<span id="66d951ec"></span>
### WebSocket 二进制协议
WebSocket 使用二进制协议传输数据。
协议的组成由至少 4 个字节的可变 header、payload size 和 payload 三部分组成，其中

* header 描述消息类型、序列化方式以及压缩格式等信息；
* payload size 是 payload 的长度；
* payload 是具体负载内容，依据消息类型不同 payload 内容不同；

需注意：协议中整数类型的字段都使用**大端**表示。
<span id="b03f6f15"></span>
##### 二进制帧

| | | | | \
|Byte |Left 4-bit |Right 4-bit |说明 |
|---|---|---|---|
| | | | | \
|0 - Left half |Protocol version | |目前只有v1，始终填0b0001 |
| | | | | \
|0 - Right half | |Header size (4x) |目前只有4字节，始终填0b0001 |
| | | | | \
|1 - Left half |Message type | |固定为0b001 |
| | | | | \
|1 - Right half | |Message type specific flags |\
| | | |在sendText时，为0 |\
| | | |在finishConnection时，为0b100 |
| | | | | \
|2 - Left half |Serialization method | |0b0000：Raw（无特殊序列化方式，主要针对二进制音频数据）0b0001：JSON（主要针对文本类型消息） |
| | | | | \
|2 - Right half | |Compression method |0b0000：无压缩0b0001：gzip |
| | || | \
|3 |Reserved | |留空（0b0000 0000） |
| | || | \
|[4 ~ 7] |[Optional field,like event number,...] | |取决于Message type specific flags，可能有、也可能没有 |
| | || | \
|... |Payload | |可能是音频数据、文本数据、音频文本混合数据 |

<span id="99690d1d"></span>
###### payload请求参数

| | | | | | \
|**字段** |**描述** |**是否必须** |**类型** |**默认值** |
|---|---|---|---|---|
| | | | | | \
|action |\
| |生成类型： |\
| | |\
| |* 0：根据提供的 input_text 或者 input_info.input_url 总结生成播客 |\
| |* 3：根据提供的 nlp_texts 对话文本直接生成播客 |\
| |* 4：根据提供的 prompt_text 文本联网生成播客 |是 |number |0 |
| | | | | | \
|input_text |待播客合成输入文本，上下文最长 32k，超过会报错 |\
| |> action = 0 时候和 input_info.input_url 二选一，都不为空优先生效 input_text |否 |string |—— |
| | | | | | \
|prompt_text |prompt文本，不具备指令能力 |\
| |> action = 4 时必填 |\
| |> 一般比较简单，比如 “火山引擎” ，“怎么平衡工作和生活？” |否 |string |—— |
| | | | | | \
|nlp_texts |待合成的播客轮次文本列表 |\
| |> action = 3 时必填 |否 |[]object |—— |
| | | | | | \
|nlp_texts.text |每个轮次播客文本 |\
| |> 单轮不超过 300 字符 |\
| |> 总文本长度不超过 10000 字符 |否 |string |—— |
| | | | | | \
|nlp_texts.speaker |每个轮次播客发音人 |\
| |> 详细参见：可选发音人列表 |否 |string |—— |
| | | | | | \
|input_info |输入辅助信息 |否 |object |—— |
| | | | | | \
|input_info.input_url |网页链接或者可下载的文件(pdf,doc,txt)链接,会自动转换成长文播客文本 |否 |string |—— |
| | | | | | \
|input_info.only_nlp_text |只输出播客轮次文本列表，没有音频 |否 |bool |—— |
| | | | | | \
|input_info.return_audio_url |返回可下载的完整播客音频链接，有效期 1h |\
| |> 新增一个事件 363 （PodcastEnd），里面会有 meta_info.audio_url 字段 |否 |bool |—— |
| | | | | | \
|input_info.input_text_max_length |action=0 模式下面的模型处理最大字符数，超过会截断文本，建议设置不超过 12000 可以保证模型处理的稳定性 |\
| |> 事件 363 （PodcastEnd），里面会有input_metrics表示截断信息 |否 | int |—— |
| | | | | | \
|input_info.max_char_length_per_round |支持每轮最大合成字符数（不是硬截断）。优先考虑语句完整性（带着句号），后判断是否超过最大字符数。 |否 |int |—— |
| | | | | | \
|input_info.strict_audit |用于声明安全审核等级，true代表严格审核、false代表普通审核，默认为false |否 |bool |false |
| | | | | | \
|use_head_music |是否使用开头音效 |否 |bool |true |
| | | | | | \
|use_tail_music |是否使用结尾音效 |否 |bool |false |
| | | | | | \
|aigc_watermark |是否在合成结尾增加音频节奏标识，作为显示水印 |否 |bool |false |
| | | | | | \
|aigc_metadata |在合成音频 header加入元数据隐式水印，支持 mp3/wav/ogg_opus |否 |object |—— |
| | | | | | \
|aigc_metadata.enable |是否启用隐式水印 |否 |bool |false |
| | | | | | \
|aigc_metadata.content_producer |合成服务提供者的名称或编码 |否 |string |"" |
| | | | | | \
|aigc_metadata.produce_id |内容制作编号 |否 |string |"" |
| | | | | | \
|aigc_metadata.content_propagator |内容传播服务提供者的名称或编码 |否 |string |"" |
| | | | | | \
|aigc_metadata.propagate_id |内容传播编号 |否 |string |"" |
| | | | | | \
|audio_config |音频参数，便于服务节省音频解码耗时 |\
| | |否 |object |—— |
| | | | | | \
|audio_config.format |\
| |音频编码格式，mp3/ogg_opus/pcm/aac |\
| | |否 |string |pcm |
| | | | | | \
|audio_config.sample_rate |\
| |音频采样率，可选值 [16000, 24000, 48000] |\
| | |否 |number |24000 |
| | | | | | \
|audio_config.speech_rate |\
| |语速，取值范围[-50,100]，100代表2.0倍速，-50代表0.5倍数 |否 |number |0 |
| | | | | | \
|speaker_info |指定发音人信息 |否 |object |—— |
| | | | | | \
|speaker_info.random_order |发音人是否随机顺序开始，默认是 |否 |bool |true |
| | | | | | \
|speaker_info.speakers  |播客发音人, 只能选择 2 发音人 |\
| |> 详细参见：可选发音人列表 |否 |[]string | |
| | | | | | \
|speaker_info.speaker_additions  |发音人额外信息，使用 TTS 和 ICL 音色的时候可生效： |\
| |key:  speaker_id |\
| |value:  参考tts 合成文档（[https://www.volcengine.com/docs/6561/1719100?lang=zh](https://www.volcengine.com/docs/6561/1719100?lang=zh)）里面的 【req_params.additions】 参数，是个jsonstring |\
| | |\
| |在使用复刻 2.0 音色的使用如果要切换模型也需要输入到下面的additions参数里面。model  参数的取值参考上述文档里面【req_params.model】的介绍。 |\
| | |\
| |参考：{"SPEAKERID1": additions, "SPEAKERID2": additions} |\
| |Golang: |\
| |additions = fmt.Sprintf("{"model": "seed-tts-2.0-standard"}") |\
| | |\
| |Python: |\
| |additions = json.dumps({"model": "seed-tts-2.0-standard"}) |否 |map[string][string] |—— |
| | | | | | \
|retry_info |重试信息 |否 |object |—— |
| | | | | | \
|retry_info.retry_task_id |前一个没获取完整的播客记录的 task_id(第一次StartSession使用的 session_id就是任务的 task_id) |否 |string |—— |
| | | | | | \
|retry_info.last_finished_round_id |前一个获取完整的播客记录的轮次 id |否 |number |—— |

**可选发音人列表**
> 发音人的选择最好用同个系列的配对使用会有更好的效果
> 默认：dayi/mizai 系列


| | | \
|**系列** |**发音人名称** |
|---|---|
| | | \
|黑猫侦探社咪仔 |zh_female_mizaitongxue_v2_saturn_bigtts |
|^^| | \
| |zh_male_dayixiansheng_v2_saturn_bigtts |
| | | \
|刘飞和潇磊 |zh_male_liufei_v2_saturn_bigtts |
|^^| | \
| |zh_male_xiaolei_v2_saturn_bigtts |
| | | \
|TTS 音色列表 |https://www.volcengine.com/docs/6561/1257544?lang=zh |

**参数使用示例**
action = 0 长文本总结模式示例
```JSON
{
    "input_id": "test_podcast",
    "input_text": "分析下当前的大模型发展",
    "action": 0,
    "use_head_music": false,
    "audio_config": {
        "format": "mp3",
        "sample_rate": 24000,
        "speech_rate": 0,
    },
    "speaker_info": {
        "random_order": true,
        "speakers": [
            "zh_male_dayixiansheng_v2_saturn_bigtts",
            "zh_female_mizaitongxue_v2_saturn_bigtts"
        ]
    },
    "aigc_watermark": false,
    "aigc_metadata": {
        "enable": true,
        "content_producer": "volcengine",
        "produce_id": "12abc",
        "content_propagator": "volcengine",
        "propagate_id": "34def"
    }
}
```


action = 0 url 解析模式示例
```JSON
{
    "input_id": "test_podcast",
    "action": 0,
    "use_head_music": false,
    "audio_config": {
        "format": "mp3",
        "sample_rate": 24000,
        "speech_rate": 0,
    },
    "input_info": {
        "input_url": "https://mp.weixin.qq.com/s/CiN0XRWQc3hIV9lLLS0rGA"
    }
}
```


action = 3 根据提供的对话文本调用示例
```JSON
{
    "input_id": "test_podcast",
    "action": 3,
    "use_head_music": false,
    "audio_config": {
        "format": "mp3",
        "sample_rate": 24000,
        "speech_rate": 0,
    },
    "nlp_texts": [
        {
            "speaker": "zh_male_dayixiansheng_v2_saturn_bigtts",
            "text": "今天呢我们要聊的呢是火山引擎在这个 FORCE 原动力大会上面的一些比较重磅的发布。"
        },
        {
            "speaker": "zh_female_mizaitongxue_v2_saturn_bigtts",
            "text": "来看看都有哪些亮点哈。"
        }
    ]
}
```


action = 4 根据提供prompt文本联网总结调用示例
```JSON
{
    "input_id": "test_podcast",
    "action": 4,
    "prompt_text": "火山引擎",
    "use_head_music": false,
    "audio_config": {
        "format": "mp3",
        "sample_rate": 24000,
        "speech_rate": 0,
    }
}
```

<span id="867b5edf"></span>
## 2.2 响应Response
<span id="08d6433e"></span>
### 建连响应
主要关注建连阶段 HTTP Response 的状态码和 Body

* 建连成功：状态码为 200
* 建连失败：状态码不为 200，Body 中提供错误原因说明

<span id="f38fbebd"></span>
### WebSocket 传输响应
<span id="fdc96a41"></span>
#### 二进制帧 - 正常响应帧

| | | | | \
|Byte |Left 4-bit |Right 4-bit |说明 |
|---|---|---|---|
| | | | | \
|0 - Left half |Protocol version | |目前只有v1，始终填0b0001 |
| | | | | \
|0 - Right half | |Header size (4x) |目前只有4字节，始终填0b0001 |
| | | | | \
|1 - Left half |Message type | |音频帧返回：0b1011 |\
| | | |其他帧返回：0b1001 |
| | | | | \
|1 - Right half | |Message type specific flags |固定为0b0100 |
| | | | | \
|2 - Left half |Serialization method | |0b0000：Raw（无特殊序列化方式，主要针对二进制音频数据）0b0001：JSON（主要针对文本类型消息） |
| | | | | \
|2 - Right half | |Compression method |0b0000：无压缩0b0001：gzip |
| | || | \
|3 |Reserved | |留空（0b0000 0000） |
| | || | \
|[4 ~ 7] |[Optional field,like event number,...] |\
| | | |取决于Message type specific flags，可能有、也可能没有 |
| | || | \
|... |Payload | |可能是音频数据、文本数据、音频文本混合数据 |

<span id="3db9c08f"></span>
##### payload响应参数

| | | | \
|字段 |描述 |类型 |
|---|---|---|
| | | | \
|data |返回的二进制数据包 |byte |
| | | | \
|event |返回的事件类型 |number |

<span id="612af984"></span>
#### 二进制帧 - 错误响应帧

| | | | | \
|Byte |Left 4-bit |Right 4-bit |说明 |
|---|---|---|---|
| | | | | \
|0 - Left half |Protocol version | |目前只有v1，始终填0b0001 |
| | | | | \
|0 - Right half | |Header size (4x) |目前只有4字节，始终填0b0001 |
| | | | | \
|1  |Message type |Message type specific flags |0b11110000 |
| | | | | \
|2 - Left half |Serialization method | |0b0000：Raw（无特殊序列化方式，主要针对二进制音频数据）0b0001：JSON（主要针对文本类型消息） |
| | | | | \
|2 - Right half | |Compression method |0b0000：无压缩0b0001：gzip |
| | || | \
|3 |Reserved | |留空（0b0000 0000） |
| | || | \
|[4 ~ 7] |Error code | |错误码 |
| | || | \
|... |Payload | |错误消息对象 |

<span id="3cd5a26a"></span>
## 2.3 event定义
在生成 podcast 阶段，不需要客户端发送上行的event帧。event类型如下：

| | | | | \
|Event code |含义 |事件类型 |应用阶段：上行/下行 |
|---|---|---|---|
| | | | | \
|150 |SessionStarted，会话任务开始 |Session 类 |下行 |
| | | | | \
|360 |PodcastRoundStart，播客返回新轮次内容开始，带着轮次 idx 和 speaker |数据类 |下行 |
| | | | | \
|361 |PodcastRoundResponse，播客返回轮次的音频内容 |数据类 |下行 |
| | | | | \
|362 |PodcastRoundEnd，播客返回内容当前轮次结束 |\
| |示例： |\
| |{ |\
| |"audio_duration":8.419333, // 单位秒，时长 |\
| |"end_time":38.216, // 单位秒，开始时间 |\
| |"start_time":25.25 // 单位秒，结束时间 |\
| |} |数据类 |下行 |
| | | | | \
|363 |PodcastEnd，返回一些播客总结性的信息，表示播客结束（为了兼容之前的使用，这个事件不一定会返回） |\
| |示例：{'meta_info': {'audio_url': 'https://speech-tts-podcast.tos-cn-beijing.volces.com/speech-tts-podcast/tts_audio/aGjiRDfUWi/b598a76a-ebb2-4117-9270-9b3b740e1adb/podcast_demo.mp3?X-Tos-Algorithm=TOS4-HMAC-SHA256&X-Tos-Credential=AKLTY2M5ZDg4NzA2MDUxNDI0ZThkMTU5YTNkNjk4ZDg5OTM%2F20250825%2Fcn-beijing%2Ftos%2Frequest&X-Tos-Date=20250825T070712Z&X-Tos-Expires=3600&X-Tos-Signature=55a5e2d0bd40f91fc846068f9d35737b96e9891134aabb783b973f91b5f993c9&X-Tos-SignedHeaders=host', 'topics': None, 'input_metrics': {'origin_input_text_length': 14, 'input_text_length': 10, 'input_text_truncated': True}}} |数据类 |下行 |
| | | | | \
|152 |SessionFinished，会话已结束（上行&下行） |\
| |标识语音一个完整的语音合成完成 |Session 类 |\
| | | |下行 |
| | | | | \
|154 |UsageResponse, 播客返回的用量事件。 |\
| |示例:{"usage":{"input_text_tokens":980,"output_audio_tokens":0}}  其中input_text_tokens表示"API调用token-输入-文本", output_audio_tokens表示"API调用token-输出-音频"  。 |数据类 |下行 |

在关闭连接阶段，需要客户端传递上行event帧去关闭连接。event类型如下：

| | | | | \
|Event code |含义 |事件类型 |应用阶段：上行/下行 |
|---|---|---|---|
| | | | | \
|2 |FinishConnection，结束连接 |Connect 类 |上行 |
| | | | | \
|52 |ConnectionFinished 结束连接成功 |Connect 类 |下行 |

**示意图（重要！！！！）**：

![Image](https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/9b8ade61c5c94728b1b789769272eb1c~tplv-goo7wpa0wc-image.image =2034x)
<span id="ff2a496b"></span>
## 2.4 不同类型帧举例说明
<span id="e8b4a309"></span>
### StartSession
<span id="8c1de3b6"></span>
#### 请求 request

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |StartSession | |event type | |
| | | | || \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | | | || \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | | | || \
|24 ~ 27 |uint32( ...) | |len(payload) | |
| | || || \
|28 ~ ... |{} |\
| | | |`payload` 见下面的例子 |\
| | | | | |

`payload`
```JSON
{
    "input_id": "test_podcast",
    "input_text": "分析下当前的大模型发展",
    "scene": "deep_research",
    "action": 0,
    "use_head_music": false,
    "audio_params": {
        "format": "pcm",
        "sample_rate": 24000,
        "speech_rate": 0,
    }
}
```

断点续传的时候需要加上 retry 信息
`payload`
```JSON
{
    "input_id": "test_podcast",
    "input_text": "分析下当前的大模型发展",
    "scene": "deep_research",
    "action": 0,
    "use_head_music": false,
    "audio_params": {
        "format": "pcm",
        "sample_rate": 24000,
        "speech_rate": 0,
    },
    "retry_info": {
        "retry_task_id": "xxxxxxxxx",
        "last_finished_round_id": 5
    }
}
```

<span id="189e205c"></span>
#### 响应Response
<span id="44d0f8d2"></span>
##### SessionStarted

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1011 |0100 |Audio-only response |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || | | \
|4 ~ 7 |SessionStarted | |event type | |
| | || | | \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | || | | \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | || | | \
|24 ~ 27 |uint32( ...) | |len(audio_binary) | |
| | || | | \
|28 ~ ... |{ |\
| |} |\
| | | |payload_json |\
| | | |扩展保留，暂留空JSON |\
| | | | | |

**UsageResponse**

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1011 |0100 |Audio-only response |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || | | \
|4 ~ 7 |UsageResponse | |event type | |
| | || | | \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | || | | \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | || | | \
|24 ~ 27 |uint32( ...) | |len(audio_binary) | |
| | || | | \
|28 ~ ... |文本 token 消耗推送： |\
| |{"usage":{"input_text_tokens":980,"output_audio_tokens":0}} |\
| |音频 token 消耗推送： |\
| |{"usage":{"input_text_tokens": 0,"output_audio_tokens":501}} | |payload_json |\
| | | |用量信息 |\
| | | | | |


下面三个事件循环 ♻️,如果没有收到PodcastTTSRoundEnd（需要和PodcastSpeaker成对出现）就断掉了链接说明需要断点续传重新发起请求
<span id="be25536d"></span>
##### PodcastRoundStart

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1011 |0100 |Audio-only response |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || | | \
|4 ~ 7 |PodcastRoundStart | |event type | |
| | || | | \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | || | | \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | || | | \
|24 ~ 27 |uint32( ...) | |len(audio_binary) | |
| | || | | \
|28 ~ ... |{ |\
| | "text_type": "", // 文本类型 |\
| | "speaker": "", // 本次说话speaker |\
| | "round_id": -1, // 对话轮次，-1 是开头音乐 |\
| | "text": "" // 对话文本 |\
| |} |\
| | | |response_meta_json |\
| | | | |\
| | | |round_id == -1，代表开头音频 |\
| | | |round_id ==9999，代表结尾音频 |\
| | | | | |

<span id="8ff44443"></span>
##### PodcastRoundResponse

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |PodcastTTSResponse | |event type | |
| | | | || \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | | | || \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | | | || \
|24 ~ 27 |uint32( ...) | |len(payload) | |
| | || || \
|28 ~ ... |... 音频内容 |\
| | | |payload |\
| | | | | |

<span id="00071d9e"></span>
##### PodcastRoundEnd

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || | | \
|4 ~ 7 |PodcastRoundEnd | |event type | |
| | || || \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | || || \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | || || \
|24 ~ 27 |uint32( ...) | |len(response_meta_json) | |
| | || || \
|28 ~ ... |{ |\
| | "is_error": true, |\
| | "error_msg": "something error" |\
| |} |\
| |or |\
| |{ |\
| |"audio_duration":8.419333, // 单位秒 |\
| |"end_time":38.216, // 单位秒 |\
| |"start_time":25.25 // 单位秒 |\
| |} |\
| | |\
| |每句播客返回带着时长以及在完整播客音频的开始和结束时间位置。 |\
| | | |response_meta_json |\
| | | | | |

<span id="c228fade"></span>
##### PodcastEnd

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || | | \
|4 ~ 7 |PodcastEnd | |event type | |
| | || || \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | || || \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | || || \
|24 ~ 27 |uint32( ...) | |len(response_meta_json) | |
| | || || \
|28 ~ ... |{'meta_info': {'audio_url': 'https://speech-tts-podcast.tos-cn-beijing.volces.com/speech-tts-podcast/tts_audio/aGjiRDfUWi/a0979493-196a-42ad-aff1-1dfe63c7e219/podcast_demo.mp3?X-Tos-Algorithm=TOS4-HMAC-SHA256&X-Tos-Credential=AKLTY2M5ZDg4NzA2MDUxNDI0ZThkMTU5YTNkNjk4ZDg5OTM%2F20250825%2Fcn-beijing%2Ftos%2Frequest&X-Tos-Date=20250825T084035Z&X-Tos-Expires=3600&X-Tos-Signature=2a549ee5f5ed8a32ce34d475ccf56f50a02e78b3431eb760fb9edc3d0d15296b&X-Tos-SignedHeaders=host', 'topics': None}} |\
| | | |response_meta_json |\
| | | |没有需要返回的 meta 信息这个事件不会推送 | |


<span id="4bdd490c"></span>
### FinishSession
<span id="6b7beb11"></span>
#### 请求request

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |FinishSession | |event type | |
| | | | || \
|8 ~ 11 |uint32(12) | |len(<session_id>) | |
| | | | || \
|12 ~ 23 |nxckjoejnkegf | |session_id | |
| | | | || \
|24 ~ 27 |uint32( ...) | |len(payload) | |
| | || || \
|28 ~ ... |{} |\
| | | |tts_session_meta |\
| | | | | |

<span id="c0f24907"></span>
#### 响应response

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |SessionFinished | |event type | |
| | || || \
|8 ~ 11 |uint32(7) | |len(<connection_id>) | |
| | || || \
|12 ~ 15 |uint32(58) | |len(<response_meta_json>) | |
| | || || \
|28 ~ ... |{ |\
| | "status_code": 20000000, |\
| | "message": "ok" |\
| |} | |response_meta_json |\
| | | | |\
| | | |* 仅含status_code和message字段 |\
| | | | |\
| | | | | |

<span id="0ae4a5f4"></span>
### FinishConnection
<span id="479c59a9"></span>
#### 请求request

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |FinishConnection | |event type | |
| | || || \
|8 ~ 11 |uint32(2) | |len(<response_meta_json>) | |
| | || || \
|12 ~ 13 |{} | |tts_session_meta | |

<span id="4f3476aa"></span>
#### 响应response

| | | | || \
|Byte |Left 4-bit |Right 4-bit |说明 | |
|---|---|---|---|---|
| | | | | | \
|0 |0001 |0001 |v1 |4-byte header |
| | | | | | \
|1 |1001 |0100 |Full-client request |with event number |
| | | | | | \
|2 |0001 |0000 |JSON |no compression |
| | | | | | \
|3 |0000 |0000 | | |
| | || || \
|4 ~ 7 |ConnectionFinished | |event type | |
| | || || \
|8 ~ 11 |uint32(7) | |len(<connection_id>) | |
| | || || \
|12 ~ 15 |uint32(58) | |len(<response_meta_json>) | |
| | || || \
|28 ~ ... |{ |\
| | "status_code": 20000000, |\
| | "message": "ok" |\
| |} | |response_meta_json |\
| | | | |\
| | | |* 仅含status_code和message字段 |\
| | | | |\
| | | | | |

<span id="a6e99449"></span>
# 3 错误码

| | | | \
|Code |Message |说明 |
|---|---|---|
| | | | \
|20000000 |ok |音频合成结束的成功状态码 |
| | | | \
|45000000 |quota exceeded for types: concurrency |并发限流，一般是请求并发数超过限制 |
| | | | \
|40000010 |PodcastTTS invalid param |参数错误，根据 msg 信息检测入参是否正确 |
| | | | \
|40000022 |IllegalPayload:TextRiskAuditFailed |开启严格风控之后命中风控合成失败，建议检查输入文本 |
| | | | \
|55000000 |服务端一些error |服务端通用错误 |
| | | | \
|50302102 |action = 0 的报错： |\
| |NLP RespError(50000001/FangzhouPodcastNLPFailed:content filter)  |\
| | |\
| |action = 4 的报错： |\
| |NLP RespError(50000001/server error: GetOutlineFailed:Failed to generate the podcast. The cause of the error is: content filter) |\
| |或者 |\
| |NLP RespError(50000001/server error: GetOutlineFailed:Failed to generate the podcast. The cause of the error is: get outline base model return empty) |触发安全审核过滤 |
| | | | \
|50302102 |NLP RespError(50000001/FangzhouPodcastNLPFailed:content length)  |文本上下文超过限制 |

<span id="6049e8be"></span>
# 4 调用示例

```mixin-react
return (<Tabs>
<Tabs.TabPane title="Python调用示例" key="Svx3pxAfmJ"><RenderMd content={`<span id="a3c05b77"></span>
### 前提条件

* 调用之前，您需要获取以下信息：
   * \`<appid>\`：使用控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。
   * \`<access_token>\`：使用控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。

<span id="d3d0871e"></span>
### Python环境

* Python：3.9版本及以上。
* Pip：25.1.1版本及以上。您可以使用下面命令安装。

\`\`\`Bash
python3 -m pip install --upgrade pip
\`\`\`

<span id="25d265ba"></span>
### 下载代码示例
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/81f64b12d14647199d220383d08a0cd2~tplv-goo7wpa0wc-image.image" name="volcengine.speech.volc_speech_python_sdk_1.0.0.25.tar.gz" ></Attachment>
解压缩代码包，安装依赖
\`\`\`Bash
mkdir -p volcengine_podcasts_demo
tar xvzf volcengine_podcasts_demo.tar.gz -C ./volcengine_podcasts_demo
cd volcengine_podcasts_demo
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install --upgrade pip
pip3 install -e .
\`\`\`

<span id="856193c9"></span>
### 发起调用
> \`<appid>\`替换为您的APP ID。
> \`<access_token>\`替换为您的Access Token。
> \`<\`text\`>\` 为播客文本。

\`\`\`Bash
python3 examples/volcengine/podcasts.py --appid <appid> --access_token <access_token> --text "介绍下火山引擎" 
\`\`\`

`}></RenderMd></Tabs.TabPane>
<Tabs.TabPane title="Java调用示例" key="Rcn3QsklGw"><RenderMd content={`<span id="9e95f949"></span>
### 前提条件

* 调用之前，您需要获取以下信息：
   * \`<appid>\`：使用控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。
   * \`<access_token>\`：使用控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。

<span id="d9f83856"></span>
### Java环境

* Java：21版本及以上。
* Maven：3.9.10版本及以上。

<span id="9e71a59b"></span>
### 下载代码示例
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/e9e28010dfa74e8c89cf0a1d489353fb~tplv-goo7wpa0wc-image.image" name="volcengine.speech.volc_speech_java_sdk_1.0.0.19.tar.gz" ></Attachment>
解压缩代码包，安装依赖
\`\`\`Bash
mkdir -p volcengine_podcasts_demo
tar xvzf volcengine_podcasts_demo.tar.gz -C ./volcengine_podcasts_demo
cd volcengine_podcasts_demo
\`\`\`

<span id="f0fa987e"></span>
### 发起调用
> \`<appid>\`替换为您的APP ID。
> \`<access_token>\`替换为您的Access Token。
> \`<\`text\`>\` 为播客文本。

\`\`\`Bash
mvn compile exec:java -Dexec.mainClass=com.speech.volcengine.Podcasts -DappId=<appid> -DaccessToken=<access_token> -Dtext="介绍下火山引擎"
\`\`\`

`}></RenderMd></Tabs.TabPane>
<Tabs.TabPane title="Go调用示例" key="NTa0nb2keF"><RenderMd content={`<span id="7c32bea6"></span>
### 前提条件

* 调用之前，您需要获取以下信息：
   * \`<appid>\`：使用控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。
   * \`<access_token>\`：使用控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。

<span id="b7fa2152"></span>
### Go环境

* Go：1.21.0版本及以上。

<span id="088c8441"></span>
### 下载代码示例
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/7b4e428a005b4276ab72c9a43de8563a~tplv-goo7wpa0wc-image.image" name="volcengine.speech.volc_speech_go_sdk_1.0.0.23.tar.gz" ></Attachment>
解压缩代码包，安装依赖
\`\`\`Bash
mkdir -p volcengine_podcasts_demo
tar xvzf volcengine_podcasts_demo.tar.gz -C ./volcengine_podcasts_demo
cd volcengine_podcasts_demo
\`\`\`

<span id="4bd5054a"></span>
### 发起调用
> \`<appid>\`替换为您的APP ID。
> \`<access_token>\`替换为您的Access Token。
> \`<\`text\`>\` 为播客文本。

\`\`\`Bash
go run volcengine/podcasts/main.go --appid <appid> --access_token <access_token>  --text "介绍下火山引擎"
\`\`\`

`}></RenderMd></Tabs.TabPane>
<Tabs.TabPane title="C#调用示例" key="TcbduWe5vi"><RenderMd content={`<span id="05a6aa4b"></span>
### 前提条件

* 调用之前，您需要获取以下信息：
   * \`<appid>\`：使用控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。
   * \`<access_token>\`：使用控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。

<span id="d811040a"></span>
### C#环境

* .Net 9.0版本。

<span id="24989e6a"></span>
### 下载代码示例
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/6d89d6f745124e2bbebf2281fef73306~tplv-goo7wpa0wc-image.image" name="volcengine.speech.volc_speech_dotnet_sdk_1.0.0.13.tar.gz" ></Attachment>
解压缩代码包，安装依赖
\`\`\`Bash
mkdir -p volcengine_podcasts_demo
tar xvzf volcengine_podcasts_demo.tar.gz -C ./volcengine_podcasts_demo
cd volcengine_podcasts_demo
\`\`\`

<span id="5b232098"></span>
### 发起调用
> \`<appid>\`替换为您的APP ID。
> \`<access_token>\`替换为您的Access Token。
> \`<\`text\`>\` 为播客文本。

\`\`\`Bash
dotnet run --project Volcengine/Podcasts/Volcengine.Speech.Podcasts.csproj -- --appid <appid> --access_token <access_token> --text "介绍下火山引擎"
\`\`\`

`}></RenderMd></Tabs.TabPane>
<Tabs.TabPane title="TypeScript调用示例" key="gksi3vM55q"><RenderMd content={`<span id="ad07bbae"></span>
### 前提条件

* 调用之前，您需要获取以下信息：
   * \`<appid>\`：使用控制台获取的APP ID，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。
   * \`<access_token>\`：使用控制台获取的Access Token，可参考 [控制台使用FAQ-Q1](https://www.volcengine.com/docs/6561/196768#q1%EF%BC%9A%E5%93%AA%E9%87%8C%E5%8F%AF%E4%BB%A5%E8%8E%B7%E5%8F%96%E5%88%B0%E4%BB%A5%E4%B8%8B%E5%8F%82%E6%95%B0appid%EF%BC%8Ccluster%EF%BC%8Ctoken%EF%BC%8Cauthorization-type%EF%BC%8Csecret-key-%EF%BC%9F)。

<span id="4cac4d0f"></span>
### node环境

* node：v24.0版本及以上。

<span id="9da02db9"></span>
### 下载代码示例
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/e2d6eb69d5134beda0470fc8c4b2b6b7~tplv-goo7wpa0wc-image.image" name="volcengine.speech.volc_speech_js_sdk_1.0.0.19.tar.gz" ></Attachment>
<span id="f23044b0"></span>
### 解压缩代码包，安装依赖
\`\`\`Bash
mkdir -p volcengine_podcasts_demo
tar xvzf volcengine_podcasts_demo.tar.gz -C ./volcengine_podcasts_demo
cd volcengine_podcasts_demo
npm install
npm install -g typescript
npm install -g ts-node
\`\`\`

<span id="69c963b6"></span>
### 发起调用
> \`<appid>\`替换为您的APP ID。
> \`<access_token>\`替换为您的Access Token。
> \`<\`text\`>\` 为播客文本。

\`\`\`Bash
npx ts-node src/volcengine/podcasts.ts --appid <appid> --access_token <access_token> --text "介绍下火山引擎"
\`\`\`

`}></RenderMd></Tabs.TabPane></Tabs>);
 ```


输出音频 demo：
<Attachment link="https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/4043a92404914e7787e98b234d8d945d~tplv-goo7wpa0wc-image.image" name="podcast_final.mp3" ></Attachment>
