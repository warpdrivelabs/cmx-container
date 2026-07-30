//! 已废弃接口（全部注释保留，不参与编译）。
//!
//! - dict_* 系列：旧 JSON 文件字典检索引擎（前端已迁移到 `/api/dct/*`）。
//! - defs_* 系列：三元定义统一注册（前端无调用者）。
//! - launcher_catalog：功能目录（前端无调用者）。
//! - DefsQuery / DictQuery / DictIdPath / DictEntryPath：上述废弃接口的参数类型。

// ─── 以下废弃字典接口已无前端调用（路由已注释），暂时注释 handler ───
// // ───────────────────────── 字典检索引擎 ─────────────────────────
//
// /// suggest / entries 写入的 query 参数。
// #[derive(Debug, Deserialize)]
// pub struct DictQuery {
//     #[serde(default)]
//     pub q: Option<String>,
//     #[serde(default)]
//     pub rebuild: Option<String>,
// }
//
// /// 字典 id 路径。
// #[derive(Debug, Deserialize)]
// pub struct DictIdPath {
//     #[serde(rename = "dictId")]
//     pub dict_id: String,
// }
//
// /// 字典 id + 条目 id 路径。
// #[derive(Debug, Deserialize)]
// pub struct DictEntryPath {
//     #[serde(rename = "dictId")]
//     pub dict_id: String,
//     pub id: String,
// }
//
// /// `GET /api/dict/_schemas` —— schema 列表。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/registry.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_schemas(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let schemas = cmx_portal::dict::schema::list_schemas_json().await?;
//     Ok(Json(ApiResp::ok(serde_json::json!({ "schemas": schemas }))))
// }
//
// /// `POST /api/dict/_schema` —— 注册/更新 schema。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/registry.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_register_schema(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::schema::register_schema(&body).await?,
//     )))
// }
//
// /// `POST /api/dict/multi-search` —— 多字典联查。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_multi_search(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::multi::execute(&body).await?,
//     )))
// }
//
// /// `POST /api/dict/batch-data` —— 多字典内容批量加载。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_batch_data(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::api::batch_data_endpoint(&body).await?,
//     )))
// }
//
// /// `POST /api/dict/:dictId/search` —— 单字典检索。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_search(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::api::search_endpoint(&p.dict_id, &body).await?,
//     )))
// }
//
// /// `GET /api/dict/:dictId/suggest?q=` —— 自动补全。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_suggest(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
//     Query(q): Query<DictQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::api::suggest_endpoint(&p.dict_id, q.q.as_deref().unwrap_or("")).await?,
//     )))
// }
//
// /// `POST /api/dict/:dictId/entries?rebuild=` —— 写入条目。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_upsert_entries(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
//     Query(q): Query<DictQuery>,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let rebuild = q.rebuild.as_deref() == Some("true");
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::api::upsert_entries_endpoint(&p.dict_id, &body, rebuild).await?,
//     )))
// }
//
// /// `DELETE /api/dict/:dictId/entries/:id` —— 删除单条目。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_delete_entry(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictEntryPath>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::repo::delete_entry(&p.dict_id, &p.id).await?,
//     )))
// }
//
// /// `DELETE /api/dict/:dictId/entries` —— 清空条目。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_clear_entries(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::repo::clear_entries(&p.dict_id).await?,
//     )))
// }
//
// /// `POST /api/dict/:dictId/deactivate` —— 停用一个码。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_deactivate(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let code = body.get("code").and_then(|v| v.as_str()).unwrap_or("");
//     let valid_to = body.get("validTo").and_then(|v| v.as_str());
//     let successor = body.get("successorCode").and_then(|v| v.as_str());
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::write::deactivate(&p.dict_id, code, valid_to, successor).await?,
//     )))
// }
//
// /// `POST /api/dict/:dictId/supersede` —— 停旧启新。
// ///
// /// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
// pub async fn dict_supersede(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Path(p): Path<DictIdPath>,
//     Json(body): Json<serde_json::Value>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let old_code = body.get("oldCode").and_then(|v| v.as_str()).unwrap_or("");
//     let new_code = body.get("newCode").and_then(|v| v.as_str()).unwrap_or("");
//     let as_of = body.get("asOf").and_then(|v| v.as_str());
//     let new_entry = body.get("newEntry");
//     Ok(Json(ApiResp::ok(
//         cmx_portal::dict::write::supersede(&p.dict_id, old_code, new_code, as_of, new_entry)
//             .await?,
//     )))
// }
// ─── 废弃字典接口注释结束 ───

