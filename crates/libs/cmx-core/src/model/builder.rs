//! 动态 UPDATE SET 子句 + 参数构造器。
//!
//! 解决手写动态 UPDATE 时「SQL SET 子句顺序」与「params push 顺序」
//! 必须双重一致、极易漂移的问题。builder 自动管理占位符编号。
//!
//! 纯域构造工具,无 sqlx 依赖,wasm 可用。

use crate::model::cell::DataValue;

/// 动态 SET 子句构造器。
///
/// 自动管理 `$N` 占位符编号,消除「SQL SET 子句顺序」与
/// 「params Vec 顺序」必须双重一致的漂移风险。
///
/// # 示例
///
/// ```
/// use cmx_core::ParamsBuilder;
/// use cmx_core::model::cell::DataValue;
///
/// let mut b = ParamsBuilder::new(1); // WHERE id = $1 已占用,SET 从 $2 起
/// b.set("name", "alice".to_string())
///  .set_opt("sort_order", Some(5_i64))
///  .set_opt("description", None::<String>); // None → 跳过该列
/// let (set_clause, params) = b.build();
/// assert_eq!(set_clause, "name = $2, sort_order = $3");
/// ```
pub struct ParamsBuilder {
    assignments: Vec<String>,
    params: Vec<DataValue>,
    next_index: usize,
}

impl ParamsBuilder {
    /// 创建 builder,占位符从 `start_offset + 1` 开始编号。
    ///
    /// `start_offset` = 已被占用的占位符数。
    /// 例如 WHERE 子句已用 `$1`,SET 子句应从 `$2` 起,则传 `1`。
    pub fn new(start_offset: usize) -> Self {
        Self {
            assignments: Vec::new(),
            params: Vec::new(),
            next_index: start_offset + 1,
        }
    }

    /// 添加必填列赋值。`val` 须满足 `Into<DataValue>`。
    pub fn set(&mut self, col: &str, val: impl Into<DataValue>) -> &mut Self {
        let idx = self.next_index;
        self.next_index += 1;
        self.assignments.push(format!("{col} = ${idx}"));
        self.params.push(val.into());
        self
    }

    /// 添加可选列赋值。`None` 时**跳过该列**(不加入 SET),避免无谓赋值。
    pub fn set_opt(&mut self, col: &str, val: Option<impl Into<DataValue>>) -> &mut Self {
        if let Some(v) = val {
            self.set(col, v.into());
        }
        self
    }

    /// 添加可选列赋值(None 时仍写入 NULL,带类型)。
    ///
    /// 与 [`set_opt`](Self::set_opt) 区别:None 会写入 `SET col = NULL`,
    /// 而非跳过该列。适合语义上需要显式置 NULL 的场景。
    pub fn set_opt_null(&mut self, col: &str, val: Option<impl Into<DataValue>>) -> &mut Self {
        self.set(col, val.map(Into::into).unwrap_or(DataValue::Null));
        self
    }

    /// 返回 (`"col1 = $2, col2 = $3"`, params)。
    ///
    /// 若无任何赋值,返回空字符串(调用方应处理「无字段更新」的情况)。
    pub fn build(self) -> (String, Vec<DataValue>) {
        let clause = self.assignments.join(", ");
        (clause, self.params)
    }

    /// 返回当前已添加的赋值数量。
    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    /// 是否没有任何赋值。
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    /// 返回下一个将分配的占位符编号(即 `$N` 中的 N)。
    pub fn next_placeholder(&self) -> usize {
        self.next_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_basic() {
        let mut b = ParamsBuilder::new(1);
        b.set("name", "alice".to_string())
         .set_opt("age", Some(30_i64));
        let (clause, params) = b.build();
        assert_eq!(clause, "name = $2, age = $3");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], DataValue::String("alice".into()));
        assert_eq!(params[1], DataValue::Int(30));
    }

    #[test]
    fn set_opt_none_skips() {
        let mut b = ParamsBuilder::new(0);
        b.set("a", "x".to_string())
         .set_opt("b", None::<String>);
        let (clause, params) = b.build();
        assert_eq!(clause, "a = $1");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn empty_builder() {
        let b = ParamsBuilder::new(0);
        assert!(b.is_empty());
        let (clause, params) = b.build();
        assert_eq!(clause, "");
        assert!(params.is_empty());
    }

    #[test]
    fn set_opt_null_writes_null() {
        let mut b = ParamsBuilder::new(0);
        b.set_opt_null("desc", None::<String>);
        let (clause, params) = b.build();
        assert_eq!(clause, "desc = $1");
        assert_eq!(params[0], DataValue::Null);
    }

    #[test]
    fn option_int_null_typed() {
        let mut b = ParamsBuilder::new(0);
        b.set_opt("count", None::<i64>);
        let (clause, params) = b.build();
        // None 跳过
        assert_eq!(clause, "");
        assert!(params.is_empty());
    }

    #[test]
    fn option_int_some_writes_typed() {
        let mut b = ParamsBuilder::new(0);
        b.set_opt("count", Some(42_i64));
        let (clause, params) = b.build();
        assert_eq!(clause, "count = $1");
        assert_eq!(params[0], DataValue::Int(42));
    }

    #[test]
    fn start_offset_zero() {
        let mut b = ParamsBuilder::new(0);
        b.set("a", "x".to_string());
        let (clause, _) = b.build();
        assert_eq!(clause, "a = $1");
    }

    #[test]
    fn multiple_columns_with_offset() {
        let mut b = ParamsBuilder::new(2); // $1, $2 已占用
        b.set("a", "x".to_string())
         .set("b", 1_i64)
         .set("c", true);
        let (clause, params) = b.build();
        assert_eq!(clause, "a = $3, b = $4, c = $5");
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn len_and_is_empty() {
        let mut b = ParamsBuilder::new(0);
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        b.set("a", "x".to_string());
        assert!(!b.is_empty());
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn next_placeholder_after_build() {
        let mut b = ParamsBuilder::new(1);
        b.set("a", "x".to_string());
        assert_eq!(b.next_placeholder(), 3); // 下一个是 $3
    }
}
