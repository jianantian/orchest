`GET https://ark.cn-beijing.volces.com/api/v3/files/{file_id}`

通过 File id 获取文件信息。


<Tabs>
<Tab zoneid="TIcwAPbc" title="鉴权说明">
<TabTitle>鉴权说明</TabTitle>

本接口支持 API Key /Access Key 鉴权，详见[鉴权认证方式](https://www.volcengine.com/docs/82379/1298459)。


</Tab>
<Tab zoneid="NQzR9pWM" title="快速入口">
<TabTitle>快速入口</TabTitle>

<span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_2abecd05ca2779567c6d32f0ddc7874d.png) </span>[模型列表](https://www.volcengine.com/docs/82379/1330310)    <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_a5fdd3028d35cc512a10bd71b982b6eb.png) </span>[模型计费](https://www.volcengine.com/docs/82379/1544106)     <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_57d0bca8e0d122ab1191b40101b5df75.png) </span>[Files](https://www.volcengine.com/docs/82379/1885708)[ API 教程](https://www.volcengine.com/docs/82379/1885708)   <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_afbcf38bdec05c05089d5de5c3fd8fc8.png) </span>[API Key](https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D)


</Tab>
</Tabs>


<span id="YP6bDFZC"></span>
## 请求参数 

<span id="wcna9TMz"></span>
### 路径参数


---



id `string` <span data-api-tag="require|at4Lbm">必选</span>

待检索的文件 id。

<span id="5PiUp3nH"></span>
## 响应参数

模型会返回对应的 [file](https://www.volcengine.com/docs/82379/1873424?type=preview&lang=zh)[ object](https://www.volcengine.com/docs/82379/1873424?type=preview&lang=zh)。
