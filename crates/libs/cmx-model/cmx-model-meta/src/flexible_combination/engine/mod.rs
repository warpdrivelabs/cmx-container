//! FlexibleCombination 运行时引擎（复刻 `flexible-combination-engine.js`）。
//!
//! 核心：`resolve_merged_rule`（锚点评分 + 多规则字段合并）、`build_columns`（_fieldToColumn 全派生 →
//! CmxColumn.toJSON 形状）、`build_members`（分组 CmxColumnGroup）、`build_column_model_props`。
//!
//! 运行时闭包（calcFormula/onPickDimension/recompute）在 JSON 序列化时为 null，故此处不实现，
//! 仅产出与 Node `CmxColumn.toJSON()` 等价的可序列化结果。

mod column;
mod group;

use serde_json::{Map, Value, json};

/// 引擎：持有 dimensions + rules（可选 DRN 引用上下文）。
pub struct Engine<'a> {
    /// 维度定义表（code → 维度对象）。
    dimensions: &'a Value,
    /// 规则列表（已克隆，可重排）。
    rules: Vec<Value>,
    /// 引用方 DAM（DRN 别名/相对引用补全继承段用）；缺省为空 DAM。
    ref_from: crate::flexible_combination::drn::FromDam,
    /// 顶层 imports（DRN 别名表）。
    ref_imports: Option<Value>,
}

/// 两值「相等」（数字/字符串宽松比较，与 Node sameValue 一致）。
///
/// 把标量统一转为字符串后比较，使数字 `1` 与字符串 `"1"` 视为相等。
pub(super) fn same_value(a: &Value, b: &Value) -> bool {
    fn norm(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }
    norm(a) == norm(b)
}

/// 取字段 id（归一为字符串）。
pub(super) fn field_id(field: &Value) -> String {
    field.get("id").map(value_to_string).unwrap_or_default()
}

/// 跨规则合并字段列表：位置取首次出现，值取得分高者，同分保留先到者（lists 需按定义顺序传入）。
///
/// 与 JS 侧 `FlexibleCombinationEngine._mergeFieldLists` 语义一致。
fn merge_field_lists(lists: &[(Vec<Value>, f64)]) -> Vec<Value> {
    let mut by_code: std::collections::HashMap<String, (Value, f64, usize)> =
        std::collections::HashMap::new();
    let mut pos = 0usize;
    for (fields, score) in lists {
        for f in fields {
            let id = field_id(f);
            if id.is_empty() {
                continue;
            }
            match by_code.get(&id) {
                None => {
                    by_code.insert(id, (f.clone(), *score, pos));
                    pos += 1;
                }
                Some((_, prev_score, prev_pos)) => {
                    if *score > *prev_score {
                        let p = *prev_pos;
                        by_code.insert(id, (f.clone(), *score, p));
                    }
                }
            }
        }
    }
    let mut entries: Vec<(Value, f64, usize)> = by_code.into_values().collect();
    entries.sort_by_key(|a| a.2);
    entries.into_iter().map(|x| x.0).collect()
}

/// 将 JSON 标量值归一为字符串（字符串/数字/布尔原样转，null 与其余返回空串）。
pub(super) fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 当前界面语言候选（服务端固定 zh_CN 链）。
///
/// caption 为对象时按 zh_CN/zh-CN/zh/default/任意非空值顺序取；字符串直接用；否则回退 display name。
pub(super) fn field_caption(field: &Value) -> String {
    let cap = field.get("caption");
    match cap {
        Some(Value::Object(m)) => {
            // 按语言候选优先级取首个非空值
            for k in ["zh_CN", "zh-CN", "zh", "default"] {
                if let Some(s) = m.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    return s.to_string();
                }
            }
            // 候选都未命中：取任意非空值
            for v in m.values() {
                if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
                    return s.to_string();
                }
            }
            field_display_name(field)
        }
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => field_display_name(field),
    }
}

