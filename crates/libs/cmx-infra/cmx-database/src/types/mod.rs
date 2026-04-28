/// 类型安全模块
///
/// 提供编译时类型检查的 SQL 查询构建器
use crate::executor::ParamValue;

/// SQL 关键字
#[derive(Debug, Clone)]
pub enum SqlKeyword {
    Select,
    Insert,
    Update,
    Delete,
    From,
    Where,
    OrderBy,
    GroupBy,
    Having,
    Join(JoinType),
    Limit,
    Offset,
    Set,
    Values,
    And,
    Or,
}

/// 连接类型
#[derive(Debug, Clone)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// 比较运算符
#[derive(Debug, Clone)]
pub enum CompareOp {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Like,
    NotLike,
    In,
    NotIn,
}

impl CompareOp {
    pub fn to_sql_string(&self) -> &'static str {
        match self {
            CompareOp::Equal => "=",
            CompareOp::NotEqual => "<>",
            CompareOp::GreaterThan => ">",
            CompareOp::GreaterThanOrEqual => ">=",
            CompareOp::LessThan => "<",
            CompareOp::LessThanOrEqual => "<=",
            CompareOp::Like => "LIKE",
            CompareOp::NotLike => "NOT LIKE",
            CompareOp::In => "IN",
            CompareOp::NotIn => "NOT IN",
        }
    }
}

pub enum CompareOpEx {
    IsNull,
    IsNotNull,
}

/// 逻辑运算符
#[derive(Debug, Clone)]
pub enum LogicalOp {
    And,
    Or,
}

impl LogicalOp {
    pub fn to_sql_string(&self) -> &'static str {
        match self {
            LogicalOp::And => "AND",
            LogicalOp::Or => "OR",
        }
    }
}

/// 值表达式
#[derive(Debug, Clone)]
pub enum ValueExpr {
    Column(String),
    Param(usize),
    Value(ParamValue),
}

impl ValueExpr {
    pub fn to_sql_string(&self) -> String {
        match self {
            ValueExpr::Column(name) => name.clone(),
            ValueExpr::Param(idx) => format!("${}", idx),
            ValueExpr::Value(_) => "?".to_string(),
        }
    }

    pub fn param_count(&self) -> usize {
        match self {
            ValueExpr::Column(_) => 0,
            ValueExpr::Param(_) => 1,
            ValueExpr::Value(_) => 1,
        }
    }
}

/// 条件表达式
#[derive(Debug, Clone)]
pub enum ConditionExpr {
    Compare {
        left: ValueExpr,
        op: CompareOp,
        right: ValueExpr,
    },
    Logical {
        left: Box<ConditionExpr>,
        op: LogicalOp,
        right: Box<ConditionExpr>,
    },
    Nested(Box<ConditionExpr>),
}

impl ConditionExpr {
    pub fn to_sql_string(&self, start_index: usize) -> String {
        match self {
            ConditionExpr::Compare { left, op, .. } => {
                format!("{} {} ${}", left.to_sql_string(), op.to_sql_string(), start_index + 1)
            },
            ConditionExpr::Logical { left, op, right } => {
                format!("({} {} {})", left.to_sql_string(start_index), op.to_sql_string(), right.to_sql_string(start_index))
            },
            ConditionExpr::Nested(inner) => {
                format!("({})", inner.to_sql_string(start_index))
            },
        }
    }

    pub fn param_count(&self) -> usize {
        match self {
            ConditionExpr::Compare { left, right, .. } => left.param_count() + right.param_count(),
            ConditionExpr::Logical { left, right, .. } => left.param_count() + right.param_count(),
            ConditionExpr::Nested(inner) => inner.param_count(),
        }
    }
}

/// 查询类型
#[derive(Debug, Clone)]
pub enum QueryType {
    Select,
    Insert(InsertValues),
    Update(Vec<(String, ValueExpr)>),
    Delete,
}

/// 插入值
#[derive(Debug, Clone)]
pub enum InsertValues {
    Values(Vec<Vec<ValueExpr>>),
    Select(String),
}

/// 排序方向
#[derive(Debug, Clone)]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// 连接子句
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct JoinClause {
    join_type: JoinType,
    table_name: String,
    condition: ConditionExpr,
}

