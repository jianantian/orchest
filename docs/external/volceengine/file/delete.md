`DELETE https://ark.cn-beijing.volces.com/api/v3/files/{file_id}`

根据文件ID删除文件，并将文件从存储空间中移除。


<Tabs>
<Tab zoneid="5gPFP2ta" title="快速入口">
<TabTitle>快速入口</TabTitle>

 <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_2abecd05ca2779567c6d32f0ddc7874d.png) </span>[模型列表](https://www.volcengine.com/docs/82379/1330310)          <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_a5fdd3028d35cc512a10bd71b982b6eb.png) </span>[模型计费](https://www.volcengine.com/docs/82379/1544106)       <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_57d0bca8e0d122ab1191b40101b5df75.png) </span>[模型调用教程](https://www.volcengine.com/docs/82379/1585128)    <span>![图片](https://portal.volccdn.com/obj/volcfe/cloud-universal-doc/upload_afbcf38bdec05c05089d5de5c3fd8fc8.png) </span>[API Key](https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D)


</Tab>
<Tab zoneid="enBSXJ0V" title="鉴权说明">
<TabTitle>鉴权说明</TabTitle>

本接口仅支持 API Key 鉴权，请在 [获取 API Key](https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey) 页面，获取长效 API Key。


</Tab>
</Tabs>


<span id="hnU4cNWd"></span>
## 请求参数

<span id="Uxe9XAQw"></span>
### 路径参数


---



**id** `string` <span data-api-tag="require|yM0oK2">必选</span>

待删除的文件id。

<span id="0EtRjtOR"></span>
## 响应参数


---



**id** `string`

被删除的文件id。


---



**object** `string`

固定为 `file`。


---



**deleted** `boolean`

文件被删除，取值`true`表明删除成功。
