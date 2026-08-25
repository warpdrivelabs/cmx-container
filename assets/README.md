# assets/ —— 开发期统一资产工作区

按服务名隔离的页面/菜单/元数据真源。**开发、修改都在本目录进行**；发布时用
`scripts/publish-assets.sh <svc>` 拷贝到对应主应用仓（或打包时直接 COPY 本目录）。

## 目录归属（一服务一文件夹，页面 id 一服务一前缀）

| 文件夹 | 属主服务 | 页面 id 前缀 |
|---|---|---|
| `portal/` | cmx-portalservice | `portal.job/notify/system/display/*`、`demo.*`、`_legacy` 裸名 |
| `model/` | cmx-model | **`portal.model.*`**（2026-08-24 统一改名） |
| `mdm/` | cmx-mdm | `portal.mdm.*` |
| `flow/` | cmx-flowengine | `portal.flow.*` |
| `report/` | cmx-report | `portal.rpt.*` / `portal.consol.*` |
| `rules/` | cmx-rulesengine | `portal.rules.*` |

菜单定义真源：`model/data/menu-pages/<domain>/<app>/<module>/*.json`
（经模型中心 deploy 写入平台库 `cmx_menu`；门户侧栏只读 DB）。

## 各服务配置指向（[assets] 段）

```toml
root          = "../cmx-container/assets/<svc>/data"     # 有内容数据的服务才需要
ui_native_dir = "../cmx-container/assets/<svc>/web/ui-native"
ui_html_dir   = "../cmx-container/assets/<svc>/web/ui-html"
```

## ⚠️ 冻结备份（勿删勿改）

- `cmx-container/data/` —— 迁移前门户内容根，已整体拷入本工作区，**就地冻结为备份**；
- `cmx-model/data/` —— 同上。

两者在所有服务配置切换（2026-08-24）后已无任何运行时引用，仅作历史回溯用途。
如需找回旧版页面/菜单，以这两个目录为准；日常修改一律走本工作区。

## 已知过渡态

设计器保存业务域页面（dict/doc 等）暂落 `portal/data/html-pages`（门户根），
与 model 投递目录并存——待 F3-save（按 id 归属反代保存）落地后收敛。
