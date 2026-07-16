//! 列构建相关方法（`build_columns` 及其派生链）。
//!
//! 复刻 Node `flexible-combination-engine.js` 的 `_fieldToColumn` 全派生 + `CmxColumn.toJSON` 形状。

use serde_json::{Map, Value, json};

use super::{field_caption, field_data_type, field_id, is_numeric_type, value_to_string};

impl<'a> super::Engine<'a> {
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
                // 历史包袱:前端组件注册名即 `cmx-dict-selct`(select 误拼),见 EDIT_MODES 注释。
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
                    disp.as_object_mut()
                        .expect("invariant: disp 由 json!({{...}}) 构造,必为对象")
                        .insert(k.clone(), v.clone());
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
}

/// CmxColumn._normalizeDisplay 等价。
pub(super) fn normalize_display(props: &Map<String, Value>) -> Map<String, Value> {
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
pub(super) fn normalize_edit(props: &Map<String, Value>) -> Map<String, Value> {
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
pub(super) fn strip_fn(v: Option<Value>) -> Value {
    v.unwrap_or(Value::Null)
}

/// 浅拷贝（serde_json 不会有函数值，直接克隆）。
pub(super) fn strip_fns(obj: &Map<String, Value>) -> Map<String, Value> {
    obj.clone()
}
