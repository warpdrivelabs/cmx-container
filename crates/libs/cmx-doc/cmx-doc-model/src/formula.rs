//! formula — 后端安全表达式求值器（方案 §14.2 / §15 T1，落地 Phase 9）
//!
//! 与前端 `formula-eval.js` **语义对齐的子集**：支持校验/规则场景最常用的
//!   - 字面量：数字 / 'str' / "str" / true / false / null
//!   - 字段引用：标识符从 scope 取值（缺失按数值 0，与前端一致）
//!   - 算术：+ - * /、一元负号
//!   - 比较：> < >= <= == !=
//!   - 逻辑：&& || !
//!   - 分组：( )
//!   - 函数：ABS/MIN/MAX/ROUND/IF/AND/OR/NOT/ISEMPTY/COALESCE（可扩展）
//!
//! 不用裸 eval：自带词法 + 递归下降解析 + 求值，安全（可跑在保存前校验）。
//! 定位：覆盖「行内字段计算 / 借贷平衡类校验」，非图灵完备 DSL；更复杂逻辑走 §15 T2/T3。

use std::collections::HashMap;

/// 求值结果值。
#[derive(Debug, Clone, PartialEq)]
pub enum FValue {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl FValue {
    pub fn as_num(&self) -> f64 {
        match self {
            FValue::Num(n) => *n,
            FValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            FValue::Str(s) => s.parse().unwrap_or(0.0),
            FValue::Null => 0.0,
        }
    }
    pub fn as_bool(&self) -> bool {
        match self {
            FValue::Bool(b) => *b,
            FValue::Num(n) => *n != 0.0,
            FValue::Str(s) => !s.is_empty(),
            FValue::Null => false,
        }
    }
    pub fn is_empty(&self) -> bool {
        matches!(self, FValue::Null) || matches!(self, FValue::Str(s) if s.is_empty())
    }
}

/// 求值上下文：字段名 → 值。
pub type Scope = HashMap<String, FValue>;

/// 从 serde_json::Value（行对象）构建 scope。
pub fn scope_from_json(row: &serde_json::Value) -> Scope {
    let mut s = Scope::new();
    if let Some(obj) = row.as_object() {
        for (k, v) in obj {
            s.insert(k.clone(), json_to_fvalue(v));
        }
    }
    s
}

fn json_to_fvalue(v: &serde_json::Value) -> FValue {
    match v {
        serde_json::Value::Null => FValue::Null,
        serde_json::Value::Bool(b) => FValue::Bool(*b),
        serde_json::Value::Number(n) => FValue::Num(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => {
            // 数值字符串（如 Decimal 序列化为字符串）尽量当数字
            if let Ok(n) = s.parse::<f64>() {
                FValue::Num(n)
            } else {
                FValue::Str(s.clone())
            }
        }
        _ => FValue::Null,
    }
}

/// 求值表达式；解析或求值失败返回 Err。
pub fn eval_formula(expr: &str, scope: &Scope) -> Result<FValue, String> {
    let tokens = lex(expr)?;
    let mut p = Parser { tokens, pos: 0 };
    let node = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(format!("表达式有多余 token @ {}", p.pos));
    }
    eval_node(&node, scope)
}

/// 便捷：求值为布尔（校验用）。失败按 fallback。
pub fn eval_bool(expr: &str, scope: &Scope, fallback: bool) -> bool {
    eval_formula(expr, scope)
        .map(|v| v.as_bool())
        .unwrap_or(fallback)
}

// ─────────────────────── 词法 ───────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Op(String), // + - * / > < >= <= == != && || !
    LParen,
    RParen,
    Comma,
    True,
    False,
    Null,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '+' | '-' | '*' | '/' => {
                out.push(Tok::Op(c.to_string()));
                i += 1;
            }
            '>' | '<' | '=' | '!' => {
                if i + 1 < cs.len() && cs[i + 1] == '=' {
                    out.push(Tok::Op(format!("{c}=")));
                    i += 2;
                } else if c == '!' {
                    out.push(Tok::Op("!".into()));
                    i += 1;
                } else if c == '=' {
                    return Err("单个 = 非法（用 ==）".into());
                } else {
                    out.push(Tok::Op(c.to_string()));
                    i += 1;
                }
            }
            '&' | '|' => {
                if i + 1 < cs.len() && cs[i + 1] == c {
                    out.push(Tok::Op(format!("{c}{c}")));
                    i += 2;
                } else {
                    return Err(format!("非法运算符 {c}"));
                }
            }
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut buf = String::new();
                while i < cs.len() && cs[i] != quote {
                    buf.push(cs[i]);
                    i += 1;
                }
                if i >= cs.len() {
                    return Err("字符串未闭合".into());
                }
                i += 1; // 跳过闭合引号
                out.push(Tok::Str(buf));
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let mut buf = String::new();
                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                    buf.push(cs[i]);
                    i += 1;
                }
                let n: f64 = buf.parse().map_err(|_| format!("非法数字 {buf}"))?;
                out.push(Tok::Num(n));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let mut buf = String::new();
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '.') {
                    buf.push(cs[i]);
                    i += 1;
                }
                match buf.to_ascii_lowercase().as_str() {
                    "true" => out.push(Tok::True),
                    "false" => out.push(Tok::False),
                    "null" => out.push(Tok::Null),
                    _ => out.push(Tok::Ident(buf)),
                }
            }
            _ => return Err(format!("非法字符 {c}")),
        }
    }
    Ok(out)
}

