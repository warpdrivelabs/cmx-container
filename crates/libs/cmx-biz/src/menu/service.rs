//! Menu Service
//!
//! 封装菜单的 CRUD 与列表/分页查询逻辑。
//! create 计算标准分级字段(leaf/depth/parent_code/id_path/code_path)后写入，
//! 并更新父节点 leaf=0。

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use serde_json::Value;
use tracing::{debug, instrument};

use crate::error::{BizError, Result};
use crate::menu::{MenuBmc, MenuFilter, MenuForCreate, MenuForUpdate};

/// 把主键 `Value`(String 或其他 JSON)归一化为 `DataValue::String`。
///
/// 统一 delete/update 中对 `serde_json::Value` 主键的转换,消除重复 match。
fn value_to_datavalue(v: &Value) -> DataValue {
    match v {
        Value::String(s) => DataValue::String(s.clone()),
        _ => DataValue::String(v.to_string().trim_matches('"').to_string()),
    }
}

/// 把主键 `Value` 归一化为 `String`。
fn value_to_id_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => v.to_string().trim_matches('"').to_string(),
    }
}

/// 创建菜单时计算出的分级字段
struct TreeFields {
    parent_code: Option<String>,
    depth: i32,
    id_path: String,
    code_path: String,
}

/// 菜单服务
pub struct MenuService;

