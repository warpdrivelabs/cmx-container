//! FlexibleCombination 运行时引擎（复刻 `flexible-combination-engine.js`）。
//!
//! 核心：`resolve_merged_rule`（锚点评分 + 多规则字段合并）、`build_columns`（_fieldToColumn 全派生 →
//! CmxColumn.toJSON 形状）、`build_members`（分组 CmxColumnGroup）、`build_column_model_props`。
//!
//! 运行时闭包（calcFormula/onPickDimension/recompute）在 JSON 序列化时为 null，故此处不实现，
//! 仅产出与 Node `CmxColumn.toJSON()` 等价的可序列化结果。

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
fn same_value(a: &Value, b: &Value) -> bool {
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
fn field_id(field: &Value) -> String {
    field.get("id").map(value_to_string).unwrap_or_default()
}

/// 将 JSON 标量值归一为字符串（字符串/数字/布尔原样转，null 与其余返回空串）。
fn value_to_string(v: &Value) -> String {
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
fn field_caption(field: &Value) -> String {
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
fn field_display_name(field: &Value) -> String {
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
fn is_numeric_type(dt: &str) -> bool {
    matches!(
        dt.to_uppercase().as_str(),
        "INT" | "BIGINT" | "TINYINT" | "DECIMAL" | "NUMBER"
    )
}

/// 推断字段数据类型（与 Node `_fieldDataType` 一致）。
///
/// field.dataType 为非空真值才用；否则 measure 维度默认 DECIMAL，其余默认 VARCHAR。
fn field_data_type(field: &Value) -> String {
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
                .unwrap()
                .insert("columnModel".to_string(), Value::Object(column_model));
        }
        Some(merged)
    }

    // ── 字段 → CmxColumn.toJSON ────────────────────────────────────

    /// buildColumns：每个字段 → 列 JSON。
    ///
    /// # Arguments
    ///
    /// * `rule` - 命中规则（取 detail.fields 逐字段转列）。
    ///
    /// # Returns
    ///
    /// 返回列 JSON 数组；rule 无 detail.fields 时返回空。
    pub fn build_columns(&self, rule: &Value) -> Vec<Value> {
        rule.get("detail")
            .and_then(|d| d.get("fields"))
            .and_then(|v| v.as_array())
            .map(|fields| {
                fields
                    .iter()
                    .map(|f| self.field_to_column(f, rule))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// _fieldToColumn + CmxColumn.toJSON 等价输出。
    fn field_to_column(&self, field: &Value, rule: &Value) -> Value {
        let req = field
            .get("edit")
            .and_then(|e| e.get("required"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let id = field_id(field);
        let caption = field_caption(field);
        let dict_settings = self.dict_settings_for_field(field, rule);
        let base = field
            .get("column")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        // col 起点：field 全透传 + base 覆盖（去掉 type/editMode/column）
        let mut col = field.as_object().cloned().unwrap_or_default();
        for (k, v) in &base {
            col.insert(k.clone(), v.clone());
        }
        col.insert("id".to_string(), json!(id));
        col.insert(
            "caption".to_string(),
            json!(format!("{}{}", if req { "* " } else { "" }, caption)),
        );
        let data_type = base
            .get("dataType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| field_data_type(field));
        col.insert("dataType".to_string(), json!(data_type));
        col.insert("required".to_string(), json!(req));
        col.remove("type");
        col.remove("editMode");
        col.remove("column");
        if let Some(w) = field.get("width") {
            col.insert("width".to_string(), w.clone());
        }
        // 物理属性回填
        if (!col.contains_key("agg") || col["agg"].is_null())
            && let Some(a) = field.get("agg")
        {
            col.insert("agg".to_string(), a.clone());
        }
        if col.get("length").map(|v| v.is_null()).unwrap_or(true)
            && let Some(l) = field.get("fieldLength")
        {
            col.insert("length".to_string(), l.clone());
        }
        if col
            .get("integerDigits")
            .map(|v| v.is_null())
            .unwrap_or(true)
            && let Some(l) = field.get("intDigits")
        {
            col.insert("integerDigits".to_string(), l.clone());
        }
        if col
            .get("decimalDigits")
            .map(|v| v.is_null())
            .unwrap_or(true)
            && let Some(l) = field.get("decimalDigits")
        {
            col.insert("decimalDigits".to_string(), l.clone());
        }

        // display 归一
        let display = self.normalize_field_display(base.get("display"), field.get("display"));
        let mut display = display;
        if !display.contains_key("align")
            && field.get("dimType").and_then(|v| v.as_str()) == Some("measure")
        {
            display.insert("align".to_string(), json!("right"));
        }
        col.insert("display".to_string(), Value::Object(display));

        // edit = base.edit + field.edit
        let mut edit = base
            .get("edit")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(fe) = field.get("edit").and_then(|v| v.as_object()) {
            for (k, v) in fe {
                edit.insert(k.clone(), v.clone());
            }
        }
        if !edit.contains_key("mode") {
            let mode = if dict_settings.is_some() {
                "cmx-dict-selct".to_string()
            } else {
                self.col_edit_mode(field)
            };
            edit.insert("mode".to_string(), json!(mode));
        }
        if field
            .get("edit")
            .and_then(|e| e.get("mode"))
            .and_then(|v| v.as_str())
            == Some("computed")
            && base.get("edit").and_then(|e| e.get("mode")).is_none()
        {
            edit.insert("mode".to_string(), json!("readonly"));
        }
        for (fk, ck) in [
            ("requiredWhen", "requiredWhen"),
            ("readonlyWhen", "readonlyWhen"),
            ("placeholder", "placeholder"),
        ] {
            if let Some(v) = field.get("edit").and_then(|e| e.get(fk))
                && !edit.contains_key(ck)
            {
                edit.insert(ck.to_string(), v.clone());
            }
        }
        // validations + pattern → edit.validate（pattern 编译为函数 → JSON null，故仅 expr 规则保留）
        let mut rules_v: Vec<Value> = field
            .get("validations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|v| v.get("expr").is_some())
                    .map(|v| json!({ "expr": v.get("expr").cloned().unwrap_or(Value::Null), "message": v.get("message").and_then(|m| m.as_str()).unwrap_or("校验未通过") }))
                    .collect()
            })
            .unwrap_or_default();
        // pattern 规则在 Node 是闭包函数 → _stripFns 丢弃，故不加入序列化结果
        let _ = field.get("pattern");
        if !rules_v.is_empty() && !edit.contains_key("validate") {
            edit.insert(
                "validate".to_string(),
                Value::Array(std::mem::take(&mut rules_v)),
            );
        }
        // base.editSettings / dictSettings 合并
        if base.contains_key("editSettings") || dict_settings.is_some() {
            if let Some(es) = base.get("editSettings").and_then(|v| v.as_object()) {
                for (k, v) in es {
                    edit.insert(k.clone(), v.clone());
                }
            }
            if let Some(ds) = &dict_settings
                && let Some(o) = ds.as_object()
            {
                for (k, v) in o {
                    edit.insert(k.clone(), v.clone());
                }
            }
        }
        // enumValues → select options（无字典 + 无 base.edit.mode）
        let has_base_mode = base.get("edit").and_then(|e| e.get("mode")).is_some();
        if dict_settings.is_none()
            && !has_base_mode
            && let Some(enums) = field
                .get("enumValues")
                .and_then(|v| v.as_array())
                .filter(|a| !a.is_empty())
        {
            let options: Vec<Value> = enums
                    .iter()
                    .map(|v| {
                        if v.is_object() {
                            json!({ "value": v.get("value").cloned().unwrap_or(Value::Null), "label": v.get("label").cloned().unwrap_or_else(|| v.get("value").cloned().unwrap_or(Value::Null)) })
                        } else {
                            json!({ "value": v, "label": value_to_string(v) })
                        }
                    })
                    .collect();
            edit.insert("options".to_string(), Value::Array(options));
            edit.insert("mode".to_string(), json!("select"));
        }
        if let Some(ds) = &dict_settings
            && let Some(o) = ds.as_object()
        {
            for (k, v) in o {
                edit.insert(k.clone(), v.clone());
            }
        }
        // dimension select options（来自 dim.values）
        if field.get("dimType").and_then(|v| v.as_str()) == Some("dimension") {
            let dim_code = field
                .get("refDict")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            if dict_settings.is_none()
                && edit.get("mode").and_then(|v| v.as_str()) == Some("select")
                && let Some(dim) = self.get_dimension(&dim_code)
                && let Some(values) = dim.get("values").and_then(|v| v.as_array())
            {
                let options: Vec<Value> = values
                            .iter()
                            .map(|v| json!({ "value": v.get("code").cloned().unwrap_or(Value::Null), "label": v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()).unwrap_or_else(|| value_to_string(v.get("code").unwrap_or(&Value::Null))) }))
                            .collect();
                edit.insert("options".to_string(), Value::Array(options));
            }
        }
        // measure / numeric display 精度
        if field.get("dimType").and_then(|v| v.as_str()) == Some("measure")
            || is_numeric_type(&data_type)
        {
            let mut disp =
                json!({ "decimals": 2, "thousand": ",", "zeroBlank": true, "negativeColor": true });
            if let Some(fd) = field.get("display").and_then(|v| v.as_object()) {
                for (k, v) in fd {
                    disp.as_object_mut().unwrap().insert(k.clone(), v.clone());
                }
            }
            edit.insert("display".to_string(), disp);
        }

        // 现在用 col + display + edit 走 CmxColumn 构造 + toJSON
        col.insert("edit".to_string(), Value::Object(edit));
        self.cmx_column_to_json(col)
    }

    /// 等价 CmxColumn 构造 + toJSON：补默认、归一 display/edit、按 KNOWN + 透传键输出。
    fn cmx_column_to_json(&self, props: Map<String, Value>) -> Value {
        let get = |k: &str| props.get(k).cloned();
        let display = normalize_display(&props);
        let edit = normalize_edit(&props);
        let required = edit
            .get("required")
            .cloned()
            .filter(|v| !v.is_null())
            .or_else(|| get("required"))
            .unwrap_or(json!(false));
        let display_mode = display
            .get("mode")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| get("displayMode").and_then(|v| v.as_str().map(|s| s.to_string())))
            .unwrap_or_else(|| "text".to_string());

        let mut json = Map::new();
        json.insert("id".to_string(), get("id").unwrap_or(json!("")));
        json.insert("caption".to_string(), get("caption").unwrap_or(json!("")));
        json.insert(
            "dataType".to_string(),
            get("dataType").unwrap_or(json!("VARCHAR")),
        );
        json.insert("length".to_string(), get("length").unwrap_or(Value::Null));
        json.insert(
            "integerDigits".to_string(),
            get("integerDigits").unwrap_or(Value::Null),
        );
        json.insert(
            "decimalDigits".to_string(),
            get("decimalDigits").unwrap_or(Value::Null),
        );
        json.insert("calcFormula".to_string(), strip_fn(get("calcFormula")));
        json.insert(
            "validateFormula".to_string(),
            strip_fn(get("validateFormula")),
        );
        json.insert("displayMode".to_string(), json!(display_mode));
        json.insert("displayMask".to_string(), strip_fn(get("displayMask")));
        json.insert(
            "actionRef".to_string(),
            get("actionRef").unwrap_or(Value::Null),
        );
        json.insert("width".to_string(), get("width").unwrap_or(Value::Null));
        json.insert("required".to_string(), required);
        json.insert("visible".to_string(), get("visible").unwrap_or(json!(true)));
        json.insert("frozen".to_string(), get("frozen").unwrap_or(json!(false)));
        json.insert("agg".to_string(), get("agg").unwrap_or(Value::Null));
        json.insert("display".to_string(), Value::Object(strip_fns(&display)));
        json.insert("edit".to_string(), Value::Object(strip_fns(&edit)));

        // 透传额外键（非 KNOWN、非函数）
        const KNOWN: &[&str] = &[
            "id",
            "caption",
            "dataType",
            "length",
            "integerDigits",
            "decimalDigits",
            "calcFormula",
            "validateFormula",
            "displayMode",
            "displayMask",
            "label",
            "align",
            "actionRef",
            "width",
            "required",
            "visible",
            "frozen",
            "agg",
            "display",
            "edit",
            // 构造期消费但不输出的中间键
            "editSettings",
            "column",
            "type",
            "editMode",
        ];
        for (k, v) in &props {
            if KNOWN.contains(&k.as_str()) {
                continue;
            }
            json.insert(k.clone(), v.clone());
        }
        Value::Object(json)
    }

    fn col_edit_mode(&self, field: &Value) -> String {
        let m = field
            .get("edit")
            .and_then(|e| e.get("mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("cmx-text-input");
        match m {
            "computed" => "readonly".to_string(),
            "tree-ref" => "ref".to_string(),
            other => other.to_string(),
        }
    }

    fn normalize_field_display(
        &self,
        base_display: Option<&Value>,
        field_display: Option<&Value>,
    ) -> Map<String, Value> {
        let mut src = Map::new();
        if let Some(o) = base_display.and_then(|v| v.as_object()) {
            for (k, v) in o {
                src.insert(k.clone(), v.clone());
            }
        }
        if let Some(o) = field_display.and_then(|v| v.as_object()) {
            for (k, v) in o {
                src.insert(k.clone(), v.clone());
            }
        }
        let mut out = Map::new();
        for (k, v) in &src {
            if v.is_null() || v == "" {
                continue;
            }
            let key = match k.as_str() {
                "decimals" => "decimalDigits",
                "zeroBlank" => "zeroAsBlank",
                "thousand" => "thousandSeparator",
                other => other,
            };
            out.insert(key.to_string(), v.clone());
        }
        out
    }

    /// _dictSettingsForField：dimension 字段 → 字典选择设置（含 columns/writeBack 默认）。
    fn dict_settings_for_field(&self, field: &Value, rule: &Value) -> Option<Value> {
        if field.get("dimType").and_then(|v| v.as_str()) != Some("dimension") {
            return None;
        }
        let id = field_id(field);
        let dim_code = field
            .get("refDict")
            .and_then(|v| v.as_str())
            .map(|s| self.effective_ref(s))
            .unwrap_or_else(|| id.clone());
        let dim = self.get_dimension(&dim_code).cloned().unwrap_or(json!({}));
        // dim.dict：对象或字符串
        let dim_dict = match dim.get("dict") {
            Some(Value::Object(o)) => Value::Object(o.clone()),
            Some(Value::String(s)) => json!({ "dictId": s }),
            _ => json!({}),
        };
        let dd = |k: &str| dim_dict.get(k).cloned();
        let dict_code = field
            .get("refDict")
            .and_then(|v| v.as_str())
            .map(|s| self.effective_ref(s))
            .or_else(|| dd("dictId").and_then(|v| v.as_str().map(|s| s.to_string())))
            .or_else(|| dd("dictCode").and_then(|v| v.as_str().map(|s| s.to_string())))
            .or_else(|| dd("code").and_then(|v| v.as_str().map(|s| s.to_string())));
        let dict_code = dict_code?;
        let id_col = dd("idCol")
            .or_else(|| dd("idField"))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "id".to_string());
        let code_col = field
            .get("refField")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| dd("codeCol").and_then(|v| v.as_str().map(|s| s.to_string())))
            .or_else(|| dd("codeField").and_then(|v| v.as_str().map(|s| s.to_string())))
            .or_else(|| dd("valueField").and_then(|v| v.as_str().map(|s| s.to_string())))
            .unwrap_or_else(|| id_col.clone());
        let label_col = field
            .get("displayField")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| dd("labelCol").and_then(|v| v.as_str().map(|s| s.to_string())))
            .or_else(|| dd("labelField").and_then(|v| v.as_str().map(|s| s.to_string())))
            .unwrap_or_else(|| "name".to_string());
        let value_field = field
            .get("refField")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| dd("valueField").and_then(|v| v.as_str().map(|s| s.to_string())))
            .unwrap_or_else(|| code_col.clone());
        let write_back = dd("writeBack").filter(|v| v.is_object()).unwrap_or_else(|| {
            json!({ id.clone(): value_field, format!("{id}Id"): id_col, format!("{id}Name"): label_col })
        });
        let dim_name = dim.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let cap = field_caption(field);
        let dict_title = dd("dictTitle")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                format!(
                    "选择{}",
                    if !cap.is_empty() {
                        &cap
                    } else if !dim_name.is_empty() {
                        dim_name
                    } else {
                        &dict_code
                    }
                )
            });
        Some(json!({
            "dictCode": dict_code,
            "idCol": id_col,
            "codeCol": code_col,
            "labelCol": label_col,
            "parentCol": dd("parentCol").or_else(|| dd("parentField")).and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "parent_id".to_string()),
            "hierarchical": dd("hierarchical").and_then(|v| v.as_bool()).unwrap_or(false),
            "helpLayout": dd("helpLayout").and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "grid".to_string()),
            "valueField": value_field,
            "displayMode": dd("displayMode").and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "code-label".to_string()),
            "dictTitle": dict_title,
            "columns": dd("columns").filter(|v| v.is_array()),
            "filters": dd("filters").or_else(|| dd("dictFilters")),
            "pageSize": dd("pageSize").and_then(|v| v.as_i64()).unwrap_or(50),
            "writeBack": write_back,
            "dependents": self.fields_depending_on_dimension(rule, &dim_code),
        }))
    }

    /// _fieldsDependingOnDimension：attribute(source.dimension) + measure(defaultFrom.dimension)
    /// 直接依赖，加上由这些字段进一步驱动的 computed measure（formula+dependsOn）。
    fn fields_depending_on_dimension(&self, rule: &Value, dim_code: &str) -> Vec<String> {
        let fields = rule
            .get("detail")
            .and_then(|d| d.get("fields"))
            .and_then(|v| v.as_array());
        let Some(fields) = fields else { return vec![] };
        let mut direct: Vec<String> = Vec::new();
        for f in fields {
            let dt = f.get("dimType").and_then(|v| v.as_str());
            let is_attr_src = dt == Some("attribute")
                && f.get("source")
                    .and_then(|s| s.get("dimension"))
                    .and_then(|v| v.as_str())
                    == Some(dim_code);
            let is_measure_df = dt == Some("measure")
                && f.get("defaultFrom")
                    .and_then(|s| s.get("dimension"))
                    .and_then(|v| v.as_str())
                    == Some(dim_code);
            if is_attr_src || is_measure_df {
                let id = field_id(f);
                if !id.is_empty() {
                    direct.push(id);
                }
            }
        }
        let mut downstream: Vec<String> = Vec::new();
        for f in fields {
            let has_formula = f.get("formula").map(|v| !v.is_null()).unwrap_or(false);
            if has_formula
                && let Some(deps) = f.get("dependsOn").and_then(|v| v.as_array())
                && deps.iter().any(|d| {
                    d.as_str()
                        .map(|s| direct.iter().any(|x| x == s))
                        .unwrap_or(false)
                })
            {
                let id = field_id(f);
                if !id.is_empty() {
                    downstream.push(id);
                }
            }
        }
        // 去重保序（direct 在前）
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for id in direct.into_iter().chain(downstream) {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
        out
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
            if let Some(grp) = build_group_node(g, &by_id, &mut used, &mut counter) {
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

/// CmxColumnGroup 节点构建（递归）。groups members 可为字符串(列id)或嵌套组对象。
///
/// 成员为字符串时按列 id 从 `by_id` 取列并标记 used；为对象时递归构建子分组。
/// 空分组（无有效成员）返回 `None`。
fn build_group_node(
    node: &Value,
    by_id: &std::collections::HashMap<String, Value>,
    used: &mut std::collections::HashSet<String>,
    counter: &mut usize,
) -> Option<Value> {
    let mut props = node.as_object().cloned().unwrap_or_default();
    props.remove("members");
    *counter += 1;
    let gid = props
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            props
                .get("caption")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| format!("__cp_g_{counter}"));
    let caption = props
        .get("caption")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut members: Vec<Value> = Vec::new();
    if let Some(ms) = node.get("members").and_then(|v| v.as_array()) {
        for m in ms {
            match m {
                Value::String(s) => {
                    if used.contains(s) {
                        continue;
                    }
                    if let Some(col) = by_id.get(s) {
                        members.push(col.clone());
                        used.insert(s.clone());
                    }
                }
                Value::Object(_) => {
                    if let Some(child) = build_group_node(m, by_id, used, counter) {
                        members.push(child);
                    }
                }
                _ => {}
            }
        }
    }
    if members.is_empty() {
        return None;
    }
    // CmxColumnGroup.toJSON 形状：__type + id/caption + aggregate(默认全 false) + aggregatePosition + members + 透传键。
    let aggregate = {
        let mut agg = Map::new();
        for k in ["sum", "avg", "max", "min", "count"] {
            agg.insert(k.to_string(), json!(false));
        }
        if let Some(o) = node.get("aggregate").and_then(|v| v.as_object()) {
            for (k, v) in o {
                agg.insert(k.clone(), v.clone());
            }
        }
        Value::Object(agg)
    };
    let aggregate_position = node
        .get("aggregatePosition")
        .cloned()
        .unwrap_or_else(|| json!("after"));

    let mut out = Map::new();
    out.insert("__type".to_string(), json!("CmxColumnGroup"));
    out.insert("id".to_string(), json!(gid));
    out.insert("caption".to_string(), json!(caption));
    out.insert("aggregate".to_string(), aggregate);
    out.insert("aggregatePosition".to_string(), aggregate_position);
    out.insert("members".to_string(), Value::Array(members));
    // 透传额外键（非 KNOWN）：props 已去掉 members，含作者写的其它分组属性
    const GROUP_KNOWN: &[&str] = &["id", "caption", "aggregate", "aggregatePosition", "members"];
    for (k, v) in &props {
        if GROUP_KNOWN.contains(&k.as_str()) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Some(Value::Object(out))
}

/// CmxColumn._normalizeDisplay 等价。
fn normalize_display(props: &Map<String, Value>) -> Map<String, Value> {
    let mut d = props
        .get("display")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if !d.contains_key("decimalDigits")
        && let Some(v) = props.get("decimalDigits").filter(|v| !v.is_null())
    {
        d.insert("decimalDigits".to_string(), v.clone());
    }
    if !d.contains_key("format")
        && let Some(v) = props.get("displayMask").filter(|v| !v.is_null())
    {
        d.insert("format".to_string(), v.clone());
    }
    if !d.contains_key("mode")
        && let Some(dm) = props
            .get("displayMode")
            .and_then(|v| v.as_str())
            .filter(|s| *s != "text")
    {
        d.insert("mode".to_string(), json!(dm));
    }
    if !d.contains_key("mode") {
        d.insert("mode".to_string(), json!("text"));
    }
    if let Some(ar) = props.get("actionRef").filter(|v| !v.is_null())
        && !d.contains_key("link")
    {
        d.insert("link".to_string(), json!({ "actionRef": ar }));
    }
    d
}

/// CmxColumn._normalizeEdit 等价。
fn normalize_edit(props: &Map<String, Value>) -> Map<String, Value> {
    let es = props
        .get("editSettings")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut e = es.clone();
    if let Some(o) = props.get("edit").and_then(|v| v.as_object()) {
        for (k, v) in o {
            e.insert(k.clone(), v.clone());
        }
    }
    if !e.contains_key("mode") {
        e.insert("mode".to_string(), json!("cmx-text-input"));
    }
    if !e.contains_key("required")
        && let Some(v) = props.get("required").filter(|v| !v.is_null())
    {
        e.insert("required".to_string(), v.clone());
    }
    if !e.contains_key("validate")
        && let Some(v) = props.get("validateFormula").filter(|v| !v.is_null())
    {
        e.insert("validate".to_string(), v.clone());
    }
    if !e.contains_key("trigger") {
        e.insert("trigger".to_string(), json!("inherit"));
    }
    if !e.contains_key("parent")
        && let Some(v) = es.get("parentField")
    {
        e.insert("parent".to_string(), v.clone());
    }
    e
}

/// 函数值在 JSON 中为 null（这里没有函数，原样保留，仅做 null 兜底）。
fn strip_fn(v: Option<Value>) -> Value {
    v.unwrap_or(Value::Null)
}

/// 浅拷贝（serde_json 不会有函数值，直接克隆）。
fn strip_fns(obj: &Map<String, Value>) -> Map<String, Value> {
    obj.clone()
}
