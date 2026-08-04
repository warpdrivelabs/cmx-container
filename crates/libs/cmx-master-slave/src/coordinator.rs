//! 主从协调器本体 —— [`CmxMasterSlave`]，前端 `CmxMasterSlave` 的 Rust 对等物。
//!
//! 持有 schema + 一棵内存树（[`MsTree`]）。业务无感知，只认路径 + 汇总规则。通过泛型
//! [`HierService`] 驱动任意后端服务——换服务 = 换 `impl`，协调器一字不改。

use cmx_rowsource::ZmcDataSet;

use crate::agg::{self, ReadResult};
use crate::changeset::{rollup_changeset, ChangeSet, SaveOutcome};
use crate::schema::HierSchema;
use crate::service::{HierService, LoadQuery};
use crate::tree::MsTree;
use crate::Result;

/// 后端主从协调器。镜像前端 `class CmxMasterSlave`（去掉视图/DOM/事件，保数据类职责）。
pub struct CmxMasterSlave {
    schema: HierSchema,
    tree: MsTree,
}

impl CmxMasterSlave {
    /// 新建协调器（对齐前端 `new CmxMasterSlave(config)`）。校验 schema 自洽。
    pub fn new(schema: HierSchema) -> Result<Self> {
        schema.validate()?;
        Ok(Self {
            schema,
            tree: MsTree::new(),
        })
    }

    /// 绑定的 schema。
    pub fn schema(&self) -> &HierSchema {
        &self.schema
    }

    /// 当前内存树。
    pub fn tree(&self) -> &MsTree {
        &self.tree
    }

    /// 以一棵 [`ZmcDataSet`] 装载数据（对齐前端 `setDataSet`）。协调器接管这棵树。
    /// 按 schema 形状分派：PathTree 用嵌套 childRows，SelfRef 用扁平自引用。
    pub fn set_data_set<R: cmx_rowsource::ZmcRowSource>(&mut self, zmc: &ZmcDataSet<R>) {
        let (path, pk) = self
            .schema
            .roots()
            .into_iter()
            .next()
            .map(|l| (l.path.clone(), l.pk.clone()))
            .unwrap_or_else(|| ("root".into(), "id".into()));
        self.tree = match &self.schema.shape {
            crate::schema::Shape::SelfRef { parent_field } => {
                MsTree::from_zmc_self_ref(zmc, &path, &pk, parent_field)
            }
            crate::schema::Shape::PathTree => MsTree::from_zmc(zmc, &path, &pk),
        };
    }

    /// 以平铺多层行装载（对齐前端 `setFlatData`）：`rows_by_path` 是 路径→该层行数组。
    /// 用 schema 的 relations（child_key）自动建父子。测试 / 非 Zmc 场景用。
    pub fn set_flat_data(
        &mut self,
        rows_by_path: &std::collections::HashMap<String, Vec<serde_json::Map<String, serde_json::Value>>>,
    ) {
        let order = self.schema.layer_order();
        let flat_layers: Vec<(String, String, Option<String>)> = order
            .iter()
            .map(|path| {
                let l = self.schema.layer(path).unwrap();
                let fk = self
                    .schema
                    .relations
                    .iter()
                    .find(|r| &r.child == path)
                    .map(|r| r.child_key.clone())
                    .or_else(|| match &self.schema.shape {
                        crate::schema::Shape::SelfRef { parent_field } => Some(parent_field.clone()),
                        _ => None,
                    });
                (path.clone(), l.pk.clone(), fk)
            })
            .collect();
        self.tree = crate::tree::from_flat(rows_by_path, &flat_layers);
    }

    /// 经服务装载整棵树（对齐前端 loadDoc/loadDict）。换服务 = 换 `svc`。
    pub async fn load_via<S: HierService>(
        &mut self,
        svc: &S,
        query: &LoadQuery,
    ) -> std::result::Result<(), String> {
        let zmc = svc.load(&self.schema, query).await?;
        self.set_data_set(&zmc);
        Ok(())
    }