/// 提取锚点维度的层级路径（`<dim>.__path`，逗号分隔字符串或数组 → 字符串向量）。
///
/// 供 `$under` 层级泛化匹配：调用方选中树形字典某级值时，把其祖先链（含自身，
/// 如 `"2,2221,222101"`）一并传入锚点，规则即可用 `{"$under": "2221"}` 命中子孙值。
pub(super) fn anchor_path_values(anchor: &Map<String, Value>, base_dim: &str) -> Vec<String> {
    let Some(v) = anchor.get(&format!("{base_dim}.__path")) else {
        return Vec::new();
    };
    match v {
        Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        Value::Array(arr) => arr
            .iter()
            .map(value_to_string)
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// 取字段展示名：按 name/label/fieldName/code 顺序取首个非空，否则回退 id。
pub(super) fn field_display_name(field: &Value) -> String {
    for k in ["name", "label", "fieldName", "code"] {
        if let Some(s) = field
            .get(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return s.to_string();
        }
    }
    field_id(field)
}

/// 判定数据类型是否为数值型（INT/BIGINT/TINYINT/DECIMAL/NUMBER）。
pub(super) fn is_numeric_type(dt: &str) -> bool {
    matches!(
        dt.to_uppercase().as_str(),
        "INT" | "BIGINT" | "TINYINT" | "DECIMAL" | "NUMBER"
    )
}

/// 推断字段数据类型（与 Node `_fieldDataType` 一致）。
///
/// field.dataType 为非空真值才用；否则 measure 维度默认 DECIMAL，其余默认 VARCHAR。
pub(super) fn field_data_type(field: &Value) -> String {
    // 与 Node `_fieldDataType` 一致：field.dataType 为「真值」才用（空串视为缺省）。
    if let Some(dt) = field
        .get("dataType")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return dt.to_string();
    }
    // measure 维度默认 DECIMAL
    if field.get("dimType").and_then(|v| v.as_str()) == Some("measure") {
        return "DECIMAL".to_string();
    }
    "VARCHAR".to_string()
}

impl<'a> Engine<'a> {
    pub fn new(dimensions: &'a Value, rules: &'a Value) -> Engine<'a> {
        Engine {
            dimensions,
            // rules 非数组时视为空规则集
            rules: rules.as_array().cloned().unwrap_or_default(),
            ref_from: crate::flexible_combination::drn::FromDam::default(),
            ref_imports: None,
        }
    }

    /// 注入 DRN 引用上下文（引用方 DAM + imports 别名表），使字段 refDict 支持
    /// `@别名` / `drn:…` / `DCT/x` 写法归一为有效 dictId。链式调用。
    pub fn with_ref_context(
        mut self,
        from: crate::flexible_combination::drn::FromDam,
        imports: Option<Value>,
    ) -> Self {
        self.ref_from = from;
        self.ref_imports = imports;
        self
    }

    /// 归一字段 refDict → 有效 dict/维度 code（裸 code 原样，DRN/别名展开取 name）。
    fn effective_ref(&self, raw: &str) -> String {
        crate::flexible_combination::drn::effective_dict_id(
            raw,
            &self.ref_from,
            self.ref_imports.as_ref(),
        )
    }

    /// 按 code 取维度定义（须为对象才算有效）。
    fn get_dimension(&self, code: &str) -> Option<&Value> {
        self.dimensions.get(code).filter(|v| v.is_object())
    }

    /// 在维度内置 values 找 code 对应值对象，找不到返回 { code }。
    fn resolve_dim_value(&self, dim_code: &str, code: &Value) -> Value {
        if let Some(dim) = self.get_dimension(dim_code)
            && let Some(list) = dim.get("values").and_then(|v| v.as_array())
        {
            // 在维度内置值列表里按 code 匹配
            for v in list {
                if same_value(v.get("code").unwrap_or(&Value::Null), code) {
                    return v.clone();
                }
            }
        }
        // 未找到：返回仅含 code 的占位对象
        json!({ "code": code })
    }

    // ── 规则匹配评分 ──────────────────────────────────────────────

    /// 返回命中规则 (rule, specificity, order)，按具体度降序、同分保持顺序。
    ///
    /// specificity = score * 100 + dims.len()（命中的锚点维度越多越具体）。
    fn matched_rules(&self, anchor: &Map<String, Value>) -> Vec<(Value, f64, usize)> {
        let mut out: Vec<(Value, f64, usize)> = Vec::new();
        let mut order = 0usize;
        for r in &self.rules {
            // 规则声明的锚点维度必须全部出现在 anchor 中才算候选
            let dims: Vec<&str> = r
                .get("anchor")
                .and_then(|a| a.get("dimensions"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            if !dims.iter().all(|d| anchor.contains_key(*d)) {
                continue;
            }
            let match_obj = r.get("anchor").and_then(|a| a.get("match"));
            // score < 0 表示不匹配，跳过
            let score = self.score_match(match_obj, anchor);
            if score < 0.0 {
                continue;
            }
            // 具体度 = match 评分 * 100 + 命中维度数
            let specificity = score * 100.0 + dims.len() as f64;
            out.push((r.clone(), specificity, order));
            order += 1;
        }
        // 按具体度降序，同分按声明顺序升序（稳定排序）
        out.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.cmp(&b.2))
        });
        out
    }

    /// 评分 match 对象：任一维度条件不满足返回 -1，否则返回各维度得分之和。
    fn score_match(&self, match_obj: Option<&Value>, anchor: &Map<String, Value>) -> f64 {
        let Some(Value::Object(m)) = match_obj else {
            return 0.0;
        };
        // 空 match 对象视为通配，得 0 分
        if m.is_empty() {
            return 0.0;
        }
        let mut score = 0.0;
        for (dim, cond) in m {
            // dim 可能形如 "gl_account.account_type"（属性路径）
            let (base_dim, _attr_path) = dim
                .split_once('.')
                .map(|(a, b)| (a, Some(b)))
                .unwrap_or((dim.as_str(), None));
            // anchor 缺该维度值（或为 null）→ 不匹配
            let code = match anchor.get(base_dim) {
                Some(v) if !v.is_null() => v,
                _ => return -1.0,
            };
            let item = self.score_condition(dim, base_dim, code, cond, anchor);
            if item < 0.0 {
                return -1.0;
            }
            score += item;
        }
        score
    }

    /// 评分单条件。dim 可能带属性路径（"gl_account.account_type"）。
    ///
    /// anchor 仅用于维度根条件的 `$under`（读 `<dim>.__path` 层级路径）；属性路径条件不感知层级。
    fn score_condition(
        &self,
        dim_full: &str,
        base_dim: &str,
        code: &Value,
        cond: &Value,
        anchor: &Map<String, Value>,
    ) -> f64 {
        // 属性路径条件：match 键含点 → 取维度值的该属性比对
        if let Some((_, attr)) = dim_full.split_once('.') {
            let val = self.resolve_dim_value(base_dim, code);
            // code 属性直接用锚点值，其余属性从维度值对象取
            let attr_val = if attr == "code" {
                code.clone()
            } else {
                val.get(attr).cloned().unwrap_or(Value::Null)
            };
            let s = self.score_value_condition(&attr_val, cond);
            // 属性路径匹配最低计 1 分
            return if s < 0.0 { -1.0 } else { s.max(1.0) };
        }
        match cond {
            // 通配 "*" 或 null：弱匹配（0.25 分）
            Value::String(s) if s == "*" => 0.25,
            Value::Null => 0.25,
            // 数组：命中任一元素得 1.5 分，否则不匹配
            Value::Array(arr) => {
                if arr.iter().any(|v| same_value(code, v)) {
                    1.5
                } else {
                    -1.0
                }
            }
            Value::Object(o) => {
                // 含 $ 操作符 → 走操作符评分（携带锚点层级路径，供 $under）
                if o.keys().any(|k| k.starts_with('$')) {
                    let path = anchor_path_values(anchor, base_dim);
                    self.score_operator_condition(code, o, &path)
                } else {
                    // 对象形式：多属性条件，解析维度值后逐属性评分
                    let val = self.resolve_dim_value(base_dim, code);
                    let mut score = 0.0;
                    for (attr, want) in o {
                        let attr_val = if attr == "code" {
                            code.clone()
                        } else {
                            val.get(attr).cloned().unwrap_or(Value::Null)
                        };
                        let s = self.score_value_condition(&attr_val, want);
                        if s < 0.0 {
                            return -1.0;
                        }
                        // 通配（0 分）按 1 分计入，强化属性匹配权重
                        score += if s == 0.0 { 1.0 } else { s };
                    }
                    score
                }
            }
            // 标量精确匹配：得 3 分，否则不匹配
            other => {
                if same_value(code, other) {
                    3.0
                } else {
                    -1.0
                }
            }
        }
    }

    /// 评分 $ 操作符条件（$exists/$eq/$ne/$in/$nin/$under）。
    ///
    /// 任一操作符不满足返回 -1；最终得分按 $eq(3) > $in/$under(1.5) > 其余(1)。
    ///
    /// `$under`：层级泛化匹配——锚点值等于目标值，或锚点层级路径（`<dim>.__path`
    /// 传入的祖先链）包含目标值时命中。用于树形维度（如科目 L1/L2/L3）按祖先配置
    /// 规则、选中子孙值时联动命中。path 由维度根条件提取；属性路径条件传入空切片
    /// （不支持 $under）。
    fn score_operator_condition(&self, code: &Value, cond: &Map<String, Value>, path: &[String]) -> f64 {
        // $exists：存在性判定
        if let Some(exists) = cond.get("$exists") {
            let want = exists.as_bool().unwrap_or(false);
            let actual = !code.is_null() && code != "";
            if want != actual {
                return -1.0;
            }
        }
        // $eq：相等（不满足直接淘汰）
        if let Some(eq) = cond.get("$eq")
            && !same_value(code, eq)
        {
            return -1.0;
        }
        // $ne：不等（相等则淘汰）
        if let Some(ne) = cond.get("$ne")
            && same_value(code, ne)
        {
            return -1.0;
        }
        // $in：在列表中
        if let Some(inv) = cond.get("$in") {
            let list = inv.as_array().cloned().unwrap_or_else(|| vec![inv.clone()]);
            if !list.iter().any(|v| same_value(code, v)) {
                return -1.0;
            }
        }
        // $nin：不在列表中
        if let Some(nin) = cond.get("$nin") {
            let list = nin.as_array().cloned().unwrap_or_else(|| vec![nin.clone()]);
            if list.iter().any(|v| same_value(code, v)) {
                return -1.0;
            }
        }
        // $under：锚点值等于目标，或其层级路径含目标（祖先泛化）
        if let Some(under) = cond.get("$under") {
            let hit = same_value(code, under)
                || path.iter().any(|p| same_value(&Value::String(p.clone()), under));
            if !hit {
                return -1.0;
            }
        }
        // 得分：$eq 最高，其次 $in/$under，其余（$ne/$nin/$exists）1 分
        if cond.contains_key("$eq") {
            3.0
        } else if cond.contains_key("$in") || cond.contains_key("$under") {
            1.5
        } else {
            1.0
        }
    }

    /// 评分标量值条件（通配/数组/操作符对象/精确匹配）。
    fn score_value_condition(&self, value: &Value, want: &Value) -> f64 {
        match want {
            // 通配 "*"：弱匹配 0.25 分
            Value::String(s) if s == "*" => 0.25,
            // 数组：命中任一得 1.5，否则淘汰
            Value::Array(arr) => {
                if arr.iter().any(|v| same_value(value, v)) {
                    1.5
                } else {
                    -1.0
                }
            }
            // 含 $ 操作符的对象：委托操作符评分（属性值条件无层级语境，$under 不生效）
            Value::Object(o) if o.keys().any(|k| k.starts_with('$')) => {
                self.score_operator_condition(value, o, &[])
            }
            // 精确匹配：得 1 分，否则淘汰
            other => {
                if same_value(value, other) {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }

    /// 合并所有命中规则为一条（字段去重、分组拼接、columnModel 合并）。
    ///
    /// 单条命中直接返回；多条命中时字段位置取首现、值取高分（同分保留先到），
    /// 分组按声明顺序拼接，columnModel 高分覆盖、同分靠前优先，锚点维度取并集。
    ///
    /// # Arguments
    ///
    /// * `anchor` - 锚点维度取值（code → 值）。
    ///
    /// # Returns
    ///
    /// 返回合并后的规则（含 id/anchor/detail/columnModel）；无命中规则返回 `None`。
    pub fn resolve_merged_rule(&self, anchor: &Map<String, Value>) -> Option<Value> {
        let matched = self.matched_rules(anchor);
        if matched.is_empty() {
            return None;
        }
        if matched.len() == 1 {
            return Some(matched[0].0.clone());
        }

        // 按定义顺序（order 升序）
        let mut by_def = matched.clone();
        by_def.sort_by_key(|a| a.2);

        // 字段集0：跨规则合并字段（位置取首现，值取高分，同分保留先到）
        let field_lists: Vec<(Vec<Value>, f64)> = by_def
            .iter()
            .filter_map(|(rule, score, _)| {
                let fields = rule
                    .get("detail")
                    .and_then(|d| d.get("fields"))
                    .and_then(|v| v.as_array())?;
                Some((fields.clone(), *score))
            })
            .collect();
        let fields = merge_field_lists(&field_lists);

        // 分组：定义顺序拼接
        let mut groups: Vec<Value> = Vec::new();
        for (rule, _, _) in &by_def {
            if let Some(gs) = rule
                .get("detail")
                .and_then(|d| d.get("groups"))
                .and_then(|v| v.as_array())
            {
                groups.extend(gs.iter().cloned());
            }
        }

        // detail.table：定义顺序首个非空（跨面板不同表时以最靠前规则为准）
        let detail_table: Option<String> = by_def
            .iter()
            .filter_map(|(rule, _, _)| {
                rule.get("detail")
                    .and_then(|d| d.get("table"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            })
            .next();

        // fieldTabs：按字段集标识（table，缺省退 name/id）跨规则聚合——同表字段合并、
        // 分组拼接、columnModel 高分覆盖（与规则级同策略）；聚合顺序取首现顺序。
        // 与 JS 侧 FlexibleCombinationEngine.resolveMergedRule 语义一致。
        type TabAgg = (
            Value,
            Vec<(Vec<Value>, f64)>,
            Vec<Value>,
            Vec<(Value, f64, usize)>,
        );
        let mut tab_order: Vec<String> = Vec::new();
        let mut tab_agg: std::collections::HashMap<String, TabAgg> =
            std::collections::HashMap::new();
        for (rule, score, order) in &by_def {
            let Some(tabs) = rule
                .get("detail")
                .and_then(|d| d.get("fieldTabs"))
                .and_then(|v| v.as_array())
            else {
                continue;
            };
            for t in tabs {
                let key = t
                    .get("table")
                    .and_then(|v| v.as_str())
                    .or_else(|| t.get("name").and_then(|v| v.as_str()))
                    .or_else(|| t.get("id").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let entry = tab_agg.entry(key.clone()).or_insert_with(|| {
                    tab_order.push(key.clone());
                    (t.clone(), Vec::new(), Vec::new(), Vec::new())
                });
                if let Some(fs) = t.get("fields").and_then(|v| v.as_array()) {
                    entry.1.push((fs.clone(), *score));
                }
                if let Some(gs) = t.get("groups").and_then(|v| v.as_array()) {
                    entry.2.extend(gs.iter().cloned());
                }
                if let Some(cm) = t.get("columnModel").filter(|v| v.is_object()) {
                    entry.3.push((cm.clone(), *score, *order));
                }
            }
        }
        let mut field_tabs: Vec<Value> = Vec::new();
        for key in &tab_order {
            let Some((head, tab_lists, tab_groups, tab_cms)) = tab_agg.get(key) else {
                continue;
            };
            let mut next = head.as_object().cloned().unwrap_or_default();
            next.remove("use");
            next.remove("pick");
            next.remove("over");
            next.insert(
                "fields".to_string(),
                Value::Array(merge_field_lists(tab_lists)),
            );
            if tab_groups.is_empty() {
                next.remove("groups");
            } else {
                next.insert("groups".to_string(), Value::Array(tab_groups.clone()));
            }
            // 字段集级 columnModel：高分覆盖、同分靠前优先 → score asc / 同分 order desc 后 assign
            let mut cm_order = tab_cms.clone();
            cm_order.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.2.cmp(&a.2))
            });
            let mut cm_merged = Map::new();
            for (cm, _, _) in &cm_order {
                if let Value::Object(m) = cm {
                    for (k, v) in m {
                        cm_merged.insert(k.clone(), v.clone());
                    }
                }
            }
            if cm_merged.is_empty() {
                next.remove("columnModel");
            } else {
                next.insert("columnModel".to_string(), Value::Object(cm_merged));
            }
            field_tabs.push(Value::Object(next));
        }

        // columnModel：高分覆盖、同分靠前优先 → score asc / 同分 order desc 后 assign
        let mut cm_order = matched.clone();
        cm_order.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.2.cmp(&a.2))
        });
        let mut column_model = Map::new();
        for (rule, _, _) in &cm_order {
            if let Some(Value::Object(cm)) = rule.get("columnModel") {
                for (k, v) in cm {
                    column_model.insert(k.clone(), v.clone());
                }
            }
        }

        // 锚点维度并集
        let mut dims_union: Vec<String> = Vec::new();
        for (rule, _, _) in &by_def {
            if let Some(ds) = rule
                .get("anchor")
                .and_then(|a| a.get("dimensions"))
                .and_then(|v| v.as_array())
            {
                for d in ds {
                    if let Some(s) = d.as_str()
                        && !dims_union.iter().any(|x| x == s)
                    {
                        dims_union.push(s.to_string());
                    }
                }
            }
        }

        let id = by_def
            .iter()
            .filter_map(|m| m.0.get("id").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("+");
        let id = if id.is_empty() {
            "__merged__".to_string()
        } else {
            id
        };
        let mut detail = Map::new();
        detail.insert("fields".to_string(), Value::Array(fields));
        if let Some(t) = detail_table {
            detail.insert("table".to_string(), json!(t));
        }
        if !groups.is_empty() {
            detail.insert("groups".to_string(), Value::Array(groups));
        }
        if !field_tabs.is_empty() {
            detail.insert("fieldTabs".to_string(), Value::Array(field_tabs));
        }
        let mut merged = json!({
            "id": id,
            "anchor": { "dimensions": dims_union, "match": {} },
            "detail": Value::Object(detail),
        });
        if !column_model.is_empty() {
            merged
                .as_object_mut()
                .expect("invariant: merged 由 json!({{...}}) 构造,必为对象")
                .insert("columnModel".to_string(), Value::Object(column_model));
        }
        Some(merged)
    }

    /// buildMembers：分组（CmxColumnGroup）+ 未分组列。
    ///
    /// 无 groups 时直接返回扁平列；有 groups 时按分组结构嵌套，未被任何分组引用的列追加到末尾。
    ///
    /// # Arguments
    ///
    /// * `rule` - 命中规则（取 detail.fields + detail.groups 构建分组结构）。
    ///
    /// # Returns
    /// 返回成员数组（分组节点与未分组列混合）；无 groups 时返回扁平列。
    pub fn build_members(&self, rule: &Value) -> Vec<Value> {
        let flat = self.build_columns(rule);
        let groups = rule
            .get("detail")
            .and_then(|d| d.get("groups"))
            .and_then(|v| v.as_array());
        let Some(groups) = groups.filter(|g| !g.is_empty()) else {
            return flat;
        };
        let by_id: std::collections::HashMap<String, Value> = flat
            .iter()
            .map(|c| {
                (
                    c.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    c.clone(),
                )
            })
            .collect();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut counter = 0usize;
        let mut out: Vec<Value> = Vec::new();
        for g in groups {
            if let Some(grp) = group::build_group_node(g, &by_id, &mut used, &mut counter) {
                out.push(grp);
            }
        }
        for c in &flat {
            let cid = c
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !used.contains(&cid) {
                out.push(c.clone());
            }
        }
        out
    }

    /// buildColumnModelProps：combination.columnModel + rule.columnModel（规则覆盖）。
    ///
    /// # Arguments
    ///
    /// * `rule` - 命中规则（其 columnModel 覆盖 combination 的同名字段）。
    /// * `combination` - 弹性组合定义（提供基础 columnModel）。
    ///
    /// # Returns
    ///
    /// 返回合并后的列模型属性对象。
    pub fn build_column_model_props(&self, rule: &Value, combination: &Value) -> Value {
        let mut out = Map::new();
        if let Some(o) = combination.get("columnModel").and_then(|v| v.as_object()) {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        if let Some(o) = rule.get("columnModel").and_then(|v| v.as_object()) {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 双规则（精确 + 兜底），各带双字段集（t_head + t_line），同名字段以 v 区分来源。
    fn rules() -> Value {
        json!([
            {
                "id": "exact",
                "anchor": { "dimensions": ["account"], "match": { "account": "1122" } },
                "detail": {
                    "table": "t_head",
                    "fields": [ { "id": "a" }, { "id": "b", "v": 1 } ],
                    "fieldTabs": [
                        { "id": "ft1", "table": "t_line", "fields": [ { "id": "x" }, { "id": "y", "v": 1 } ] }
                    ]
                }
            },
            {
                "id": "fallback",
                "anchor": { "dimensions": ["account"] },
                "detail": {
                    "table": "t_head",
                    "fields": [ { "id": "b", "v": 2 }, { "id": "c" } ],
                    "fieldTabs": [
                        { "id": "ft2", "table": "t_line", "fields": [ { "id": "y", "v": 2 }, { "id": "z" } ] }
                    ]
                }
            }
        ])
    }

    #[test]
    fn resolve_merged_rule_keeps_table_and_merges_field_tabs() {
        let dims = json!({});
        let rs = rules();
        let engine = Engine::new(&dims, &rs);
        let mut anchor = Map::new();
        anchor.insert("account".to_string(), json!("1122"));
        let merged = engine.resolve_merged_rule(&anchor).expect("应合并出规则");

        assert_eq!(merged["id"], json!("exact+fallback"));
        // detail.table 保留（定义顺序首个非空）
        assert_eq!(merged["detail"]["table"], json!("t_head"));
        // 字段集0：位置首现 a,b,c；同名字段 b 取高分（exact, v=1）
        let fields = merged["detail"]["fields"].as_array().unwrap();
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
        let b = fields.iter().find(|f| f["id"] == json!("b")).unwrap();
        assert_eq!(b["v"], json!(1));
        // fieldTabs：同表 t_line 聚合为 1 项；x,y,z 顺序；同名字段 y 取高分
        let tabs = merged["detail"]["fieldTabs"].as_array().unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0]["table"], json!("t_line"));
        let tab_ids: Vec<_> = tabs[0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["id"].as_str())
            .collect();
        assert_eq!(tab_ids, vec!["x", "y", "z"]);
        let y = tabs[0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == json!("y"))
            .unwrap();
        assert_eq!(y["v"], json!(1));
    }

    #[test]
    fn resolve_merged_rule_omits_field_tabs_when_absent() {
        let dims = json!({});
        let rs = json!([
            { "id": "r1", "anchor": { "dimensions": ["account"], "match": { "account": "1122" } },
              "detail": { "table": "t_head", "fields": [ { "id": "a" } ] } },
            { "id": "r2", "anchor": { "dimensions": ["account"] },
              "detail": { "table": "t_head", "fields": [ { "id": "b" } ] } }
        ]);
        let engine = Engine::new(&dims, &rs);
        let mut anchor = Map::new();
        anchor.insert("account".to_string(), json!("1122"));
        let merged = engine.resolve_merged_rule(&anchor).unwrap();
        assert!(merged["detail"].get("fieldTabs").is_none());
        assert_eq!(merged["detail"]["table"], json!("t_head"));
    }

    /// L1/L2/L3 层级泛化规则集：L1/L2 用 $under 配在祖先上，L3 精确，另有兜底。
    /// 案例树：2(负债 L1) → 2221(应交税费 L2) → 222101(进项税 L3)。
    fn hierarchical_rules() -> Value {
        json!([
            {
                "id": "l1-liability",
                "anchor": { "dimensions": ["gl_account"], "match": { "gl_account": { "$under": "2" } } },
                "detail": { "table": "t_aux", "fields": [ { "id": "item_text", "v": 1 }, { "id": "cost_center_id" } ] }
            },
            {
                "id": "l2-payable",
                "anchor": { "dimensions": ["gl_account"], "match": { "gl_account": { "$under": "2221" } } },
                "detail": { "table": "t_aux", "fields": [ { "id": "supplier_id" }, { "id": "item_text", "v": 2 } ] }
            },
            {
                "id": "l3-vat-in",
                "anchor": { "dimensions": ["gl_account"], "match": { "gl_account": "222101" } },
                "detail": { "table": "t_aux", "fields": [ { "id": "tax_rate", "v": 3 } ] }
            },
            {
                "id": "fallback",
                "anchor": { "dimensions": ["gl_account"], "match": {} },
                "detail": { "table": "t_aux", "fields": [ { "id": "amount" } ] }
            }
        ])
    }

    /// $under 层级联动：选中 L3（带祖先链 __path）→ L1+L2+L3+兜底 四条命中合并；
    /// 字段位置取首现（L1 基础在前），同名字段值取高分（L3 精确 3 分 > L2/L1 泛化 1.5 分）。
    #[test]
    fn under_path_merges_l1_l2_l3_chain() {
        let dims = json!({});
        let rs = hierarchical_rules();
        let engine = Engine::new(&dims, &rs);
        let mut anchor = Map::new();
        anchor.insert("gl_account".to_string(), json!("222101"));
        anchor.insert("gl_account.__path".to_string(), json!("2,2221,222101"));
        let merged = engine.resolve_merged_rule(&anchor).expect("四条规则应全命中");

        assert_eq!(
            merged["id"],
            json!("l1-liability+l2-payable+l3-vat-in+fallback")
        );
        let fields = merged["detail"]["fields"].as_array().unwrap();
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        // 位置：L1 基础（item_text, cost_center_id）在前，L2/L3 增量随后，兜底殿后
        assert_eq!(
            ids,
            vec!["item_text", "cost_center_id", "supplier_id", "tax_rate", "amount"]
        );
        // 同名字段 item_text：L1(1.5) vs L2(1.5) 同分取先到（v=1）
        let it = fields.iter().find(|f| f["id"] == json!("item_text")).unwrap();
        assert_eq!(it["v"], json!(1));
    }

    /// $under 未命中：值不等且 __path 不含祖先 → 该规则淘汰（仅兜底命中）。
    #[test]
    fn under_path_miss_falls_back() {
        let dims = json!({});
        let rs = hierarchical_rules();
        let engine = Engine::new(&dims, &rs);
        // 资产类 L2：1001，path 不含 2/2221 → 仅兜底
        let mut anchor = Map::new();
        anchor.insert("gl_account".to_string(), json!("1001"));
        anchor.insert("gl_account.__path".to_string(), json!("1,1001"));
        let merged = engine.resolve_merged_rule(&anchor).expect("兜底应命中");
        assert_eq!(merged["id"], json!("fallback"));
    }

    /// $under 无 __path 时退化为值相等匹配（不炸、不误命中兄弟分支）。
    #[test]
    fn under_without_path_degrades_to_exact() {
        let dims = json!({});
        let rs = hierarchical_rules();
        let engine = Engine::new(&dims, &rs);
        let mut anchor = Map::new();
        anchor.insert("gl_account".to_string(), json!("2221"));
        // 无 __path：l2-payable 的 $under "2221" 仅靠值相等命中；l1-liability 的 $under "2" 不命中
        let merged = engine.resolve_merged_rule(&anchor).expect("L2+兜底应命中");
        assert_eq!(merged["id"], json!("l2-payable+fallback"));
    }

    /// __path 数组形态（服务端 anchor_map 拆分后的形态）同样生效。
    #[test]
    fn under_path_accepts_array_form() {
        let dims = json!({});
        let rs = hierarchical_rules();
        let engine = Engine::new(&dims, &rs);
        let mut anchor = Map::new();
        anchor.insert("gl_account".to_string(), json!("222102"));
        anchor.insert(
            "gl_account.__path".to_string(),
            json!(["2", "2221", "222102"]),
        );
        let merged = engine.resolve_merged_rule(&anchor).unwrap();
        assert_eq!(merged["id"], json!("l1-liability+l2-payable+fallback"));
    }
}