/// 查询构建器
#[allow(dead_code)]
pub struct QueryBuilder {
    query_type: QueryType,
    table_name: String,
    columns: Vec<String>,
    conditions: Vec<ConditionExpr>,
    order_by: Vec<(String, OrderDirection)>,
    group_by: Vec<String>,
    limit_value: Option<usize>,
    offset_value: Option<usize>,
    join_clauses: Vec<JoinClause>,
    param_index: usize,
}

/// 类型安全的查询结果
pub struct TypedRow {
    values: Vec<ParamValue>,
}

impl TypedRow {
    pub fn new(values: Vec<ParamValue>) -> Self {
        Self { values }
    }

    pub fn get_string(&self, index: usize) -> Option<&str> {
        match self.values.get(index)? {
            ParamValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn get_i64(&self, index: usize) -> Option<i64> {
        match self.values.get(index)? {
            ParamValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn get_f64(&self, index: usize) -> Option<f64> {
        match self.values.get(index)? {
            ParamValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn get_bool(&self, index: usize) -> Option<bool> {
        match self.values.get(index)? {
            ParamValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// 类型安全的查询结果集
#[allow(dead_code)]
pub struct TypedResult {
    rows: Vec<TypedRow>,
    columns: Vec<String>,
}

impl TypedResult {
    pub fn new(rows: Vec<TypedRow>, columns: Vec<String>) -> Self {
        Self { rows, columns }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn rows(&self) -> &[TypedRow] {
        &self.rows
    }

    pub fn first(&self) -> Option<&TypedRow> {
        self.rows.first()
    }
}

impl QueryBuilder {
    pub fn new(table_name: &str) -> Self {
        Self {
            query_type: QueryType::Select,
            table_name: table_name.to_string(),
            columns: vec![],
            conditions: vec![],
            order_by: vec![],
            group_by: vec![],
            limit_value: None,
            offset_value: None,
            join_clauses: vec![],
            param_index: 0,
        }
    }

    pub fn select(columns: Vec<&str>) -> Self {
        let mut builder = Self::new("");
        builder.query_type = QueryType::Select;
        builder.columns = columns.iter().map(|s| s.to_string()).collect();
        builder
    }

    pub fn from(&mut self, table_name: &str) -> &mut Self {
        self.table_name = table_name.to_string();
        self
    }

    pub fn column(&mut self, column: &str) -> &mut Self {
        self.columns.push(column.to_string());
        self
    }

    pub fn where_condition(&mut self, condition: ConditionExpr) -> &mut Self {
        self.conditions.push(condition);
        self
    }

    pub fn eq(&mut self, column: &str, value: ParamValue) -> &mut Self {
        let expr = ConditionExpr::Compare {
            left: ValueExpr::Column(column.to_string()),
            op: CompareOp::Equal,
            right: ValueExpr::Value(value),
        };
        self.conditions.push(expr);
        self
    }

    pub fn gt(&mut self, column: &str, value: ParamValue) -> &mut Self {
        let expr = ConditionExpr::Compare {
            left: ValueExpr::Column(column.to_string()),
            op: CompareOp::GreaterThan,
            right: ValueExpr::Value(value),
        };
        self.conditions.push(expr);
        self
    }

    pub fn lt(&mut self, column: &str, value: ParamValue) -> &mut Self {
        let expr = ConditionExpr::Compare {
            left: ValueExpr::Column(column.to_string()),
            op: CompareOp::LessThan,
            right: ValueExpr::Value(value),
        };
        self.conditions.push(expr);
        self
    }

    pub fn like(&mut self, column: &str, value: &str) -> &mut Self {
        let expr = ConditionExpr::Compare {
            left: ValueExpr::Column(column.to_string()),
            op: CompareOp::Like,
            right: ValueExpr::Value(ParamValue::String(value.to_string())),
        };
        self.conditions.push(expr);
        self
    }

    pub fn in_values(&mut self, column: &str, values: Vec<ParamValue>) -> &mut Self {
        let expr = ConditionExpr::Compare {
            left: ValueExpr::Column(column.to_string()),
            op: CompareOp::In,
            right: ValueExpr::Value(ParamValue::String(format!("{:?}", values))),
        };
        self.conditions.push(expr);
        self
    }

    pub fn order_by(&mut self, column: &str, direction: OrderDirection) -> &mut Self {
        self.order_by.push((column.to_string(), direction));
        self
    }

    pub fn asc(&mut self, column: &str) -> &mut Self {
        self.order_by.push((column.to_string(), OrderDirection::Asc));
        self
    }

    pub fn desc(&mut self, column: &str) -> &mut Self {
        self.order_by.push((column.to_string(), OrderDirection::Desc));
        self
    }

    pub fn group_by(&mut self, columns: Vec<&str>) -> &mut Self {
        self.group_by = columns.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn limit(&mut self, limit: usize) -> &mut Self {
        self.limit_value = Some(limit);
        self
    }

    pub fn offset(&mut self, offset: usize) -> &mut Self {
        self.offset_value = Some(offset);
        self
    }

    pub fn join(&mut self, table: &str, condition: ConditionExpr) -> &mut Self {
        self.join_clauses.push(JoinClause {
            join_type: JoinType::Inner,
            table_name: table.to_string(),
            condition,
        });
        self
    }

    pub fn left_join(&mut self, table: &str, condition: ConditionExpr) -> &mut Self {
        self.join_clauses.push(JoinClause {
            join_type: JoinType::Left,
            table_name: table.to_string(),
            condition,
        });
        self
    }

    pub fn to_sql(&mut self) -> (String, Vec<ParamValue>) {
        let mut sql = String::new();
        let params = Vec::new();

        match &self.query_type {
            QueryType::Select => {
                sql.push_str("SELECT ");
                if self.columns.is_empty() {
                    sql.push('*');
                } else {
                    sql.push_str(&self.columns.join(", "));
                }
                sql.push_str(" FROM ");
                sql.push_str(&self.table_name);
            },
            QueryType::Delete => {
                sql.push_str("DELETE FROM ");
                sql.push_str(&self.table_name);
            },
            QueryType::Insert(values) => {
                sql.push_str("INSERT INTO ");
                sql.push_str(&self.table_name);
                match values {
                    InsertValues::Values(vals) => {
                        sql.push_str(" (");
                        let mut first = true;
                        for _val in vals {
                            if !first {
                                sql.push_str(", ");
                            }
                            first = false;
                        }
                        sql.push_str(") VALUES (");
                        first = true;
                        for _val in vals {
                            if !first {
                                sql.push_str(", ");
                            }
                            sql.push('?');
                            first = false;
                        }
                        sql.push(')');
                    },
                    InsertValues::Select(s) => {
                        sql.push(' ');
                        sql.push_str(s);
                    },
                }
            },
            QueryType::Update(sets) => {
                sql.push_str("UPDATE ");
                sql.push_str(&self.table_name);
                sql.push_str(" SET ");
                let mut first = true;
                for (col, _) in sets {
                    if !first {
                        sql.push_str(", ");
                    }
                    sql.push_str(col);
                    sql.push_str(" = ?");
                    first = false;
                }
            },
        }

        for join in &self.join_clauses {
            match join.join_type {
                JoinType::Inner => sql.push_str(" INNER JOIN "),
                JoinType::Left => sql.push_str(" LEFT JOIN "),
                JoinType::Right => sql.push_str(" RIGHT JOIN "),
                JoinType::Full => sql.push_str(" FULL JOIN "),
            }
            sql.push_str(&join.table_name);
        }

        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            for (i, cond) in self.conditions.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                sql.push_str(&cond.to_sql_string(self.param_index));
                self.param_index += cond.param_count();
            }
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            let mut first = true;
            for (col, dir) in &self.order_by {
                if !first {
                    sql.push_str(", ");
                }
                sql.push_str(col);
                match dir {
                    OrderDirection::Asc => sql.push_str(" ASC"),
                    OrderDirection::Desc => sql.push_str(" DESC"),
                }
                first = false;
            }
        }

        if let Some(limit) = self.limit_value {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset_value {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        (sql, params)
    }
}

/// WHERE 条件构建器
pub struct WhereClause {
    conditions: Vec<ConditionExpr>,
}

impl WhereClause {
    pub fn new() -> Self {
        Self { conditions: vec![] }
    }

    pub fn and(&mut self, condition: ConditionExpr) -> &mut Self {
        self.conditions.push(condition);
        self
    }

    pub fn build(&self) -> Vec<ConditionExpr> {
        self.conditions.clone()
    }
}

impl Default for WhereClause {
    fn default() -> Self {
        Self::new()
    }
}