    /// 经服务懒下钻某层（对齐前端 loadDictChildren）。返回子树 ZmcDataSet（调用方决定并入）。
    pub async fn expand_via<S: HierService>(
        &self,
        svc: &S,
        layer_path: &str,
        parent_ids: &[String],
    ) -> std::result::Result<ZmcDataSet<S::Row>, String> {
        svc.expand(&self.schema, layer_path, parent_ids).await
    }

    /// 保存变更集（对齐前端 saveDoc/saveDict）。
    ///
    /// **权威流程**：协调器先做写时上卷（[`rollup_changeset`]，承接字段成为权威值），
    /// 再交给服务落库。服务侧只管校验/铸号/事务，汇总由协调器裁定。
    pub async fn save_via<S: HierService>(
        &self,
        svc: &S,
        mut changes: ChangeSet,
    ) -> std::result::Result<SaveOutcome, String> {
        // 写时上卷（服务端权威）——落库前重算父层承接字段
        rollup_changeset(&self.schema, &mut changes).map_err(|e| e.to_string())?;
        svc.save(&self.schema, &changes).await
    }

    /// 读时上卷（不落库）：对当前内存树按 schema 汇总规则现算，返回各 target 值。
    pub fn rollup_read(&self) -> Result<Vec<ReadResult>> {
        agg::rollup_read(&self.tree, &self.schema.aggregations)
    }

    /// 对当前内存树内存上卷（不落库），原地回写承接字段。供预览/试算。
    pub fn rollup_in_place(&mut self) -> Result<()> {
        agg::rollup(&mut self.tree, &self.schema.aggregations)
    }

