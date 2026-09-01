# cmx-mdm-proxy —— 主数据（MDM）平台反代薄壳

主数据治理已抽取为**独立微服务**（对标 flow / report / rules）：

- **中立核 `cmx-mdm-app`**：全部 handler + 路由 + M5 分发引擎 + M7 流程客户端，位于独立
  workspace `../../../cmx-mdm`（与 cmx-container 并排），由 `cmx-mdm-server`（:8095）承载。
- **本 crate 只含反代**：`MdmProxyModule` 把平台 `/api/mdm/*` 透明转发到远程 cmx-mdm-server，
  `with_mdm_page_proxy` 把主数据拥有的 native 页（`portal.mdm.*`）取页请求反代过去。前端零改。

切换只看 `[service_rpc.services].mdm` 服务定位配置（url 静态基址或 discovery Nacos 选例）：
配了才挂主数据路由；没配则门户不挂 `/api/mdm/*`（无进程内嵌形态）。转发核（头卫生 / 三层出站
鉴权 / 超时拆分 / 流式转发 / 502/503 兜底）在 `cmx-proxy-core`，与 flow / report / rules 反代壳共用。