// ─────────────────────── 语法（递归下降） ───────────────────────

#[derive(Debug, Clone)]
enum Node {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Ident(String),
    Unary(String, Box<Node>),
    Binary(String, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    // expr := or
    fn parse_expr(&mut self) -> Result<Node, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Node, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Op(o)) if o == "||") {
            self.next();
            let right = self.parse_and()?;
            left = Node::Binary("||".into(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Node, String> {
        let mut left = self.parse_cmp()?;
        while matches!(self.peek(), Some(Tok::Op(o)) if o == "&&") {
            self.next();
            let right = self.parse_cmp()?;
            left = Node::Binary("&&".into(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_cmp(&mut self) -> Result<Node, String> {
        let mut left = self.parse_add()?;
        while let Some(Tok::Op(o)) = self.peek() {
            if matches!(o.as_str(), ">" | "<" | ">=" | "<=" | "==" | "!=") {
                let op = o.clone();
                self.next();
                let right = self.parse_add()?;
                left = Node::Binary(op, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }
    fn parse_add(&mut self) -> Result<Node, String> {
        let mut left = self.parse_mul()?;
        while let Some(Tok::Op(o)) = self.peek() {
            if o == "+" || o == "-" {
                let op = o.clone();
                self.next();
                let right = self.parse_mul()?;
                left = Node::Binary(op, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }
    fn parse_mul(&mut self) -> Result<Node, String> {
        let mut left = self.parse_unary()?;
        while let Some(Tok::Op(o)) = self.peek() {
            if o == "*" || o == "/" {
                let op = o.clone();
                self.next();
                let right = self.parse_unary()?;
                left = Node::Binary(op, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Node, String> {
        if let Some(Tok::Op(o)) = self.peek()
            && (o == "-" || o == "!")
        {
            let op = o.clone();
            self.next();
            let operand = self.parse_unary()?;
            return Ok(Node::Unary(op, Box::new(operand)));
        }
        self.parse_primary()
    }
    fn parse_primary(&mut self) -> Result<Node, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Node::Num(n)),
            Some(Tok::Str(s)) => Ok(Node::Str(s)),
            Some(Tok::True) => Ok(Node::Bool(true)),
            Some(Tok::False) => Ok(Node::Bool(false)),
            Some(Tok::Null) => Ok(Node::Null),
            Some(Tok::LParen) => {
                let e = self.parse_expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err("缺少 )".into()),
                }
            }
            Some(Tok::Ident(name)) => {
                // 函数调用？
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.next(); // (
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_expr()?);
                            match self.peek() {
                                Some(Tok::Comma) => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    match self.next() {
                        Some(Tok::RParen) => Ok(Node::Call(name.to_ascii_uppercase(), args)),
                        _ => Err("函数缺少 )".into()),
                    }
                } else {
                    Ok(Node::Ident(name))
                }
            }
            other => Err(format!("意外 token: {other:?}")),
        }
    }
}

// ─────────────────────── 求值 ───────────────────────

fn eval_node(node: &Node, scope: &Scope) -> Result<FValue, String> {
    match node {
        Node::Num(n) => Ok(FValue::Num(*n)),
        Node::Str(s) => Ok(FValue::Str(s.clone())),
        Node::Bool(b) => Ok(FValue::Bool(*b)),
        Node::Null => Ok(FValue::Null),
        // 字段引用：缺失按 0（与前端一致）
        Node::Ident(name) => Ok(scope.get(name).cloned().unwrap_or(FValue::Num(0.0))),
        Node::Unary(op, operand) => {
            let v = eval_node(operand, scope)?;
            match op.as_str() {
                "-" => Ok(FValue::Num(-v.as_num())),
                "!" => Ok(FValue::Bool(!v.as_bool())),
                _ => Err(format!("未知一元运算符 {op}")),
            }
        }
        Node::Binary(op, l, r) => {
            // 逻辑短路
            if op == "&&" {
                let lv = eval_node(l, scope)?;
                if !lv.as_bool() {
                    return Ok(FValue::Bool(false));
                }
                return Ok(FValue::Bool(eval_node(r, scope)?.as_bool()));
            }
            if op == "||" {
                let lv = eval_node(l, scope)?;
                if lv.as_bool() {
                    return Ok(FValue::Bool(true));
                }
                return Ok(FValue::Bool(eval_node(r, scope)?.as_bool()));
            }
            let lv = eval_node(l, scope)?;
            let rv = eval_node(r, scope)?;
            match op.as_str() {
                "+" => {
                    // 字符串相加做拼接，否则数值
                    if matches!(lv, FValue::Str(_)) || matches!(rv, FValue::Str(_)) {
                        Ok(FValue::Str(format!("{}{}", to_str(&lv), to_str(&rv))))
                    } else {
                        Ok(FValue::Num(lv.as_num() + rv.as_num()))
                    }
                }
                "-" => Ok(FValue::Num(lv.as_num() - rv.as_num())),
                "*" => Ok(FValue::Num(lv.as_num() * rv.as_num())),
                "/" => {
                    let d = rv.as_num();
                    Ok(FValue::Num(if d == 0.0 { 0.0 } else { lv.as_num() / d }))
                }
                ">" => Ok(FValue::Bool(lv.as_num() > rv.as_num())),
                "<" => Ok(FValue::Bool(lv.as_num() < rv.as_num())),
                ">=" => Ok(FValue::Bool(lv.as_num() >= rv.as_num())),
                "<=" => Ok(FValue::Bool(lv.as_num() <= rv.as_num())),
                "==" => Ok(FValue::Bool(values_eq(&lv, &rv))),
                "!=" => Ok(FValue::Bool(!values_eq(&lv, &rv))),
                _ => Err(format!("未知运算符 {op}")),
            }
        }
        Node::Call(name, args) => eval_call(name, args, scope),
    }
}

fn eval_call(name: &str, args: &[Node], scope: &Scope) -> Result<FValue, String> {
    let ev = |n: &Node| eval_node(n, scope);
    match name {
        "ABS" => Ok(FValue::Num(ev(&args[0])?.as_num().abs())),
        "ROUND" => {
            let x = ev(&args[0])?.as_num();
            let digits = args
                .get(1)
                .map(&ev)
                .transpose()?
                .map(|v| v.as_num() as i32)
                .unwrap_or(0);
            let f = 10f64.powi(digits);
            Ok(FValue::Num((x * f).round() / f))
        }
        "MIN" => {
            let mut m = f64::INFINITY;
            for a in args {
                m = m.min(ev(a)?.as_num());
            }
            Ok(FValue::Num(m))
        }
        "MAX" => {
            let mut m = f64::NEG_INFINITY;
            for a in args {
                m = m.max(ev(a)?.as_num());
            }
            Ok(FValue::Num(m))
        }
        "SUM" => {
            let mut s = 0.0;
            for a in args {
                s += ev(a)?.as_num();
            }
            Ok(FValue::Num(s))
        }
        "IF" => {
            let cond = ev(&args[0])?.as_bool();
            if cond {
                ev(&args[1])
            } else {
                args.get(2)
                    .map(ev)
                    .transpose()?
                    .ok_or_else(|| "IF 缺少 else".to_string())
            }
        }
        "AND" => {
            for a in args {
                if !ev(a)?.as_bool() {
                    return Ok(FValue::Bool(false));
                }
            }
            Ok(FValue::Bool(true))
        }
        "OR" => {
            for a in args {
                if ev(a)?.as_bool() {
                    return Ok(FValue::Bool(true));
                }
            }
            Ok(FValue::Bool(false))
        }
        "NOT" => Ok(FValue::Bool(!ev(&args[0])?.as_bool())),
        "ISEMPTY" => Ok(FValue::Bool(ev(&args[0])?.is_empty())),
        "COALESCE" => {
            for a in args {
                let v = ev(a)?;
                if !v.is_empty() {
                    return Ok(v);
                }
            }
            Ok(FValue::Null)
        }
        _ => Err(format!("未知函数 {name}")),
    }
}

fn to_str(v: &FValue) -> String {
    match v {
        FValue::Str(s) => s.clone(),
        FValue::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        FValue::Bool(b) => b.to_string(),
        FValue::Null => String::new(),
    }
}

fn values_eq(a: &FValue, b: &FValue) -> bool {
    match (a, b) {
        (FValue::Str(x), FValue::Str(y)) => x == y,
        (FValue::Null, FValue::Null) => true,
        (FValue::Bool(x), FValue::Bool(y)) => x == y,
        _ => (a.as_num() - b.as_num()).abs() < 1e-9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(pairs: &[(&str, f64)]) -> Scope {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), FValue::Num(*v)))
            .collect()
    }

    #[test]
    fn arithmetic() {
        let s = scope(&[("qty", 3.0), ("price", 10.0)]);
        assert_eq!(eval_formula("qty * price", &s).unwrap(), FValue::Num(30.0));
        assert_eq!(eval_formula("(1 + 2) * 4", &s).unwrap(), FValue::Num(12.0));
        assert_eq!(eval_formula("-price", &s).unwrap(), FValue::Num(-10.0));
    }

    #[test]
    fn debit_credit_balance() {
        // 核心校验场景：借贷平衡
        let s = scope(&[("total_dr", 1130000.0), ("total_cr", 1130000.0)]);
        assert!(eval_bool("total_dr == total_cr", &s, false));
        let s2 = scope(&[("total_dr", 100.0), ("total_cr", 90.0)]);
        assert!(!eval_bool("total_dr == total_cr", &s2, false));
    }

    #[test]
    fn comparison_and_logic() {
        let s = scope(&[("a", 5.0), ("b", 3.0)]);
        assert!(eval_bool("a > b", &s, false));
        assert!(eval_bool("a > b && b > 0", &s, false));
        assert!(!eval_bool("a < b || b > 10", &s, false));
        assert!(eval_bool("!(a < b)", &s, false));
    }

    #[test]
    fn missing_field_is_zero() {
        let s = Scope::new();
        // 与前端一致：缺失字段按 0
        assert_eq!(eval_formula("qty * 5", &s).unwrap(), FValue::Num(0.0));
        assert!(eval_bool("qty == 0", &s, false));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn functions() {
        let s = scope(&[("x", -7.0)]);
        assert_eq!(eval_formula("ABS(x)", &s).unwrap(), FValue::Num(7.0));
        assert_eq!(eval_formula("MAX(1, 5, 3)", &s).unwrap(), FValue::Num(5.0));
        assert_eq!(eval_formula("SUM(1, 2, 3)", &s).unwrap(), FValue::Num(6.0));
        assert_eq!(
            eval_formula("ROUND(3.14159, 2)", &s).unwrap(),
            FValue::Num(3.14)
        );
        assert_eq!(
            eval_formula("IF(x < 0, 'neg', 'pos')", &s).unwrap(),
            FValue::Str("neg".into())
        );
    }

    #[test]
    fn string_literals_and_concat() {
        let s = Scope::new();
        assert_eq!(
            eval_formula("'a' + 'b'", &s).unwrap(),
            FValue::Str("ab".into())
        );
        assert!(eval_bool("'posted' == 'posted'", &s, false));
    }

    #[test]
    fn scope_from_json_works() {
        let row = serde_json::json!({ "total_dr": 100, "total_cr": "100", "note": "x" });
        let s = scope_from_json(&row);
        // Decimal 字符串 "100" 当数字
        assert!(eval_bool("total_dr == total_cr", &s, false));
    }

    #[test]
    fn bad_expr_returns_err_or_fallback() {
        let s = Scope::new();
        assert!(eval_formula("1 +", &s).is_err());
        assert!(eval_bool("1 +", &s, true)); // fallback
    }
}
