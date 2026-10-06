# 常见问题

## 模型调用会产生费用吗？

费用以所选模型服务商的计费规则为准，使用自己的 API Key 调用。Ollama 本地模型不产生模型 API 调用费用。

## 对话和 API Key 如何处理？

对话保存在本机，调用模型时会将对话与相关工具结果发送至配置的 API 地址。API Key 明文保存在本机 `config.json`，分享配置或日志前请移除密钥。

## BetterGI 的功能都已适配吗？

目前已接入配置、脚本与调度组操作，完整功能适配仍在推进。后续安排见[开发者的话](https://sleepy-doll.restless-nh3.com/#/docs/developer)。

---

更多说明见[使用说明](guide.md)。
