#!/usr/bin/env python3
"""
一次性脚本：把 docs/sql/seed/cmxfico_dct_seed_data.sql 转换为 35 个 cf_*.json
用法：python3 scripts/convert_seed_sql_to_json.py
输出：data/meta/definitions/fi/cmxfico/gl/seed/cf_<table>.json
"""
import re
import json
import os
from collections import defaultdict

SRC = "docs/sql/seed/cmxfico_dct_seed_data.sql"
DST_DIR = "data/meta/definitions/fi/cmxfico/gl/seed"

# 解析 INSERT INTO <table> (col1, col2, ...) VALUES (v1, v2, ...);
INSERT_RE = re.compile(
    r"INSERT\s+INTO\s+(\w+)\s*\(([^)]+)\)\s*VALUES\s*\((.+?)\)\s*;",
    re.IGNORECASE | re.DOTALL,
)

def split_values(s):
    """把 VALUES 元组按逗号切分，处理单引号转义与嵌套"""
    out = []
    cur = []
    in_str = False
    i = 0
    while i < len(s):
        c = s[i]
        if in_str:
            if c == "'":
                if i + 1 < len(s) and s[i+1] == "'":
                    cur.append("'"); i += 2; continue
                in_str = False
            cur.append(c)
        else:
            if c == "'":
                in_str = True
                cur.append(c)
            elif c == ",":
                out.append("".join(cur).strip()); cur = []
            else:
                cur.append(c)
        i += 1
    if cur:
        out.append("".join(cur).strip())
    return out

def parse_value(v):
    """把 SQL 字面量转为 JSON 值"""
    v = v.strip()
    if v.upper() == "NULL":
        return None
    if v.startswith("'") and v.endswith("'"):
        return v[1:-1].replace("''", "'")
    if v.upper() in ("TRUE", "FALSE"):
        return v.upper() == "TRUE"
    try:
        return int(v)
    except ValueError:
        pass
    try:
        return float(v)
    except ValueError:
        pass
    return v

def main():
    tables = defaultdict(list)
    with open(SRC, encoding="utf-8") as f:
        content = f.read()

    for m in INSERT_RE.finditer(content):
        table = m.group(1)
        cols = [c.strip() for c in m.group(2).split(",")]
        vals = [parse_value(v) for v in split_values(m.group(3))]
        assert len(cols) == len(vals), f"列数不匹配 {table}: {cols} vs {vals}"
        tables[table].append(dict(zip(cols, vals)))

    os.makedirs(DST_DIR, exist_ok=True)
    for table, rows in tables.items():
        path = os.path.join(DST_DIR, f"{table}.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(rows, f, ensure_ascii=False, indent=2)
        print(f"  {table}: {len(rows)} 行 → {path}")

    print(f"\n共转换 {len(tables)} 张表")

if __name__ == "__main__":
    main()
