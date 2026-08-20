#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
生成 fico-db 报表「数据元素」种子数据 (DML)：
  cr_element_category  元素大类（资产负债/损益/现金流量/生产/销售 等）
  cr_data_element      各大类下的数据元素（会计科目 / 经营指标）

数据元素 = 报表单元格取数的最小语义单位（对标科目 / 指标）。设计器左侧「数据元素」面板即读此二表。
幂等：按本脚本生成的 code 精确 DELETE 再 INSERT，可重复执行，不误删他人数据。
用法：python3 gen_fico_cr_element_seed.py > fico_cr_element_seed.sql
"""

# ── 元素大类：code / 名称 / 编码前缀 ───────────────────────────────────────────
CATEGORIES = [
    ("balance",    "资产负债类", "BS"),
    ("income",     "损益类",     "PL"),
    ("cashflow",   "现金流量类", "CF"),
    ("production", "生产类",     "PD"),
    ("sales",      "销售类",     "SL"),
    ("cost",       "成本费用类", "CO"),
    ("tax",        "税务类",     "TX"),
]

# ── 每大类的数据元素：(名称, data_type, unit, decimals, value_source) ──────────
#    data_type：amount金额 / qty数量 / rate比率 / text文本 / date日期
#    value_source：gl总账 / manual手工 / calc计算
ELEMENTS = {
    "balance": [
        ("货币资金", "amount", "元", 2, "gl"), ("交易性金融资产", "amount", "元", 2, "gl"),
        ("应收票据", "amount", "元", 2, "gl"), ("应收账款", "amount", "元", 2, "gl"),
        ("预付款项", "amount", "元", 2, "gl"), ("其他应收款", "amount", "元", 2, "gl"),
        ("存货", "amount", "元", 2, "gl"), ("固定资产", "amount", "元", 2, "gl"),
        ("在建工程", "amount", "元", 2, "gl"), ("无形资产", "amount", "元", 2, "gl"),
        ("资产总计", "amount", "元", 2, "calc"), ("短期借款", "amount", "元", 2, "gl"),
        ("应付票据", "amount", "元", 2, "gl"), ("应付账款", "amount", "元", 2, "gl"),
        ("预收款项", "amount", "元", 2, "gl"), ("应付职工薪酬", "amount", "元", 2, "gl"),
        ("应交税费", "amount", "元", 2, "gl"), ("长期借款", "amount", "元", 2, "gl"),
        ("负债合计", "amount", "元", 2, "calc"), ("实收资本", "amount", "元", 2, "gl"),
        ("资本公积", "amount", "元", 2, "gl"), ("盈余公积", "amount", "元", 2, "gl"),
        ("未分配利润", "amount", "元", 2, "gl"), ("所有者权益合计", "amount", "元", 2, "calc"),
    ],
    "income": [
        ("营业收入", "amount", "元", 2, "gl"), ("营业成本", "amount", "元", 2, "gl"),
        ("税金及附加", "amount", "元", 2, "gl"), ("销售费用", "amount", "元", 2, "gl"),
        ("管理费用", "amount", "元", 2, "gl"), ("研发费用", "amount", "元", 2, "gl"),
        ("财务费用", "amount", "元", 2, "gl"), ("投资收益", "amount", "元", 2, "gl"),
        ("营业利润", "amount", "元", 2, "calc"), ("营业外收入", "amount", "元", 2, "gl"),
        ("营业外支出", "amount", "元", 2, "gl"), ("利润总额", "amount", "元", 2, "calc"),
        ("所得税费用", "amount", "元", 2, "gl"), ("净利润", "amount", "元", 2, "calc"),
        ("毛利率", "rate", "%", 2, "calc"), ("净利率", "rate", "%", 2, "calc"),
        ("基本每股收益", "amount", "元/股", 4, "calc"),
    ],
    "cashflow": [
        ("销售商品提供劳务收到的现金", "amount", "元", 2, "gl"),
        ("收到的税费返还", "amount", "元", 2, "gl"),
        ("购买商品接受劳务支付的现金", "amount", "元", 2, "gl"),
        ("支付给职工的现金", "amount", "元", 2, "gl"),
        ("支付的各项税费", "amount", "元", 2, "gl"),
        ("经营活动现金流量净额", "amount", "元", 2, "calc"),
        ("收回投资收到的现金", "amount", "元", 2, "gl"),
        ("购建固定资产支付的现金", "amount", "元", 2, "gl"),
        ("投资活动现金流量净额", "amount", "元", 2, "calc"),
        ("取得借款收到的现金", "amount", "元", 2, "gl"),
        ("偿还债务支付的现金", "amount", "元", 2, "gl"),
        ("分配股利支付的现金", "amount", "元", 2, "gl"),
        ("筹资活动现金流量净额", "amount", "元", 2, "calc"),
        ("现金及现金等价物净增加额", "amount", "元", 2, "calc"),
        ("期末现金及现金等价物余额", "amount", "元", 2, "calc"),
    ],
    "production": [
        ("产量", "qty", "件", 0, "manual"), ("产值", "amount", "元", 2, "calc"),
        ("投料量", "qty", "吨", 2, "manual"), ("单位产品成本", "amount", "元", 2, "calc"),
        ("直接材料", "amount", "元", 2, "gl"), ("直接人工", "amount", "元", 2, "gl"),
        ("制造费用", "amount", "元", 2, "gl"), ("在产品数量", "qty", "件", 0, "manual"),
        ("产成品数量", "qty", "件", 0, "manual"), ("设备开工率", "rate", "%", 2, "manual"),
        ("产能利用率", "rate", "%", 2, "calc"), ("良品率", "rate", "%", 2, "manual"),
        ("单位工时", "qty", "工时", 2, "manual"), ("能耗成本", "amount", "元", 2, "gl"),
    ],
    "sales": [
        ("销售数量", "qty", "件", 0, "manual"), ("销售金额", "amount", "元", 2, "gl"),
        ("销售单价", "amount", "元", 2, "calc"), ("销售退回", "amount", "元", 2, "gl"),
        ("销售折扣", "amount", "元", 2, "gl"), ("销售净额", "amount", "元", 2, "calc"),
        ("回款金额", "amount", "元", 2, "gl"), ("回款率", "rate", "%", 2, "calc"),
        ("客户数量", "qty", "户", 0, "manual"), ("新增客户数", "qty", "户", 0, "manual"),
        ("市场占有率", "rate", "%", 2, "manual"), ("订单数量", "qty", "单", 0, "manual"),
        ("平均客单价", "amount", "元", 2, "calc"), ("销售毛利", "amount", "元", 2, "calc"),
    ],
    "cost": [
        ("材料成本", "amount", "元", 2, "gl"), ("人工成本", "amount", "元", 2, "gl"),
        ("制造费用", "amount", "元", 2, "gl"), ("期间费用", "amount", "元", 2, "gl"),
        ("水电费", "amount", "元", 2, "gl"), ("折旧费", "amount", "元", 2, "gl"),
        ("摊销费", "amount", "元", 2, "gl"), ("差旅费", "amount", "元", 2, "gl"),
        ("业务招待费", "amount", "元", 2, "gl"), ("单位成本", "amount", "元", 2, "calc"),
        ("成本费用总额", "amount", "元", 2, "calc"),
    ],
    "tax": [
        ("应交增值税", "amount", "元", 2, "gl"), ("销项税额", "amount", "元", 2, "gl"),
        ("进项税额", "amount", "元", 2, "gl"), ("应交企业所得税", "amount", "元", 2, "gl"),
        ("应交城建税", "amount", "元", 2, "gl"), ("教育费附加", "amount", "元", 2, "gl"),
        ("应交印花税", "amount", "元", 2, "gl"), ("税负率", "rate", "%", 2, "calc"),
        ("实际税率", "rate", "%", 2, "calc"), ("已缴税额", "amount", "元", 2, "gl"),
    ],
}


def q(s: str) -> str:
    return s.replace("'", "''")


def main():
    print("-- =============================================")
    print("-- fico-db 报表「数据元素」种子数据 (DML) —— 由 gen_fico_cr_element_seed.py 生成，勿手改")
    print(f"--   cr_element_category {len(CATEGORIES)} 大类 · cr_data_element 若干元素")
    print("-- 数据元素 = 报表单元格取数的最小语义单位（对标科目/指标）。设计器左侧「数据元素」面板读此二表。")
    print("-- 幂等：按本脚本 code 精确清理再插入，可重复执行。")
    print("-- 依赖：cr_element_category / cr_data_element 已由报表数据字典元数据部署到 fico-db。")
    print("-- =============================================")

    # 元素大类
    print("\n-- 元素大类 cr_element_category")
    cat_codes = ",".join(f"'{c}'" for c, *_ in CATEGORIES)
    print(f"DELETE FROM cr_element_category WHERE code IN ({cat_codes});")
    for i, (code, name, _abbr) in enumerate(CATEGORIES, start=1):
        print(
            f"INSERT INTO cr_element_category (code,name,sort_no,status,remark,create_time) "
            f"VALUES ('{code}','{q(name)}',{i},1,NULL,CURRENT_TIMESTAMP);"
        )

    # 数据元素
    print("\n-- 数据元素 cr_data_element（按大类分组，code = {前缀}_{NN}）")
    all_codes = []
    inserts = []
    sort_no = 0
    for cat_code, cat_name, abbr in CATEGORIES:
        items = ELEMENTS.get(cat_code, [])
        for idx, (name, dtype, unit, decimals, vsrc) in enumerate(items, start=1):
            sort_no += 1
            code = f"{abbr}_{idx:02d}"
            all_codes.append(code)
            # 汇总/比率类给 formula_type 提示；纯取数给 none
            ftype = "calc" if vsrc == "calc" else "none"
            remark = f"{cat_name} · {name}"
            inserts.append(
                "INSERT INTO cr_data_element "
                "(code,name,category_code,data_type,unit,decimals,value_source,"
                "formula_type,calc_formula,check_formula,sort_no,status,remark,create_time) VALUES "
                f"('{code}','{q(name)}','{cat_code}','{dtype}','{q(unit)}',{decimals},'{vsrc}',"
                f"'{ftype}',NULL,NULL,{sort_no},1,'{q(remark)}',CURRENT_TIMESTAMP);"
            )
    # 幂等清理：本脚本前缀 + 两位序号
    prefixes = "|".join(abbr for _c, _n, abbr in CATEGORIES)
    print(f"DELETE FROM cr_data_element WHERE code ~ '^({prefixes})_[0-9]{{2}}$';")
    for line in inserts:
        print(line)

    assert len(all_codes) == len(set(all_codes)), "duplicate element codes!"
    total = len(all_codes)
    # 尾注（注释形式，便于核对）
    print(f"\n-- 合计：{len(CATEGORIES)} 大类 · {total} 个数据元素")


if __name__ == "__main__":
    main()
