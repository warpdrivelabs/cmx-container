# cmx-code-model

> 通用业务编码引擎的纯逻辑层：编码规则类型定义（`RuleSpec` / `CodeRule`）、七种段类型求值、补位/截断/校验位与断号反解析等纯算法——无 DB、无 HTTP 依赖，DB 操作抽象为 `Advance` trait 由 `cmx-code-api` 注入实现。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

---

## 项目简介

`cmx-code-model` 是 cmx-code 域二件套中的**领域模型层**。业务编码指单据号、主数据 code
等企业级标识（如 `FV202608040002` = 固定前缀 + 日期 + 按日重置流水），本 crate 只描述
"编码长什么样、怎么算"：段序列求值、流水推进、断号反解析、补位截断全部是纯函数，
无任何数据库或 Web 框架依赖——反查 max、取断号、UNIQUE 预检抽象为 `Advance` trait，
由 `cmx-code-api` 的 `PgAdvance` 实现注入；单测用 `StubAdvance` 即可跑通全链路，零 DB。

### 两层模型（规则算法 × 挂载点声明）

- **规则算法（`RuleSpec`，存 `cmx_code_rule` 表）**：纯段序列 + 连接符 `joiner` + 校验正则
  `pattern` + 优先级 `priority`，一条规则可被任意多个字典/单据复用，不含 target 信息。
- **挂载点声明（`CodeRule`，存 DCT/DOC 定义）**：引用规则码 + 声明 target 行为——写回列
  `field`（引擎只认这个，不硬编码列名）、`mode`（auto/manual）、局部覆盖
  `enableGap`/`pattern`、级联回填 `cascade`（放挂载点不放规则表，避免污染多目标复用）；
  `CodeRule::merge_with` 把两层合并为可执行规则，挂载点覆盖优先。

### 段求值三态（`SegmentValue`）

| 态 | 含义 | 返回该态的段 |
|----|------|-------------|
| `Literal(String)` | 定值直接拼接 | const / date / ref / custom |
| `NeedsSerial { reset_key, width, step, start, pad_char, pad_side }` | 交流水推进器反查 max | serial / dateSerial |
| `NeedsUniqueCheck { candidate }` | UNIQUE 冲突换种子重试 | random |

### reset_key 进 prefix（方案 §4.8.1 路径 A）

serial/dateSerial 求值出的 `reset_key`（日期串、分类值、组织码……）会拼进反查 max 的
prefix，使 `WHERE code LIKE '{prefix}%'` **天然按 reset 维度分组**——「按日重置」「按组织
重置」「按分类重置」无需改 SQL 签名即可生效；无 resetBy 时的 `_global_` 占位符不进
prefix，保持全局连续号的 LIKE 模式干净。

> 设计文档：`.trae/documents/20260804_cmx-code_通用业务编码引擎设计方案.html`。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-core` | 核心库（`DataValue` 等，SQL 参数强类型绑定的下游透传） |
| `serde` / `serde_json` | 规则/段定义 JSON 反序列化 + 弱类型 Value（attrs / params） |
| `thiserror` | `CodeError` 错误枚举派生 |
| `chrono` | 日期段/日期流水段时间源（`ResolveContext.now`） |
| `regex` | manual 模式 pattern 校验 |
| `async-trait` | `Advance` / `SegmentResolver` trait |
| `rand` | 随机段 charset 字符池 / range 区间抽取 |
| `tokio`（dev-dependency） | 仅测试 async fn（`evaluate_segments`） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-code-api` | `cmx-code-model = { workspace = true }`（**唯一直接依赖者**） | `engine.rs` 组合 `rule_algo::evaluate_segments` 铸号；`store/serial_pg.rs` 的 `PgAdvance` 实现 `Advance`；handler 用 `RuleSpec` 收发规则 CRUD |
| `cmx-platform-app` / `cmx-portalservice`（跨 workspace） | 经 `cmx-code-api` 传递依赖 | 门户进程承载 `/api/code/*` 端点与 `CodeEngine` 注入链 |
| DCT / DOC / MDM 钩子（`cmx-dct-store-pg` 等） | **当前不依赖**（经 `cmx-traits::GlobalCodeMinter` 间接消费铸号能力） | lib.rs 文档"可被钩子轻量依赖"是保留的设计能力——本 crate 依赖极轻（无 DB 无 Web），未来钩子直连算法可直接引入而不污染依赖树 |

