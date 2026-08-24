# cmx-mdm/

> 主数据管理（MDM）域分组。引擎已抽为**独立微服务**（对标 flow/report/rules/model），本分组现仅剩**平台反代薄壳**。

## 分组现状

| crate | 说明 |
| --- | --- |
| `cmx-mdm-proxy` | 平台反代薄壳：`MdmProxyModule` 把门户 `/api/mdm/*` + `portal.mdm.*` native 页取页请求透明转发到独立 cmx-mdm-server（`[center_client.services].mdm` per-key 定位）。**无进程内嵌兜底**：没配目标 = 门户不挂 `/api/mdm/*` 路由 |

引擎三件套（`cmx-mdm-model` / `cmx-mdm-store-pg` / `cmx-mdm-app` 中立核）与
`cmx-mdm-server`（:8095 chassis bin）已物理迁至独立 workspace `../../../cmx-mdm`
（与 cmx-container 并排，基础设施经跨 workspace path 复用）。转发核（头卫生 / 三层出站
鉴权 / 超时拆分 / 流式转发 / 502/503 兜底）在 `cmx-proxy-core`，各反代壳共用。
