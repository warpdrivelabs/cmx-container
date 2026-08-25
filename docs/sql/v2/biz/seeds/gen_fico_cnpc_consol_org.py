#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
生成 fico-db 报表「合并组织架构字典」种子数据 (DML) —— 中国石油 (CNPC) 合并方案。

cr_consol_org 是 parent_id 自分级树（前端 buildOrgTree 按 parent_id→id 组树、按 sort_no 排序）。
本脚本按中国石油天然气集团有限公司真实组织脉络构造一棵合并树，覆盖：
  集团(母公司) → 上市平台(中国石油股份) → 业务板块(勘探生产/炼化/销售) → 油田/石化/销售分支
  + 海外(中油国际,USD 需折算) + 合营/联营(权益法) + 工程技术服务 + 金融板块。
演示合并要素：full 完全合并 / equity 权益法、少数股东权益(<100% 持股)、外币折算(USD)、内部抵消。

id 用 1001+ 段（避开 CSCEC 示例的 1~23，两方案可共存）；consol_scheme='CNPC'。
幂等：先 DELETE consol_scheme='CNPC' 再插入，可重复执行。
用法：python3 gen_fico_cnpc_consol_org.py > fico_cnpc_consol_org.sql
"""

SCHEME = "CNPC"
ID_BASE = 1001  # 起始 id，避开 CSCEC(1~23)

# 组织节点（**前序**排列，父必先于子）：
#   code, name, parent_code, org_type, consol_method, ownership_pct, currency, remark
NODES = [
    ("CNPC", "中国石油天然气集团有限公司", None, "group", "full", 100.0, "CNY", "集团总部/合并主体(母公司)"),

    # ── 上市平台：中国石油股份（A股601857/H股0857），CNPC 持股约 86.35% → 13.65% 少数股东权益 ──
    ("PETROCHINA", "中国石油天然气股份有限公司", "CNPC", "subgroup", "full", 86.35, "CNY", "上市平台(A股601857/H股00857)"),

    # 勘探与生产板块（油气和新能源）
    ("EP", "勘探与生产板块", "PETROCHINA", "subgroup", "full", 100.0, "CNY", "油气和新能源分部"),
    ("DAQING", "大庆油田有限责任公司", "EP", "entity", "full", 100.0, "CNY", "主力油田"),
    ("CHANGQING", "长庆油田分公司", "EP", "branch", "full", 100.0, "CNY", "鄂尔多斯盆地"),
    ("TARIM", "塔里木油田分公司", "EP", "branch", "full", 100.0, "CNY", "西部油气"),
    ("XINJIANG", "新疆油田分公司", "EP", "branch", "full", 100.0, "CNY", "准噶尔盆地"),
    ("SOUTHWEST", "西南油气田分公司", "EP", "branch", "full", 100.0, "CNY", "川渝天然气"),
    ("HUABEI", "华北油田分公司", "EP", "branch", "full", 100.0, "CNY", "冀中/二连"),

    # 炼油化工与新材料板块
    ("RC", "炼油化工与新材料板块", "PETROCHINA", "subgroup", "full", 100.0, "CNY", "炼化分部"),
    ("DALIAN", "大连石化分公司", "RC", "branch", "full", 100.0, "CNY", "千万吨级炼厂"),
    ("LANZHOU", "兰州石化分公司", "RC", "branch", "full", 100.0, "CNY", "西部炼化基地"),
    ("DUSHANZI", "独山子石化分公司", "RC", "branch", "full", 100.0, "CNY", "炼化一体化"),
    ("GUANGDONG", "广东石化有限责任公司", "RC", "entity", "full", 100.0, "CNY", "2000万吨炼化一体化"),

    # 销售板块
    ("MK", "销售板块", "PETROCHINA", "subgroup", "full", 100.0, "CNY", "成品油/非油销售"),
    ("MK_HD", "华东销售分公司", "MK", "branch", "full", 100.0, "CNY", None),
    ("MK_HB", "华北销售分公司", "MK", "branch", "full", 100.0, "CNY", None),
    ("MK_HN", "华南销售分公司", "MK", "branch", "full", 100.0, "CNY", None),
    ("MK_XN", "西南销售分公司", "MK", "branch", "full", 100.0, "CNY", None),
    ("JV_BP", "中油碧辟石油有限公司", "MK", "entity", "equity", 49.0, "CNY", "与 BP 合营(权益法核算)"),

    # 天然气终端（昆仑能源，港股00135，PetroChina 控股）
    ("KUNLUN", "昆仑能源有限公司", "PETROCHINA", "entity", "full", 54.38, "CNY", "港股00135;天然气终端(少数股东权益)"),

    # ── CNPC 直属（非上市平台内）──
    # 海外油气（中油国际），功能货币 USD → 合并需外币折算
    ("CNPCINTL", "中国石油国际勘探开发有限公司", "CNPC", "subgroup", "full", 100.0, "USD", "海外油气(中油国际);USD 折算"),
    ("CNPC_ME", "中油国际(中东)有限公司", "CNPCINTL", "entity", "full", 100.0, "USD", "中东区"),
    ("CNPC_CA", "中油国际(中亚)有限公司", "CNPCINTL", "entity", "full", 100.0, "USD", "中亚区"),
    ("CNPC_AM", "中油国际(美洲)有限公司", "CNPCINTL", "entity", "full", 100.0, "USD", "美洲区"),

    # 工程技术服务
    ("CPECC", "中国石油工程建设有限公司", "CNPC", "entity", "full", 100.0, "CNY", "中油工程(601789)"),
    ("DRILLING", "中国石油集团钻探工程有限公司", "CNPC", "entity", "full", 100.0, "CNY", "钻探工程服务"),
    ("CPLOG", "中国石油集团测井有限公司", "CNPC", "entity", "full", 100.0, "CNY", "测井技术服务"),

    # 金融板块
    ("CAPITAL", "中国石油集团资本股份有限公司", "CNPC", "entity", "full", 76.50, "CNY", "金融板块(少数股东权益)"),

    # 联营（国家管网），CNPC 持股约 29.9% → 权益法
    ("PIPECHINA", "国家石油天然气管网集团有限公司", "CNPC", "entity", "equity", 29.90, "CNY", "联营企业(国家管网)权益法核算"),
]


def q(s) -> str:
    return "" if s is None else str(s).replace("'", "''")


def main():
    # 建索引：code → 记录
    by_code = {}
    order = []
    for n in NODES:
        code = n[0]
        by_code[code] = {
            "code": code, "name": n[1], "parent_code": n[2], "org_type": n[3],
            "consol_method": n[4], "ownership": n[5], "currency": n[6], "remark": n[7],
        }
        order.append(code)

    # 分配 id（前序顺序）
    for i, code in enumerate(order):
        by_code[code]["id"] = ID_BASE + i

    # 计算 parent_id / full_path / level_no
    def full_path(code):
        parts = []
        c = code
        while c is not None:
            parts.append(c)
            c = by_code[c]["parent_code"]
        return ".".join(reversed(parts))

    def level_no(code):
        n = 1
        c = by_code[code]["parent_code"]
        while c is not None:
            n += 1
            c = by_code[c]["parent_code"]
        return n

    # 谁有子节点 → 非末级
    has_child = {c: False for c in order}
    for c in order:
        p = by_code[c]["parent_code"]
        if p is not None:
            has_child[p] = True

    print("-- =============================================")
    print("-- fico-db 合并组织架构字典 (cr_consol_org) —— 中国石油 (CNPC) 合并方案")
    print("--   由 gen_fico_cnpc_consol_org.py 生成，勿手改。parent_id 自分级树。")
    print(f"--   共 {len(order)} 个组织节点；id 段 {ID_BASE}+（避开 CSCEC 示例）；consol_scheme='{SCHEME}'。")
    print("--   覆盖：集团→上市平台→板块→油田/石化/销售分支 + 海外(USD)/合营/联营(权益法)/工程/金融。")
    print(f"-- 幂等：先 DELETE consol_scheme='{SCHEME}' 再插入，可重复执行。")
    print("-- =============================================")
    print(f"\nDELETE FROM cr_consol_org WHERE consol_scheme = '{SCHEME}';")

    for sort_no, code in enumerate(order, start=1):
        r = by_code[code]
        pid = "NULL" if r["parent_code"] is None else str(by_code[r["parent_code"]]["id"])
        fp = full_path(code)
        lvl = level_no(code)
        is_leaf = 0 if has_child[code] else 1
        is_parent = 1 if r["parent_code"] is None else 0
        own = r["ownership"]
        remark = r["remark"]
        remark_sql = "NULL" if remark is None else f"'{q(remark)}'"
        print(
            "INSERT INTO cr_consol_org "
            "(id,code,name,consol_scheme,org_type,parent_id,full_path,level_no,is_leaf,"
            "entity_code,consol_method,ownership_pct,voting_pct,consol_currency,is_parent,"
            "offset_flag,remark,sort_no,status,create_time) VALUES "
            f"({r['id']},'{code}','{q(r['name'])}','{SCHEME}','{r['org_type']}',{pid},"
            f"'{fp}',{lvl},{is_leaf},'{code}','{r['consol_method']}',{own},{own},"
            f"'{r['currency']}',{is_parent},1,{remark_sql},{sort_no},1,CURRENT_TIMESTAMP);"
        )

    # 完整性断言（注释形式核对）
    print(f"\n-- 节点合计 {len(order)}；根 1（CNPC）；末级(叶) {sum(1 for c in order if not has_child[c])} 个。")


if __name__ == "__main__":
    main()