```text
cmx-code-api（PgAdvance 实现 Advance + engine 组装）
        │ 依赖
        ▼
cmx-code-model（本 crate，纯逻辑）：spec 类型 + rule_algo 求值 + segments 七段
        ▲ Advance trait 依赖注入（测试桩 StubAdvance：零 DB 单测）
```

无反向依赖：本 crate 不依赖 `cmx-code-api`（分层无环）。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 规则算法模型 | `RuleSpec`：段序列 + joiner + pattern + enable_gap + use_sequence + priority（选优取大）+ DAM 三级归属；serde 默认值齐全（mode=auto / priority=100 / is_active=true） |
| 挂载点声明模型 | `CodeRule`（camelCase JSON）：ruleCode / field（默认 `code`）/ mode / enableGap / pattern / uniqueCheck / cascade；`local_overrides()` + `merge_with()` 两层合并 |
| 七种内置段 | const / serial / date / dateSerial / ref / random / custom:check_digit，`SegmentRegistry::new()` 经 `register_builtin` 全部装载；段参数宽松存储：`SegmentSpec` 的 params 以 JSON Map flatten，各 resolver 自行取值 |
| 完整铸号 `evaluate_segments` | 固定段拼 prefix → 断号优先（enable_gap 时 `take_gap`）→ 反查 max（含 minted_buffer union）→ `next_after` 推号 → `try_insert` 冲突重试（上限 8 次）；serial 与 random 互斥直接报错 |
| 随机段铸号 | 不反查 max（无 max 概念），每次重试重新 resolve 换种子（上限 16 次，耗尽报 `RandomSpaceExhausted`）；charset 池 digit/alpha/alnum/hex/自定义串，`excludeAmbiguous` 默认 true 去除 0/O/1/I/l/Z/2/B/8（digit/hex 不过滤） |
| 流水推进公式 | `next_after(max, start, step)` = `start + ((max - start)/step + 1) * step`，max < start 时返回 start——保证 start/step 下号连续不跳 |
| 断号反解析 | `parse_code_serial(code, rule, attrs)` → `Some((prefix, serial_val, width))`，与铸号对称，供删行记断号；无流水段 / prefix 不匹配 / 尾部非数字返回 None |
| 补位/截断/校验位 | `pad`（左右补位，超宽不截断）/ `truncate`（none 报错 / right 右截断）/ `format_serial`；`custom:check_digit` 对前序段拼接算 mod11（余 10 记 'X'）或 luhn 校验位 |
| 求值上下文 | `ResolveContext`：attrs / org / now / resolved_so_far / minted_buffer / overrides / db_id+txn_id 透传，builder 链式构造；`effective_enable_gap` / `effective_pattern` 实现挂载点覆盖优先 |

---

## 模块结构

