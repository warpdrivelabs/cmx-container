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
    pub fn with_ref_context(mut self, from: crate::flexible_combination::drn::FromDam, imports: Option<Value>) -> Self {
        self.ref_from = from;
        self.ref_imports = imports;
        self
    }

    /// 归一字段 refDict → 有效 dict/维度 code（裸 code 原样，DRN/别名展开取 name）。
    fn effective_ref(&self, raw: &str) -> String {
        crate::flexible_combination::drn::effective_dict_id(raw, &self.ref_from, self.ref_imports.as_ref())
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
            let item = self.score_condition(dim, base_dim, code, cond);
            if item < 0.0 {
                return -1.0;
            }
            score += item;
        }
        score
    }

    /// 评分单条件。dim 可能带属性路径（"gl_account.account_type"）。
    fn score_condition(&self, dim_full: &str, base_dim: &str, code: &Value, cond: &Value) -> f64 {
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
                // 含 $ 操作符 → 走操作符评分
                if o.keys().any(|k| k.starts_with('$')) {
                    self.score_operator_condition(code, o)
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

    /// 评分 $ 操作符条件（$exists/$eq/$ne/$in/$nin）。
    ///
    /// 任一操作符不满足返回 -1；最终得分按 $eq(3) > $in(1.5) > 其余(1)。
    fn score_operator_condition(&self, code: &Value, cond: &Map<String, Value>) -> f64 {
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
        // 得分：$eq 最高，其次 $in，其余（$ne/$nin/$exists）1 分
        if cond.contains_key("$eq") {
            3.0
        } else if cond.contains_key("$in") {
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
            // 含 $ 操作符的对象：委托操作符评分
            Value::Object(o) if o.keys().any(|k| k.starts_with('$')) => {
                self.score_operator_condition(value, o)
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

        // 字段：位置取首现，值取高分（同分保留先到）
        let mut by_code: std::collections::HashMap<String, (Value, f64, usize)> =
            std::collections::HashMap::new();
        let mut pos = 0usize;
        for (rule, score, _) in &by_def {
            if let Some(fields) = rule
                .get("detail")
                .and_then(|d| d.get("fields"))
                .and_then(|v| v.as_array())
            {
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
        }
        let mut field_entries: Vec<(Value, f64, usize)> = by_code.into_values().collect();
        field_entries.sort_by_key(|a| a.2);
        let fields: Vec<Value> = field_entries.into_iter().map(|x| x.0).collect();

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
        let detail = if groups.is_empty() {
            json!({ "fields": fields })
        } else {
            json!({ "fields": fields, "groups": groups })
        };
        let mut merged = json!({
            "id": id,
            "anchor": { "dimensions": dims_union, "match": {} },
            "detail": detail,
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