impl MenuService {
    /// 创建菜单：计算标准分级字段(leaf/depth/parent_code/id_path/code_path)后事务内写入，
    /// 并更新父节点 leaf=0。
    ///
    /// # Arguments
    /// * `txn_id` - 外部事务 ID。传 Some 时纳入调用方事务(不再自开自提交);
    ///   传 None 时内部自开事务并提交(向后兼容)
    ///
    /// # Errors
    /// 父菜单不存在、数据库写入失败时返回错误
    #[instrument(skip(mm, data))]
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: MenuForCreate,
    ) -> Result<DataSet> {
        match txn_id {
            // 外部事务:直接复用,不自开不提交
            Some(t) => Self::create_inner(mm, db_id, t, data).await,
            // 无外部事务:内部自开事务并提交(原行为)
            None => {
                let txn_ctx = mm.get_transaction_context();
                let guard = txn_ctx
                    .begin_with_guard(db_id)
                    .await
                    .map_err(|e| BizError::business(format!("开启事务失败: {e}")))?;
                let txn = guard.txn_id().to_string();
                let dataset = Self::create_inner(mm, db_id, &txn, data).await?;
                guard
                    .commit()
                    .await
                    .map_err(|e| BizError::business(format!("事务提交失败: {e}")))?;
                Ok(dataset)
            }
        }
    }

    /// create 的核心写入逻辑(INSERT 新节点 + 更新父 leaf=0),不管理事务。
    async fn create_inner(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        data: MenuForCreate,
    ) -> Result<DataSet> {
        // 预生成 id(雪花算法),供后续拼装 id_path 使用
        let id = cmx_utils::snowflake_id_str();
        // 解析父节点:parent_code 优先,回退 parent_id;得到最终 parent_id
        let parent_id =
            Self::resolve_parent_id(mm, db_id, Some(txn_id), &data.parent_id, &data.parent_code)
                .await?;
        // 计算分级字段(用真实 id 拼 id_path)
        let tree = Self::compute_tree_fields(mm, db_id, Some(txn_id), &id, &data.code, parent_id.as_deref())
            .await?;

        let definition_str = data
            .definition
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        let sql = "INSERT INTO cmx_menu \
                   (id, code, name, description, path, icon, component, sort_order, visible, \
                    open_type, fun_code, \
                    domain_code, application_code, module_code, definition, ext_attributes, status, \
                    leaf, depth, parent_id, parent_code, id_path, code_path, archived) \
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                           $20, $21, \
                           $10, $11, $12, $13::jsonb, $14, 1, \
                           1, $15, $16, $17, $18, $19, 0) \
                   RETURNING *";
        let params: Vec<DataValue> = vec![
            DataValue::String(id),
            DataValue::String(data.code.clone()),
            DataValue::String(data.name.clone()),
            data.description.clone().into(),
            data.path.clone().into(),
            data.icon.clone().into(),
            data.component.clone().into(),
            DataValue::Int(data.sort_order as i64),
            DataValue::Int(data.visible as i64),
            DataValue::String(data.domain_code.clone()),
            DataValue::String(data.application_code.clone()),
            DataValue::String(data.module_code.clone()),
            definition_str.into(),
            data.ext_attributes.clone().into(),
            DataValue::Int(tree.depth as i64),
            parent_id.clone().into(),
            tree.parent_code.clone().into(),
            DataValue::String(tree.id_path),
            DataValue::String(tree.code_path),
            DataValue::Int(data.open_type as i64),
            data.fun_code.clone().into(),
        ];
        let dataset = mm
            .query_sql_with_datavalues(db_id, Some(txn_id), sql, params, "create_menu")
            .await
            .map_err(|e| BizError::business(format!("新增菜单失败: {e}")))?;

        // 父节点 leaf = 0(有子节点后不再是叶子)
        if let Some(pid) = &parent_id {
            let upd_sql = "UPDATE cmx_menu SET leaf = 0 WHERE id = $1";
            let _ = mm
                .execute_sql_with_datavalues(
                    db_id,
                    Some(txn_id),
                    upd_sql,
                    vec![DataValue::String(pid.clone())],
                )
                .await;
        }

        Ok(dataset)
    }

    /// 解析父菜单 ID。
    ///
    /// parent_code 非空时优先按 code 查出父菜单 ID;否则回退 parent_id。
    /// 二者均为空时返回 None(根节点)。
    ///
    /// # Arguments
    /// * `txn_id` - 可选事务 ID
    /// * `data` - 创建/更新 DTO 的父关联字段
    ///
    /// # Errors
    /// parent_code 指定的父菜单不存在时返回错误
    async fn resolve_parent_id(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        parent_id: &Option<String>,
        parent_code: &Option<String>,
    ) -> Result<Option<String>> {
        if let Some(pcode) = parent_code.as_deref().filter(|s| !s.is_empty()) {
            let ds = mm
                .query_sql_with_datavalues(
                    db_id,
                    txn_id,
                    "SELECT id FROM cmx_menu WHERE code = $1",
                    vec![DataValue::String(pcode.to_string())],
                    "menu_resolve_parent_by_code",
                )
                .await
                .map_err(|e| BizError::internal(format!("按 code 查询父菜单失败: {e}")))?;
            let schema = ds.schema.as_ref();
            let pid = ds
                .iter()
                .next()
                .and_then(|row| row.get_by_name_as::<String>(schema, "id"))
                .ok_or_else(|| BizError::business(format!("父菜单不存在(code): {pcode}")))?;
            Ok(Some(pid))
        } else {
            Ok(parent_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()))
        }
    }

    /// 根据父节点计算分级字段。
    ///
    /// 根节点: depth=1, id_path=/{id}, code_path=/{code}
    /// 子节点: depth=父+1, id_path=父id_path/{id}, code_path=父code_path/{code}
    ///
    /// # Arguments
    /// * `txn_id` - 可选事务 ID
    /// * `new_id` - 预生成的新节点 ID(用于拼 id_path,保证与入库一致)
    /// * `code` - 新节点 code
    /// * `parent_id` - 解析后的父节点 ID(根节点为 None)
    ///
    /// # Errors
    /// 父菜单不存在、数据库查询失败时返回错误
    async fn compute_tree_fields(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        new_id: &str,
        code: &str,
        parent_id: Option<&str>,
    ) -> Result<TreeFields> {
        match parent_id {
            Some(pid) => {
                let (p_code, p_id_path, p_code_path, p_depth) =
                    Self::query_parent_meta(mm, db_id, txn_id, pid).await?;
                Ok(TreeFields {
                    parent_code: Some(p_code),
                    depth: p_depth + 1,
                    id_path: format!("{p_id_path}/{new_id}"),
                    code_path: format!("{p_code_path}/{code}"),
                })
            }
            None => Ok(TreeFields {
                parent_code: None,
                depth: 1,
                id_path: format!("/{new_id}"),
                code_path: format!("/{code}"),
            }),
        }
    }

    /// 查询单个菜单
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::get(mm, db_id, None, Value::String(id.to_string()))
            .await
            .map_err(Into::into)
    }

    /// 更新菜单。
    ///
    /// 当 `data.parent_id` 显式提供且与旧值不同时触发「移动」语义:
    /// 事务内重算该节点的 parent_code/depth/id_path/code_path,
    /// 并级联更新所有后代(基于 code_path/id_path 前缀替换 + depth 增量),
    /// 同步将新父 leaf 置 0、旧父 leaf 按需重置为 1。
    /// parent_id 未变更或未提供时,仅用 COALESCE 更新普通字段。
    ///
    /// # Arguments
    /// * `id` - 菜单主键(serde_json::Value,通常为 String)
    ///
    /// # Errors
    /// 菜单/新父菜单不存在、数据库执行失败时返回错误
    #[instrument(skip(mm, data))]
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: MenuForUpdate,
    ) -> Result<DataSet> {
        let menu_id = value_to_id_string(&id);

        // 开启事务(级联更新需原子性)
        let txn_ctx = mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::business(format!("开启事务失败: {e}")))?;
        let txn_id = guard.txn_id();

        // 查询当前节点 meta: parent_id / code_path / id_path / depth / code
        let meta_sql =
            "SELECT parent_id, code_path, id_path, depth, code FROM cmx_menu WHERE id = $1";
        let meta_ds = mm
            .query_sql_with_datavalues(
                db_id,
                Some(txn_id),
                meta_sql,
                vec![DataValue::String(menu_id.clone())],
                "update_menu_meta",
            )
            .await
            .map_err(|e| BizError::internal(format!("查询菜单元数据失败: {e}")))?;
        let schema = meta_ds.schema.as_ref();
        let meta_row = meta_ds.iter().next().ok_or_else(|| {
            BizError::business(format!("菜单不存在: {menu_id}"))
        })?;

        let old_parent_id =
            meta_row.get_by_name_as::<String>(schema, "parent_id");
        let old_code_path = meta_row
            .get_by_name_as::<String>(schema, "code_path")
            .unwrap_or_default();
        let old_id_path = meta_row
            .get_by_name_as::<String>(schema, "id_path")
            .unwrap_or_default();
        let old_depth = meta_row
            .get_by_name_as::<i64>(schema, "depth")
            .unwrap_or(1) as i32;
        let menu_code = meta_row
            .get_by_name_as::<String>(schema, "code")
            .unwrap_or_default();

        // 规范化:空字符串视为 None(根节点)
        let old_parent_norm = old_parent_id.as_deref().filter(|s| !s.is_empty());
        // 解析新父:parent_code 优先(查出 id),回退 parent_id;两者均空则成为根节点
        let parent_provided = data.parent_id.is_some() || data.parent_code.is_some();
        let resolved_new_parent = if parent_provided {
            Self::resolve_parent_id(mm, db_id, Some(txn_id), &data.parent_id, &data.parent_code)
                .await?
        } else {
            None
        };
        let new_parent_norm = resolved_new_parent.as_deref();
        // 仅当显式提供 parent_id/parent_code 且解析结果与旧值不同时才级联移动
        let parent_changed = parent_provided && new_parent_norm != old_parent_norm;
        let old_parent_for_recompute = old_parent_norm.map(|s| s.to_string());

        let dataset = if parent_changed {
            // ---- parent_id 变更:级联重算 ----
            // 查新父 meta,计算新 id_path / code_path / depth / parent_code
            let (new_parent_code, new_id_path, new_code_path, new_depth) = if let Some(new_pid) =
                new_parent_norm
            {
                let p_meta = Self::query_parent_meta(mm, db_id, Some(txn_id), new_pid).await?;
                let (p_code, p_id_path, p_code_path, p_depth) = p_meta;
                (
                    Some(p_code),
                    format!("{p_id_path}/{menu_id}"),
                    format!("{p_code_path}/{menu_code}"),
                    p_depth + 1,
                )
            } else {
                (
                    None,
                    format!("/{menu_id}"),
                    format!("/{menu_code}"),
                    1,
                )
            };

            // 级联更新该节点及其后代:code_path/id_path 前缀替换 + depth 增量
            // (旧前缀 old_code_path/old_id_path → 新前缀 new_code_path/new_id_path)
            // 注:SUBSTRING(code_path FROM $3) 的 $3 用 old_path.len()+1(字节数)。
            //     code_path 仅由 UUID/菜单编码(ASCII)与 '/' 组成,字节数 == 字符数,故安全。
            let code_cascade =
                "UPDATE cmx_menu SET code_path = $2 || SUBSTRING(code_path FROM $3) \
                 WHERE code_path = $1 OR code_path LIKE ($1 || '/%')";
            mm.execute_sql_with_datavalues(
                db_id,
                Some(txn_id),
                code_cascade,
                vec![
                    DataValue::String(old_code_path.clone()),
                    DataValue::String(new_code_path.clone()),
                    DataValue::Int(old_code_path.len() as i64 + 1),
                ],
            )
            .await
            .map_err(|e| BizError::business(format!("级联更新 code_path 失败: {e}")))?;

            let id_cascade =
                "UPDATE cmx_menu SET id_path = $2 || SUBSTRING(id_path FROM $3) \
                 WHERE id_path = $1 OR id_path LIKE ($1 || '/%')";
            mm.execute_sql_with_datavalues(
                db_id,
                Some(txn_id),
                id_cascade,
                vec![
                    DataValue::String(old_id_path.clone()),
                    DataValue::String(new_id_path.clone()),
                    DataValue::Int(old_id_path.len() as i64 + 1),
                ],
            )
            .await
            .map_err(|e| BizError::business(format!("级联更新 id_path 失败: {e}")))?;

            // 后代 depth 增量(自身 + 后代 depth 同步偏移)
            let depth_cascade =
                "UPDATE cmx_menu SET depth = depth + ($2 - $3) \
                 WHERE id_path = $1 OR id_path LIKE ($1 || '/%')";
            mm.execute_sql_with_datavalues(
                db_id,
                Some(txn_id),
                depth_cascade,
                vec![
                    DataValue::String(new_id_path.clone()),
                    DataValue::Int(new_depth as i64),
                    DataValue::Int(old_depth as i64),
                ],
            )
            .await
            .map_err(|e| BizError::business(format!("级联更新 depth 失败: {e}")))?;

            // 更新该节点 parent_id/parent_code/id_path/code_path/depth + 普通字段,RETURNING *
            // 注:id_path/code_path 必须包含自身、永不为空,此处显式写入重算值(级联已处理后代)
            let upd_sql = "UPDATE cmx_menu SET \
                parent_id = $1, parent_code = $2, id_path = $3, code_path = $4, depth = $5, \
                name = COALESCE($6, name), \
                description = COALESCE($7, description), \
                path = COALESCE($8, path), \
                icon = COALESCE($9, icon), \
                component = COALESCE($10, component), \
                sort_order = COALESCE($11, sort_order), \
                visible = COALESCE($12, visible), \
                open_type = COALESCE($13, open_type), \
                fun_code = COALESCE($14, fun_code), \
                status = COALESCE($15, status), \
                ext_attributes = COALESCE($16, ext_attributes), \
                update_time = NOW() \
                WHERE id = $17 RETURNING *";
            let params = vec![
                new_parent_norm.map(|s| s.to_string()).into(),
                new_parent_code.into(),
                DataValue::String(new_id_path.clone()),
                DataValue::String(new_code_path.clone()),
                DataValue::Int(new_depth as i64),
                data.name.clone().into(),
                data.description.clone().into(),
                data.path.clone().into(),
                data.icon.clone().into(),
                data.component.clone().into(),
                data.sort_order.into(),
                data.visible.into(),
                data.open_type.into(),
                data.fun_code.clone().into(),
                data.status.into(),
                data.ext_attributes.clone().into(),
                DataValue::String(menu_id.clone()),
            ];
            let ds = mm
                .query_sql_with_datavalues(db_id, Some(txn_id), upd_sql, params, "update_menu")
                .await
                .map_err(|e| BizError::business(format!("更新菜单失败: {e}")))?;

            // 新父 leaf = 0
            if let Some(new_pid) = new_parent_norm {
                let _ = mm
                    .execute_sql_with_datavalues(
                        db_id,
                        Some(txn_id),
                        "UPDATE cmx_menu SET leaf = 0 WHERE id = $1",
                        vec![DataValue::String(new_pid.to_string())],
                    )
                    .await;
            }
            ds
        } else {
            // ---- parent_id 未变更:仅更新普通字段,并兜底重算自身路径 ----
            // id_path/code_path 必须包含自身、永不为空(兜底存量脏数据);parent 未变故基于 old_parent 重算
            let (cur_id_path, cur_code_path) = if let Some(old_pid) = old_parent_norm {
                let (p_code, p_id_path, p_code_path, _) =
                    Self::query_parent_meta(mm, db_id, Some(txn_id), old_pid).await?;
                let _ = p_code;
                (
                    format!("{p_id_path}/{menu_id}"),
                    format!("{p_code_path}/{menu_code}"),
                )
            } else {
                (format!("/{menu_id}"), format!("/{menu_code}"))
            };
            let upd_sql = "UPDATE cmx_menu SET \
                id_path = $1, code_path = $2, \
                name = COALESCE($3, name), \
                description = COALESCE($4, description), \
                path = COALESCE($5, path), \
                icon = COALESCE($6, icon), \
                component = COALESCE($7, component), \
                sort_order = COALESCE($8, sort_order), \
                visible = COALESCE($9, visible), \
                open_type = COALESCE($10, open_type), \
                fun_code = COALESCE($11, fun_code), \
                status = COALESCE($12, status), \
                ext_attributes = COALESCE($13, ext_attributes), \
                update_time = NOW() \
                WHERE id = $14 RETURNING *";
            let params = vec![
                DataValue::String(cur_id_path),
                DataValue::String(cur_code_path),
                data.name.clone().into(),
                data.description.clone().into(),
                data.path.clone().into(),
                data.icon.clone().into(),
                data.component.clone().into(),
                data.sort_order.into(),
                data.visible.into(),
                data.open_type.into(),
                data.fun_code.clone().into(),
                data.status.into(),
                data.ext_attributes.clone().into(),
                DataValue::String(menu_id.clone()),
            ];
            mm.query_sql_with_datavalues(db_id, Some(txn_id), upd_sql, params, "update_menu")
                .await
                .map_err(|e| BizError::business(format!("更新菜单失败: {e}")))?
        };

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| BizError::business(format!("事务提交失败: {e}")))?;

        // 旧父重算 leaf(提交后执行,使用 None txn_id)
        if parent_changed && let Some(ref old_pid) = old_parent_for_recompute {
            Self::recompute_parent_leaf(mm, db_id, old_pid).await;
        }

        Ok(dataset)
    }

    /// 删除菜单。
    ///
    /// 级联删除传入节点的所有后代(基于 code_path 前缀匹配,避免产生孤儿节点),
    /// 删除后重置各父节点 leaf(若无其他子节点则置 1)。
    ///
    /// # Arguments
    /// * `ids` - 待删菜单主键列表
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        if ids.is_empty() {
            let schema = std::sync::Arc::new(
                cmx_core::model::data::dataset::Schema::new("cmx_menu", vec![])
                    .expect("空 schema 构造不应失败"),
            );
            return Ok(DataSet::empty("menu_delete", schema));
        }

        let txn_ctx = mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::business(format!("开启事务失败: {e}")))?;
        let txn_id = guard.txn_id();

        // 一次查询拿到 parent_id 与 code_path 两列(合并原两次查询,2 次往返 → 1 次)
        // parent_id:仅非根(用于删除后 leaf 重置);code_path:所有待删节点(用于级联删后代)
        let id_placeholders: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect();
        let id_params: Vec<DataValue> = ids.iter().map(value_to_datavalue).collect();
        let meta_sql = format!(
            "SELECT parent_id, code_path FROM cmx_menu WHERE id IN ({})",
            id_placeholders.join(", ")
        );
        let meta_ds = mm
            .query_sql_with_datavalues(db_id, Some(txn_id), &meta_sql, id_params, "menu_delete_meta")
            .await
            .map_err(|e| BizError::internal(format!("查询待删菜单元数据失败: {e}")))?;
        let m_schema = meta_ds.schema.as_ref();
        // parent_id 去重(仅非空);code_path 去重(仅非空)
        let parent_ids: Vec<String> = {
            let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
            for row in meta_ds.iter() {
                if let Some(pid) = row.get_by_name_as::<String>(m_schema, "parent_id")
                    && !pid.is_empty()
                {
                    set.insert(pid);
                }
            }
            set.into_iter().collect()
        };
        let code_paths: Vec<String> = {
            let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
            for row in meta_ds.iter() {
                if let Some(cp) = row.get_by_name_as::<String>(m_schema, "code_path")
                    && !cp.is_empty()
                {
                    set.insert(cp);
                }
            }
            set.into_iter().collect()
        };

        // 级联删除:删除传入节点及其所有后代(code_path 前缀匹配)
        let root_ids: Vec<DataValue> = ids.iter().map(value_to_datavalue).collect();

        let dataset = if code_paths.is_empty() {
            // 无 code_path 信息(可能是叶子),直接按 id 删
            let sql = format!(
                "DELETE FROM cmx_menu WHERE id IN ({})",
                id_placeholders.join(", ")
            );
            mm.query_sql_with_datavalues(db_id, Some(txn_id), &sql, root_ids, "menu_delete")
                .await
                .map_err(|e| BizError::business(format!("删除菜单失败: {e}")))?
        } else {
            // 删传入节点本身 + 它们的后代子树
            // 后代:对每个 code_path,匹配 code_path = X 或 code_path LIKE 'X/%'
            let mut descendant_conditions: Vec<String> = Vec::new();
            let mut descendant_params: Vec<DataValue> = root_ids.clone();
            for (i, cp) in code_paths.iter().enumerate() {
                let idx = ids.len() + 1 + i;
                descendant_conditions.push(format!(
                    "code_path = ${idx} OR code_path LIKE (${idx} || '/%')"
                ));
                descendant_params.push(DataValue::String(cp.clone()));
            }
            let del_sql = format!(
                "DELETE FROM cmx_menu WHERE id IN ({}) OR ({})",
                id_placeholders.join(", "),
                descendant_conditions.join(" OR ")
            );
            mm.query_sql_with_datavalues(
                db_id,
                Some(txn_id),
                &del_sql,
                descendant_params,
                "menu_delete_cascade",
            )
            .await
            .map_err(|e| BizError::business(format!("级联删除菜单失败: {e}")))?
        };

        guard
            .commit()
            .await
            .map_err(|e| BizError::business(format!("事务提交失败: {e}")))?;

        // 批量重置父节点 leaf(单 SQL,N 次往返 → 1 次;提交后执行)
        if !parent_ids.is_empty() {
            Self::recompute_parents_leaf(mm, db_id, &parent_ids).await;
        }

        Ok(dataset)
    }

    /// 查询父节点元数据(code/id_path/code_path/depth),用于计算子节点分级字段。
    ///
    /// # Arguments
    /// * `txn_id` - 可选事务 ID
    /// * `parent_id` - 父节点主键
    ///
    /// # Returns
    /// 元组 (code, id_path, code_path, depth),父不存在时返回错误
    ///
    /// # Errors
    /// 父菜单不存在、数据库查询失败时返回错误
    async fn query_parent_meta(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        parent_id: &str,
    ) -> Result<(String, String, String, i32)> {
        let sql =
            "SELECT code, id_path, code_path, depth FROM cmx_menu WHERE id = $1";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                txn_id,
                sql,
                vec![DataValue::String(parent_id.to_string())],
                "menu_parent_meta",
            )
            .await
            .map_err(|e| BizError::internal(format!("查询父菜单元数据失败: {e}")))?;
        let schema = ds.schema.as_ref();
        let row = ds.iter().next().ok_or_else(|| {
            BizError::business(format!("父菜单不存在: {parent_id}"))
        })?;
        Ok((
            row.get_by_name_as::<String>(schema, "code")
                .unwrap_or_default(),
            row.get_by_name_as::<String>(schema, "id_path")
                .unwrap_or_default(),
            row.get_by_name_as::<String>(schema, "code_path")
                .unwrap_or_default(),
            row.get_by_name_as::<i64>(schema, "depth").unwrap_or(1) as i32,
        ))
    }

    /// 重算父节点 leaf:若无其他子节点则置 1。
    ///
    /// 在事务提交后调用(使用 None txn_id)。
    async fn recompute_parent_leaf(mm: &DatabaseManager, db_id: &str, parent_id: &str) {
        let _ = mm
            .execute_sql_with_datavalues(
                db_id,
                None,
                "UPDATE cmx_menu SET leaf = 1 \
                 WHERE id = $1 AND NOT EXISTS \
                 (SELECT 1 FROM cmx_menu WHERE parent_id = $1)",
                vec![DataValue::String(parent_id.to_string())],
            )
            .await;
    }

    /// 批量重算多个父节点 leaf:对每个父节点,若无其他子节点则置 1。
    ///
    /// 单条 SQL 完成,替代逐个 `recompute_parent_leaf` 的 N 次往返。
    /// 在事务提交后调用(使用 None txn_id)。
    async fn recompute_parents_leaf(mm: &DatabaseManager, db_id: &str, parent_ids: &[String]) {
        if parent_ids.is_empty() {
            return;
        }
        // $1 为待重算的父 id 数组;对每个父 id:若已无子节点(无行的 parent_id 指向它),则 leaf = 1
        let arr: DataValue = parent_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect::<Vec<DataValue>>()
            .into();
        let _ = mm
            .execute_sql_with_datavalues(
                db_id,
                None,
                "UPDATE cmx_menu SET leaf = 1 \
                 WHERE id = ANY($1) AND NOT EXISTS \
                 (SELECT 1 FROM cmx_menu sub WHERE sub.parent_id = cmx_menu.id)",
                vec![arr],
            )
            .await;
    }

    /// 按 code 删除菜单(幂等安装用,不存在时静默成功)
    ///
    /// # Arguments
    /// * `txn_id` - 外部事务 ID(传 Some 时纳入调用方事务;传 None 时自动提交)
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete_by_code(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        code: &str,
    ) -> Result<()> {
        use cmx_core::model::cell::DataValue;
        mm.execute_sql_with_datavalues(
            db_id,
            txn_id,
            "DELETE FROM cmx_menu WHERE code = $1",
            vec![DataValue::String(code.to_string())],
        )
        .await
        .map_err(|e| crate::error::BizError::business(format!("按 code 删除菜单失败: {e}")))?;
        Ok(())
    }

    /// 按模块编码删除全部菜单(幂等导入前清理用,物理删,不走 leaf 重算)。
    ///
    /// # Arguments
    /// * `txn_id` - 可选事务 ID(传 Some 时纳入调用方事务;传 None 时自动提交)
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete_by_module(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        module_code: &str,
    ) -> Result<()> {
        mm.execute_sql_with_datavalues(
            db_id,
            txn_id,
            "DELETE FROM cmx_menu WHERE module_code = $1",
            vec![DataValue::String(module_code.to_string())],
        )
        .await
        .map_err(|e| crate::error::BizError::business(format!("按模块删除菜单失败: {e}")))?;
        Ok(())
    }

    /// 按模块编码查询全部菜单节点(供模块导出复用,返回结构化 MenuDefinition 列表)。
    ///
    /// 模式A:一节点一行。查询模块下全部节点(含子节点,不限 parent_id),
    /// 每行一个 MenuDefinition,按 depth/sort_order 排序(父先于子,便于消费方)。
    /// 所有业务字段作为一等字段查询返回;树形衍生字段(id/id_path/code_path/leaf/depth)不导出。
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn list_by_module(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
    ) -> Result<Vec<cmx_core::model::module::MenuDefinition>> {
        let sql = "SELECT code, name, parent_code, description, path, icon, component, \
                   sort_order, visible, open_type, fun_code, definition, ext_attributes, \
                   domain_code, application_code, module_code \
                   FROM cmx_menu WHERE module_code = $1 AND archived = 0 \
                   ORDER BY depth, sort_order";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "menu_list_by_module",
            )
            .await
            .map_err(|e| BizError::internal(format!("按模块查询菜单失败: {e}")))?;
        let schema = ds.schema.as_ref();
        let mut result = Vec::new();
        for row in ds.iter() {
            let get = |name: &str| -> Option<String> { row.get_by_name_as(schema, name) };
            let get_i32 = |name: &str, default: i32| {
                row.get_by_name_as::<i64>(schema, name).map(|v| v as i32).unwrap_or(default)
            };
            // definition:节点自身额外自定义数据(JSONB),整体透传
            let definition = row
                .get_by_name_as::<serde_json::Value>(schema, "definition")
                .or_else(|| {
                    row.get_by_name_as::<String>(schema, "definition")
                        .and_then(|s| serde_json::from_str(&s).ok())
                })
                .map(cmx_utils::json::coerce_to_object)
                .filter(|v| !v.is_null());
            result.push(cmx_core::model::module::MenuDefinition {
                code: get("code").unwrap_or_default(),
                name: get("name").unwrap_or_default(),
                parent_code: get("parent_code"),
                description: get("description"),
                path: get("path"),
                icon: get("icon"),
                component: get("component"),
                sort_order: get_i32("sort_order", 0),
                visible: get_i32("visible", 1),
                open_type: get_i32("open_type", 0),
                fun_code: get("fun_code"),
                definition,
                ext_attributes: get("ext_attributes"),
                children: Vec::new(),
                domain_code: get("domain_code").unwrap_or_default(),
                application_code: get("application_code").unwrap_or_default(),
                module_code: get("module_code").unwrap_or_default(),
            });
        }
        Ok(result)
    }

    /// 列表查询
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<MenuBmc, MenuFilter>::list(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 分页查询
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<MenuBmc, MenuFilter>::page(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 查询菜单树(按域/应用/模块过滤,组装为树形结构)
    ///
    /// 参照 DomainService::get_tree / PermissionService::get_permission_tree 模式:
    /// 查全量扁平数据 → 转 MenuTreeNodeData → TreeNode::from_list 组装。
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn get_tree(
        mm: &DatabaseManager,
        db_id: &str,
        domain_code: Option<&str>,
        application_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<cmx_api_types::TreeNode<crate::menu::MenuTreeNodeData>>> {
        debug!("{:<12} - MenuService::get_tree", "SERVICE");

        // 动态构建 WHERE(可选过滤)
        let mut conditions: Vec<String> = vec!["archived = 0".to_string()];
        let mut params: Vec<DataValue> = Vec::new();
        let mut idx = 1;
        if let Some(dc) = domain_code {
            conditions.push(format!("domain_code = ${idx}"));
            params.push(DataValue::String(dc.to_string()));
            idx += 1;
        }
        if let Some(ac) = application_code {
            conditions.push(format!("application_code = ${idx}"));
            params.push(DataValue::String(ac.to_string()));
            idx += 1;
        }
        if let Some(mc) = module_code {
            conditions.push(format!("module_code = ${idx}"));
            params.push(DataValue::String(mc.to_string()));
        }
        let where_clause = conditions.join(" AND ");

        let sql = format!(
            "SELECT id, code, name, description, path, icon, component, sort_order, visible, \
             open_type, fun_code, \
             depth, parent_id, parent_code, domain_code, application_code, module_code, definition, ext_attributes \
             FROM cmx_menu WHERE {where_clause} ORDER BY sort_order"
        );

        let dataset = mm
            .query_sql_with_datavalues(db_id, None, &sql, params, "menu_tree")
            .await
            .map_err(|e| BizError::internal(format!("查询菜单树形数据失败: {e}")))?;

        let items: Vec<crate::menu::MenuTreeNodeData> = dataset
            .iter()
            .map(|row| Self::row_to_tree_node(row, &dataset.schema))
            .collect::<Result<Vec<_>>>()?;

        Ok(cmx_api_types::TreeNode::from_list(items))
    }

    /// 将 DataSet 一行转换为 MenuTreeNodeData
    fn row_to_tree_node(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> Result<crate::menu::MenuTreeNodeData> {
        let get_str = |name: &str| -> Option<String> { row.get_by_name_as(schema, name) };
        let get_i32 = |name: &str| -> i32 { row.get_by_name_as::<i32>(schema, name).unwrap_or(0) };
        let _ = get_str;
        let _ = get_i32;
        Ok(crate::menu::MenuTreeNodeData {
            id: get_str("id").unwrap_or_default(),
            parent_id: get_str("parent_id"),
            code: get_str("code").unwrap_or_default(),
            name: get_str("name").unwrap_or_default(),
            parent_code: get_str("parent_code"),
            description: get_str("description"),
            path: get_str("path"),
            icon: get_str("icon"),
            component: get_str("component"),
            sort_order: get_i32("sort_order"),
            visible: get_i32("visible"),
            open_type: get_i32("open_type"),
            fun_code: get_str("fun_code"),
            depth: get_i32("depth"),
            domain_code: get_str("domain_code").unwrap_or_default(),
            application_code: get_str("application_code").unwrap_or_default(),
            module_code: get_str("module_code").unwrap_or_default(),
            definition: row
                .get_by_name_as::<serde_json::Value>(schema, "definition")
                .or_else(|| {
                    get_str("definition")
                        .and_then(|s| serde_json::from_str(&s).ok())
                }),
            ext_attributes: get_str("ext_attributes"),
        })
    }
}