```text
cmx-code-model
├── src
│   ├── lib.rs              # 模块声明 + 顶层导出（RuleSpec/CodeRule/Target/ResolveContext/Advance/StubAdvance/…）
│   ├── spec.rs             # 核心类型：RuleSpec（规则算法）/ CodeRule+Overrides+Cascade+Scope（挂载点）
│   │                       #   / SegmentSpec+PadSide / SegmentValue（三态）/ Target / validate_mode
│   ├── rule_algo.rs        # 求值主逻辑：resolve_fixed_segments / evaluate_segments / build_prefix_and_specs
│   │                       #   / next_after / parse_code_serial（含 P0 回归：单条与批量 prefix 一致）
│   ├── context.rs          # ResolveContext + OrgContext + builder（.with/.org/.with_minted/.with_overrides/.txn）
│   ├── advance.rs          # Advance trait（query_max_serial/take_gap/try_insert）+ StubAdvance 测试桩
│   ├── registry.rs         # SegmentRegistry：new() 注册内置段 / register 扩展 / resolve（custom:* 前缀分发）
│   ├── pad.rs              # pad / truncate / format_serial（补位与超长截断）
│   ├── error.rs            # CodeError（10 变体，中文 message）+ Result 别名 + From<serde_json::Error>
│   └── segments/           # 七种段 resolver（各实现 SegmentResolver trait）
│       ├── mod.rs          #   trait 定义 + register_builtin（注册全部 7 段）
│       ├── const_.rs       #   固定段：value → Literal
│       ├── serial.rs       #   流水段：width 必填；evaluate_reset_by（date/org_code/_global_/字段/+复合）
│       ├── date.rs         #   日期段：format_date（YYYY/YY/YYYYMM/YYMM/YYYYMMDD/YYMMDD/MM/DD）
│       ├── date_serial.rs  #   日期流水段：date+serial 语法糖，reset_key=日期串（按日重置）
│       ├── ref.rs          #   引用段：field/refField（树形引用）/map/take/pad/fallback/truncate
│       ├── random.rs       #   随机段：charset（字符池）/range（区间+补位）；excludeAmbiguous 策略
│       └── custom.rs       #   自定义段分发：内置 custom:check_digit（mod11/luhn）
└── Cargo.toml              # 无 [features]
```

---

## 关键类型 / API

