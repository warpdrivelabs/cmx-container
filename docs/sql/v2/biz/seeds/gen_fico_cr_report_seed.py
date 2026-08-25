#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
生成 fico-db 报表系统种子数据 (DML)：
  cr_report_category  五大报表类别（监管/内部管理/税务/合并/法定）
  cr_period_type      六种编报期间类型（日/周/月/季/半年/年报）
  cr_entity_scope     编制口径（单体/合并/汇总）
  cr_amount_unit      金额单位（元/千元/万元）
  cr_report_list      报表主档：每「类别 × 期间」10 张 = 300 张

幂等：按本脚本生成的 code 精确 DELETE 再 INSERT，可重复执行，不误删他人数据。
用法：python3 gen_fico_cr_report_seed.py > fico_cr_report_seed.sql
"""

# ── 五大类别：code / 名称 / 报表编码前缀 / 是否法定 / 默认编制口径 ──────────────
CATEGORIES = [
    ("supervisory",   "监管类",   "SUP", 1, "single"),
    ("internal",      "内部管理类", "INT", 0, "single"),
    ("tax",           "税务类",   "TAX", 0, "single"),
    ("consolidation", "合并类",   "CON", 0, "consol"),
    ("statutory",     "法定类",   "STA", 1, "single"),
]

# ── 六种期间类型：code / 名称 / 报表编码中段 ───────────────────────────────────
PERIODS = [
    ("day",      "日报",   "DAY"),
    ("week",     "周报",   "WK"),
    ("month",    "月报",   "MON"),
    ("quarter",  "季报",   "QTR"),
    ("halfyear", "半年报", "HY"),
    ("year",     "年报",   "YR"),
]

# ── 每类别 10 个业务主题（题名, 报表类型）。报表类型：BS/PL/CF/EQ/CUSTOM ────────
THEMES = {
    "supervisory": [
        ("流动性监管报表", "CUSTOM"), ("资本充足率报表", "CUSTOM"), ("大额风险暴露报表", "CUSTOM"),
        ("杠杆率监管报表", "CUSTOM"), ("偿付能力报表", "CUSTOM"), ("资产质量监管表", "CUSTOM"),
        ("关联交易监管表", "CUSTOM"), ("风险集中度监管表", "CUSTOM"), ("净稳定资金比例表", "CUSTOM"),
        ("监管指标汇总表", "CUSTOM"),
    ],
    "internal": [
        ("部门费用分析表", "CUSTOM"), ("成本中心报表", "CUSTOM"), ("利润中心报表", "CUSTOM"),
        ("预算执行分析表", "CUSTOM"), ("资金调度表", "CUSTOM"), ("应收账龄分析表", "CUSTOM"),
        ("存货周转分析表", "CUSTOM"), ("现金流管理表", "CF"), ("经营指标看板", "CUSTOM"),
        ("管理驾驶舱报表", "CUSTOM"),
    ],
    "tax": [
        ("增值税申报表", "CUSTOM"), ("企业所得税申报表", "CUSTOM"), ("附加税费计算表", "CUSTOM"),
        ("印花税汇总表", "CUSTOM"), ("个税代扣代缴表", "CUSTOM"), ("税负分析表", "CUSTOM"),
        ("递延所得税表", "CUSTOM"), ("进项税额明细表", "CUSTOM"), ("销项税额明细表", "CUSTOM"),
        ("纳税调整明细表", "CUSTOM"),
    ],
    "consolidation": [
        ("合并资产负债表", "BS"), ("合并利润表", "PL"), ("合并现金流量表", "CF"),
        ("合并所有者权益变动表", "EQ"), ("内部往来抵销表", "CUSTOM"), ("长期股权投资抵销表", "CUSTOM"),
        ("少数股东权益表", "CUSTOM"), ("合并范围明细表", "CUSTOM"), ("商誉减值测试表", "CUSTOM"),
        ("合并工作底稿", "CUSTOM"),
    ],
    "statutory": [
        ("资产负债表", "BS"), ("利润表", "PL"), ("现金流量表", "CF"),
        ("所有者权益变动表", "EQ"), ("财务报表附注", "CUSTOM"), ("分部报告表", "CUSTOM"),
        ("应交税费明细表", "CUSTOM"), ("主营业务收支表", "CUSTOM"), ("资产减值明细表", "CUSTOM"),
        ("财务情况说明书", "CUSTOM"),
    ],
}

ENTITY_SCOPES = [
    ("single",   "单体", 1),
    ("consol",   "合并", 2),
    ("combined", "汇总", 3),
]
AMOUNT_UNITS = [
    ("yuan",  "元",  1),
    ("kyuan", "千元", 2),
    ("wyuan", "万元", 3),
]


def q(s: str) -> str:
    """单引号转义。"""
    return s.replace("'", "''")


def emit_classify(table: str, rows, comment: str):
    print(f"\n-- {comment}")
    codes = ",".join(f"'{c}'" for c, *_ in rows)
    print(f"DELETE FROM {table} WHERE code IN ({codes});")
    for code, name, sort_no in rows:
        print(
            f"INSERT INTO {table} (code,name,sort_no,status,remark,create_time) "
            f"VALUES ('{code}','{q(name)}',{sort_no},1,NULL,CURRENT_TIMESTAMP);"
        )


def main():
    print("-- =============================================")
    print("-- fico-db 报表系统种子数据 (DML) —— 由 gen_fico_cr_report_seed.py 生成，勿手改")
    print("--   cr_report_category 5 · cr_period_type 6 · cr_entity_scope 3 · cr_amount_unit 3")
    print("--   cr_report_list 300（5 类别 × 6 期间 × 10）")
    print("-- 幂等：按本脚本 code 精确清理再插入，可重复执行。")
    print("-- 依赖：21 张 cr_* 表已由报表数据字典元数据部署到 fico-db。")
    print("-- =============================================")

    # 类别
    emit_classify(
        "cr_report_category",
        [(c, n, i + 1) for i, (c, n, *_r) in enumerate(CATEGORIES)],
        "报表类别（五大类）",
    )
    # 期间类型
    emit_classify(
        "cr_period_type",
        [(c, n, i + 1) for i, (c, n, _a) in enumerate(PERIODS)],
        "编报期间类型（日/周/月/季/半年/年）",
    )
    # 编制口径
    emit_classify("cr_entity_scope", ENTITY_SCOPES, "编制口径")
    # 金额单位
    emit_classify("cr_amount_unit", AMOUNT_UNITS, "金额单位")

    # 报表主档 300 张
    print("\n-- 报表主档 cr_report_list：每「类别 × 期间」10 张 = 300 张")
    all_codes = []
    inserts = []
    sort_no = 0
    for cat_code, cat_name, abbr, is_stat, scope in CATEGORIES:
        themes = THEMES[cat_code]
        for per_code, per_name, per_abbr in PERIODS:
            for idx, (theme, rtype) in enumerate(themes, start=1):
                sort_no += 1
                code = f"{abbr}_{per_abbr}_{idx:02d}"
                all_codes.append(code)
                name = f"{theme}（{per_name}）"
                remark = f"{cat_name}/{per_name} 示例报表"
                inserts.append(
                    "INSERT INTO cr_report_list "
                    "(code,name,report_type,report_category,format_code,period_type,"
                    "currency_code,amount_unit,entity_scope,template_version,data_source,"
                    "is_statutory,remark,sort_no,status,create_time) VALUES "
                    f"('{code}','{q(name)}','{rtype}','{cat_code}',NULL,'{per_code}',"
                    f"'CNY','yuan','{scope}','V1','gl_balance',"
                    f"{is_stat},'{q(remark)}',{sort_no},1,CURRENT_TIMESTAMP);"
                )
    # 精确幂等清理（只删本脚本的 code 形态）
    print("DELETE FROM cr_report_list WHERE code ~ "
          "'^(SUP|INT|TAX|CON|STA)_(DAY|WK|MON|QTR|HY|YR)_[0-9]{2}$';")
    for line in inserts:
        print(line)
    # 断言
    assert len(all_codes) == 300, f"expected 300 reports, got {len(all_codes)}"
    assert len(set(all_codes)) == 300, "duplicate report codes!"


if __name__ == "__main__":
    main()