// ─── 废弃 launcher/catalog 接口（无前端调用），暂时注释 ───
// /// `GET /api/launcher/catalog` —— 全部可打开功能（轻量目录）。
// pub async fn launcher_catalog(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let items = cmx_portal::launcher::list_catalog().await?;
//     Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
// }
// ─── 废弃 launcher/catalog 注释结束 ───

// ─── DefsQuery 随 defs handler 一并注释 ───
// // ───────────────────────── 三元定义统一注册（/api/defs/*） ─────────────────────────
//
// /// `/api/defs/*` 查询参数：DAM + kind + drn（DRN 解析用）。
// #[derive(serde::Deserialize)]
// pub struct DefsQuery {
//     #[serde(default)]
//     pub domain: Option<String>,
//     #[serde(default)]
//     pub app: Option<String>,
//     #[serde(default)]
//     pub module: Option<String>,
//     #[serde(default)]
//     pub kind: Option<String>,
//     /// 单个 DRN 字符串（resolve/deps/compile 用）。
//     #[serde(default)]
//     pub drn: Option<String>,
//     /// 锚点（compile 用，形如 gl_account=1122）。
//     #[serde(default, flatten)]
//     pub rest: std::collections::HashMap<String, String>,
// }
//
// impl DefsQuery {
//     fn to_dam(&self) -> cmx_portal::flexible_combination::drn::FromDam {
//         cmx_portal::flexible_combination::drn::FromDam {
//             domain: self.domain.clone(),
//             app: self.app.clone(),
//             module: self.module.clone(),
//         }
//     }
// }

// ─── 以下废弃 defs 接口已无前端调用，暂时注释 ───
// /// `GET /api/defs/list` —— 按 kind/DAM 列出可引用定义（DCT/DOC/FLC/BASE）。
// pub async fn defs_list(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<DefsQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let items = cmx_portal::flexible_combination::defs::list(
//         q.kind.as_deref(),
//         q.domain.as_deref(),
//         q.app.as_deref(),
//         q.module.as_deref(),
//     )
//     .await?;
//     Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
// }
//
// /// `GET /api/defs/resolve?drn=…&domain&app&module` —— 解析单个 DRN → 定义全文。
// pub async fn defs_resolve(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<DefsQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let drn = q
//         .drn
//         .as_deref()
//         .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 drn 参数"))?;
//     let def = cmx_portal::flexible_combination::defs::resolve(drn, &q.to_dam()).await?;
//     Ok(Json(ApiResp::ok(def)))
// }
//
// /// `GET /api/defs/deps?drn=…` —— 某定义的直接依赖（imports/docRef/refDict → 绝对 DRN）。
// pub async fn defs_deps(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<DefsQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     let drn = q
//         .drn
//         .as_deref()
//         .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 drn 参数"))?;
//     let from = q.to_dam();
//     let def = cmx_portal::flexible_combination::defs::resolve(drn, &from).await?;
//     let deps = cmx_portal::flexible_combination::defs::dependencies_of(&def, &from);
//     Ok(Json(ApiResp::ok(serde_json::json!({
//         "drn": drn,
//         "dependencies": deps,
//     }))))
// }
//
// /// `GET /api/defs/compile?domain&app&module&scenario&<anchor>` —— FLC overlay 编译 + 按锚点解析。
// pub async fn defs_compile(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<DefsQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::flexible_combination::api::resolve(&q.to_ref(), &q.anchor_map()).await?,
//     )))
// }
// ─── 废弃 defs 接口注释结束 ───