```rust
// ── spec.rs ── 规则算法（存 cmx_code_rule 表，纯算法，无 target）─────────────
pub struct RuleSpec {
    pub rule_code: String,           // 规则码（全局唯一，如 supplier_hq）
    pub rule_name: String,
    pub mode: String,                // "auto"（引擎生成）| "manual"（手敲，引擎只校验）
    pub org_scope: Option<String>,   // 受控组织（可选）
    pub condition: Option<String>,   // 适用条件表达式（按属性分流）
    pub segments: Vec<SegmentSpec>,  // 段序列（auto 必填）
    pub joiner: String,              // 段间连接符（默认空串）
    pub pattern: Option<String>,     // 校验正则
    pub enable_gap: bool,            // 断号补偿（连号域才开）
    pub use_sequence: bool,          // PG SEQUENCE 兜底（极端高并发可选）
    pub priority: i32,               // 多规则选优取大（默认 100）
    pub is_active: bool,             // 默认 true
    pub domain_code: String,         // DAM 三级归属（空串 = 兼容存量/全局可见）
    pub application_code: String, pub module_code: String,
}
impl RuleSpec {
    // 流水段参数读取（取首个 serial/dateSerial 段；缺省 0 / 1 / 1 / '0' / Left）
    pub fn serial_width(&self) -> usize;
    pub fn serial_start(&self) -> i64;          pub fn serial_step(&self) -> i64;
    pub fn serial_pad_char(&self) -> char;      pub fn serial_pad_side(&self) -> PadSide;
    pub fn use_sequence(&self) -> bool;
}

// ── spec.rs ── 挂载点声明（存 DCT/DOC 定义，serde camelCase）─────────────────
pub struct CodeRule {
    pub rule_code: Option<String>,   // JSON key = ruleCode
    pub field: String,               // 写回列，默认 "code"
    pub mode: String,                // 局部覆盖规则表 mode
    pub enable_gap: Option<bool>,    // 局部覆盖 enable_gap
    pub pattern: Option<String>,     // manual 模式兜底正则
    pub unique_check: Option<bool>,  // 是否强制 UNIQUE 校验
    pub cascade: Option<Cascade>,    // 级联回填（仅 DOC 父表）
}
impl CodeRule {
    pub fn local_overrides(&self) -> Overrides;            // 取挂载点局部覆盖
    pub fn merge_with(&self, rule: RuleSpec) -> RuleSpec;  // 合并两层（覆盖优先）
}
pub struct Overrides { pub enable_gap: Option<bool>, pub pattern: Option<String> }
pub struct Cascade { pub field: String, pub scope: Scope }  // Scope: Children | Descendants

// ── spec.rs ── 段声明 + 求值结果三态 ─────────────────────────────────────────
pub struct SegmentSpec {
    pub r#type: String,              // "const"/"serial"/…/"custom:xxx"（serde rename = "type"）
    pub params: serde_json::Map<String, serde_json::Value>,  // 其余字段 flatten
}
impl SegmentSpec {
    pub fn seg_type(&self) -> &str;                  // 原始 type
    pub fn get_str(&self, key: &str) -> Option<&str>;
    pub fn get_u64(&self, key: &str) -> Option<u64>; pub fn get_i64(&self, key: &str) -> Option<i64>;
    pub fn width(&self) -> Option<usize>;            // width 参数
    pub fn start(&self) -> Option<i64>; pub fn step(&self) -> Option<i64>;   // 默认 1 / 1
    pub fn reset_by(&self) -> Option<&str>;          // "resetBy"
    pub fn pad_char(&self) -> char;                  // "padChar"，默认 '0'
    pub fn pad_side(&self) -> PadSide;               // "padSide"，默认 Left
    pub fn truncate_mode(&self) -> &str;             // "truncate"，默认 "none"
    pub fn truncate_width(&self) -> Option<usize>;   // width 或 take 字段
}
pub enum PadSide { Left, Right }
pub enum SegmentValue {
    Literal(String),
    NeedsSerial { reset_key: String, width: usize, step: i64, start: i64,
                  pad_char: char, pad_side: PadSide },
    NeedsUniqueCheck { candidate: String },
}

// ── spec.rs ── 铸号目标（钩子层传入；换目标只改 field，引擎零改）──────────────
pub struct Target { pub kind: String /*"dct"|"doc"*/, pub code: String /*表名*/,
                    pub field: String /*写回列*/ }
impl Target { pub fn dct(code: &str, field: &str) -> Self;
              pub fn doc(code: &str, field: &str) -> Self; }
pub fn validate_mode(mode: &str) -> Result<()>;      // 必须是 auto/manual

// ── context.rs ── 求值上下文 ─────────────────────────────────────────────────
pub struct OrgContext { pub org_code: String }
pub struct ResolveContext {
    pub attrs: serde_json::Value,          // 行属性（ref/resetBy 取字段值）
    pub org: OrgContext,
    pub now: chrono::DateTime<chrono::Utc>, // date/dateSerial 时间源
    pub resolved_so_far: Vec<String>,      // 已解析段值（custom 校验位段用）
    pub minted_buffer: Arc<Vec<String>>,   // 同事务已铸号（union 进反查 max）
    pub overrides: Arc<Overrides>,         // 挂载点局部覆盖
    pub db_id: String,                     // 透传给 Advance 实现层
    pub txn_id: Option<String>,            // None = 非事务
}
impl ResolveContext {
    pub fn for_test() -> Self;                             // 测试用最小上下文
    pub fn new(db_id: &str, txn_id: Option<&str>) -> Self; // 生产用（钩子层构造）
    pub fn with(self, attrs: serde_json::Value) -> Self;   // 行属性
    pub fn org(self, org: OrgContext) -> Self;
    pub fn with_minted(self, buffer: &[String]) -> Self;   // 已铸号 buffer
    pub fn with_overrides(self, overrides: Overrides) -> Self;
    pub fn txn(&self) -> Option<&str>;  pub fn minted_buffer(&self) -> &[String];
    pub fn attr_str(&self, key: &str) -> Option<&str>;
    pub fn effective_enable_gap(&self, rule_default: bool) -> bool;  // 覆盖优先
    pub fn effective_pattern(&self, rule_default: &Option<String>) -> Option<String>;
}

// ── advance.rs ── DB 操作抽象（实现方 cmx-code-api::store::serial_pg::PgAdvance）
#[async_trait::async_trait]
pub trait Advance: Send + Sync {
    async fn query_max_serial(&self, target: &Target, prefix: &str, width: usize,
                              minted_buffer: &[String]) -> Result<i64>;  // 0 = 无历史号
    async fn take_gap(&self, prefix: &str, width: usize) -> Result<Option<i64>>;
    async fn try_insert(&self, target: &Target, code: &str) -> Result<()>;
}
pub struct StubAdvance;   // query_max=0 / take_gap=None / try_insert 恒 Ok（纯逻辑单测）

// ── registry.rs / segments ───────────────────────────────────────────────────
pub struct SegmentRegistry;
impl SegmentRegistry {
    pub fn new() -> Self;                                    // 注册全部内置段
    pub fn register(&mut self, resolver: Box<dyn SegmentResolver>);
    pub fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue>;
}
#[async_trait::async_trait(?Send)]
pub trait SegmentResolver: Send + Sync {
    fn seg_type(&self) -> &str;                              // 如 "const" / "custom:year2"
    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue>;
}

// ── rule_algo.rs ── 求值主逻辑（另有 pub build_prefix_and_specs 共用底座）────
pub fn resolve_fixed_segments(rule: &RuleSpec, ctx: &ResolveContext) -> Result<String>;
pub async fn evaluate_segments(rule: &RuleSpec, target: &Target,
                               ctx: &ResolveContext, advance: &dyn Advance) -> Result<String>;
pub fn next_after(max: i64, start: i64, step: i64) -> i64;
pub fn parse_code_serial(code: &str, rule: &RuleSpec, attrs: &serde_json::Value)
    -> Option<(String, i64, usize)>;

// ── error.rs / pad.rs ────────────────────────────────────────────────────────
pub enum CodeError {
    InvalidSegment(String), UnknownSegmentType(String),
    SegmentEvalFailed { field: String, expected: String, actual: String },
    NoMatchingRule(String), MaxRetryExceeded(u32), RandomSpaceExhausted(u32),
    RefFieldMissing(String), PatternMismatch { code: String, pattern: String },
    Internal(String), Database(String),
}
pub type Result<T> = core::result::Result<T, CodeError>;
pub fn pad(s: &str, width: usize, pad_char: char, side: PadSide) -> String;
pub fn truncate(s: &str, width: usize, mode: &str) -> Result<String>;  // none 报错 / right 截断
pub fn format_serial(n: i64, width: usize, pad_char: char, side: PadSide) -> String;
```