    /// 导出上卷后的平铺数据（对齐前端 `getFlatData`）：路径末段 id → 行数组。
    /// 用于跨引擎 parity 比对：与 JS `getFlatData()` 输出同结构。
    pub fn flat_data(&self) -> std::collections::HashMap<String, Vec<serde_json::Map<String, serde_json::Value>>> {
        let mut out: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
        for node in self.tree.nodes() {
            // 键用层路径的末段（= schema node id，与 JS getFlatData 一致）
            let leaf = node.path.rsplit('.').next().unwrap_or(&node.path).to_string();
            out.entry(leaf).or_default().push(node.row.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changeset::ChangeSet;
    use crate::service::LoadQuery;
    use async_trait::async_trait;
    use cmx_rowsource::{ZmcColType, ZmcDataSet, ZmcRowSource, ZmcSchema};
    use serde_json::json;
    use std::sync::Arc;

    // ── 一个最小的 mock 行 + mock 服务，验证协调器编排（不碰 DB）──

    struct MockRow {
        cols: Vec<String>,
        vals: Vec<serde_json::Value>,
    }
    impl ZmcRowSource for MockRow {
        fn col_count(&self) -> usize {
            self.cols.len()
        }
        fn col_name(&self, i: usize) -> &str {
            &self.cols[i]
        }
        fn col_type(&self, _i: usize) -> ZmcColType {
            ZmcColType::Text
        }
        fn get_bool(&self, _i: usize) -> Option<bool> {
            None
        }
        fn get_i16(&self, _i: usize) -> Option<i16> {
            None
        }
        fn get_i32(&self, _i: usize) -> Option<i32> {
            None
        }
        fn get_i64(&self, _i: usize) -> Option<i64> {
            None
        }
        fn get_f32(&self, _i: usize) -> Option<f32> {
            None
        }
        fn get_f64(&self, _i: usize) -> Option<f64> {
            None
        }
        fn get_decimal(&self, _i: usize) -> Option<rust_decimal::Decimal> {
            None
        }
        fn get_str(&self, i: usize) -> Option<&str> {
            self.vals[i].as_str()
        }
        fn get_bytes(&self, _i: usize) -> Option<&[u8]> {
            None
        }
        fn get_uuid(&self, _i: usize) -> Option<uuid::Uuid> {
            None
        }
        fn get_date(&self, _i: usize) -> Option<chrono::NaiveDate> {
            None
        }
        fn get_naive_datetime(&self, _i: usize) -> Option<chrono::NaiveDateTime> {
            None
        }
        fn get_datetime_utc(&self, _i: usize) -> Option<chrono::DateTime<chrono::Utc>> {
            None
        }
        fn get_json_value(&self, _i: usize) -> Option<serde_json::Value> {
            None
        }
    }

    struct MockSvc {
        last_saved: std::sync::Mutex<Option<ChangeSet>>,
    }

    #[async_trait]
    impl HierService for MockSvc {
        type Row = MockRow;
        async fn load(
            &self,
            _s: &HierSchema,
            _q: &LoadQuery,
        ) -> std::result::Result<ZmcDataSet<MockRow>, String> {
            // 一行 head
            let schema = Arc::new(ZmcSchema::from_parts(vec!["id".into()], vec![ZmcColType::Text]));
            let rows = vec![MockRow {
                cols: vec!["id".into()],
                vals: vec![json!("h1")],
            }];
            Ok(ZmcDataSet::with_schema("head", schema, rows))
        }
        async fn expand(
            &self,
            _s: &HierSchema,
            _p: &str,
            _ids: &[String],
        ) -> std::result::Result<ZmcDataSet<MockRow>, String> {
            let schema = Arc::new(ZmcSchema::from_parts(vec!["id".into()], vec![ZmcColType::Text]));
            Ok(ZmcDataSet::with_schema("x", schema, vec![]))
        }
        async fn save(
            &self,
            _s: &HierSchema,
            changes: &ChangeSet,
        ) -> std::result::Result<SaveOutcome, String> {
            *self.last_saved.lock().unwrap() = Some(changes.clone());
            Ok(SaveOutcome {
                affected: 1,
                ..Default::default()
            })
        }
    }

    fn schema() -> HierSchema {
        HierSchema::from_json(&json!({
            "shape": { "kind": "path_tree" },
            "layers": [
                { "path": "head", "table": "cv_header" },
                { "path": "head.items", "table": "cv_line", "child_key": "upper_id" }
            ],
            "relations": [{ "parent": "head", "child": "head.items", "child_key": "upper_id" }],
            "aggregations": [
                { "from": "head.items", "to": "head", "field": "debit", "toField": "total", "agg": "sum" }
            ]
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn load_via_service_builds_tree() {
        let svc = MockSvc {
            last_saved: std::sync::Mutex::new(None),
        };
        let mut ms = CmxMasterSlave::new(schema()).unwrap();
        ms.load_via(&svc, &LoadQuery::default()).await.unwrap();
        assert_eq!(ms.tree().collect_path("head").len(), 1);
    }

    #[tokio::test]
    async fn save_via_rolls_up_before_service() {
        let svc = MockSvc {
            last_saved: std::sync::Mutex::new(None),
        };
        let ms = CmxMasterSlave::new(schema()).unwrap();
        let cs = ChangeSet::from_json(&json!({
            "head":       { "inserted": [{"id":"h1","fields":{}}] },
            "head.items": { "inserted": [
                {"id":"i1","upper_id":"h1","fields":{"debit":100}},
                {"id":"i2","upper_id":"h1","fields":{"debit":50}}
            ]}
        }))
        .unwrap();
        let out = ms.save_via(&svc, cs).await.unwrap();
        assert_eq!(out.affected, 1);
        // 服务收到的变更集里，head.total 已被协调器上卷为 150（权威）
        let saved = svc.last_saved.lock().unwrap().clone().unwrap();
        assert_eq!(saved.layers["head"].inserted[0]["fields"]["total"], json!(150));
    }
}