---

## 使用示例

### 一、纯逻辑铸号（StubAdvance，零 DB 单测——摘自模块测试场景）

```rust
use cmx_code_model::{RuleSpec, Target, ResolveContext, StubAdvance, evaluate_segments};

// 从 JSON 构造规则：固定段 "V" + 4 位流水段（serde 默认值补齐其余字段）
let rule: RuleSpec = serde_json::from_str(r#"{
    "segments": [
        {"type": "const", "value": "V"},
        {"type": "serial", "width": 4}
    ]
}"#).unwrap();

let target = Target::dct("bus_partner", "code"); // 铸号写回 cf_bus_partner.code 列
let ctx = ResolveContext::for_test();
let advance = StubAdvance;                       // 反查 max 恒 0 → 首号从 start=1 起

// evaluate_segments 是 async fn，需在 tokio 测试内调用：
// let code = evaluate_segments(&rule, &target, &ctx, &advance).await.unwrap();
// assert_eq!(code, "V0001");   // max=0, start=1 → 补位 "0001"
```

### 二、reset_key 进 prefix（按日/按分类重置的分组原理）

```rust
use cmx_code_model::{RuleSpec, ResolveContext, resolve_fixed_segments};

// dateSerial：按日重置流水（单据号典型形态 FV20260804 + 0001）
let rule: RuleSpec = serde_json::from_str(r#"{
    "segments": [
        {"type": "const", "value": "FV"},
        {"type": "dateSerial", "format": "YYYYMMDD", "width": 4}
    ]
}"#).unwrap();
let ctx = ResolveContext::for_test();
let prefix = resolve_fixed_segments(&rule, &ctx).unwrap();
// dateSerial 的 reset_key = 日期串，拼进 prefix → 反查 max 天然按日分组：
let today = ctx.now.format("%Y%m%d").to_string();
assert_eq!(prefix, format!("FV{today}"));

// resetBy=字段名：分类值进 prefix（同表多套连续号）
let rule2: RuleSpec = serde_json::from_str(r#"{
    "segments": [
        {"type": "const", "value": "V"},
        {"type": "serial", "width": 4, "resetBy": "category"}
    ]
}"#).unwrap();
let ctx2 = ResolveContext::for_test().with(serde_json::json!({"category": "raw"}));
assert_eq!(resolve_fixed_segments(&rule2, &ctx2).unwrap(), "Vraw"); // "raw" 进 prefix

// 对照：无 resetBy 的全局流水 reset_key=_global_ 不进 prefix（保持全局连续）；
// joiner="-" 时段间连接符保留 → prefix = "V-"（单条/批量格式一致的回归基线）。
```

### 三、删行记断号：parse_code_serial 反解析（与铸号对称）

```rust
use cmx_code_model::{RuleSpec, parse_code_serial};

// 与铸号时同一条规则（含 resetBy），attrs 与被删行一致才能重建出相同 prefix
let rule: RuleSpec = serde_json::from_str(r#"{
    "segments": [
        {"type": "const", "value": "V"},
        {"type": "serial", "width": 4, "resetBy": "category"}
    ]
}"#).unwrap();
let attrs = serde_json::json!({"category": "raw"});

// 反解析：code = prefix("Vraw") + 4 位流水 → ("Vraw", 5, 4)，落 cmx_code_gap 供下次铸号填补
let parsed = parse_code_serial("Vraw0005", &rule, &attrs).unwrap();
assert_eq!(parsed, ("Vraw".to_string(), 5, 4));

// 无流水段（纯固定码/纯随机码）→ None：删了不产生可填补的断号
let fixed: RuleSpec = serde_json::from_str(
    r#"{"segments": [{"type": "const", "value": "PREFIX"}]}"#).unwrap();
assert!(parse_code_serial("PREFIX", &fixed, &serde_json::json!({})).is_none());
```

---

## FAQ

**Q1：`Advance::try_insert` 为什么恒返回 `Ok(())`？**
设计决策——DCT/DOC saver 的铸号发生在 apply_merge 之前（钩子算号写回 changeset），真正的
INSERT 由 saver 完成，业务表 UNIQUE 约束在那里兜底；因此 `evaluate_segments` 的重试循环在
铸号阶段恒不触发，UNIQUE 冲突重试责任上移到 saver 层（落库冲突 → 重新调 mint 取下一号）。
铸号函数只算号不落库；未来若需铸号期预检，在实现里返回 `Err` 即可自动触发重试。

**Q2：serial 和 random 能混用吗？**
不能。`evaluate_segments` 遇到同时含流水段与随机段的规则直接报 `InvalidSegment`——两者推进
机制不同（反查 max vs 换种子重试），设计文档 §12 的示例里两者从不混用。

**Q3：文档注释与代码的小出入？**
`registry.rs` / `lib.rs` 的注释写着"随机段（random）在 C5 阶段补"，但
`segments/mod.rs::register_builtin` 实际已注册 `RandomResolver`——注释滞后，当前**七种段全部
内置生效**，以代码为准。

**Q4：`SegmentRegistry::register` 注册的自定义段能进主铸号链路吗？**
暂不能。`build_prefix_and_specs`（`evaluate_segments` 的共用底座）内部固定
`SegmentRegistry::new()`，不接受外部注入注册表，自定义 resolver 目前只能单段手动求值；
要让自定义段进主链路需改造为注册表注入。
