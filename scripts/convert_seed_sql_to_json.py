#!/usr/bin/env python3
"""
种子 SQL → JSON 转换脚本。

把一份 DDL/DML 种子 SQL 文件里的 INSERT 语句按物理表名拆分为 JSON 数组文件，
每个表输出一个 `<table>.json`，文件内容为该表所有 INSERT 行组成的数组
（元素为 {列名: 值}）。

用法（向后兼容）：
  # 无参数：默认转换 cmxfico_dct_seed_data.sql → cf_*.json（Task 9 原行为）
  python3 scripts/convert_seed_sql_to_json.py

  # 显式参数：指定源 SQL 与目标目录
  python3 scripts/convert_seed_sql_to_json.py <src_sql> <dst_dir>
  示例：
  python3 scripts/convert_seed_sql_to_json.py docs/sql/seed/fico_cr_dict_seed.sql \
      data/meta/definitions/fi/cmxfico/report/seed

注意：
- 只解析 `INSERT INTO <table> (cols) VALUES (vals);`，DELETE/UPDATE/其他语句会被忽略。
- 字符串字面量用单引号包裹，内部连续两个单引号转义为一个单引号。
- NULL → null；TRUE/FALSE → boolean；其它数字字面量尝试 int → float。
- 无法识别的字面量（例如 CURRENT_TIMESTAMP）原样作为字符串保留。
"""
import re
import json
import os
import sys
from collections import defaultdict

# 默认源/目标（Task 9 原行为，无参数时使用）
DEFAULT_SRC = "docs/sql/seed/cmxfico_dct_seed_data.sql"
DEFAULT_DST_DIR = "data/meta/definitions/fi/cmxfico/gl/seed"

# 只匹配 INSERT INTO <table> (col1, col2, ...) VALUES (v1, v2, ...);
# 注意：DELETE/UPDATE 等语句不会匹配此正则，因此会被自动跳过。
INSERT_RE = re.compile(
    r"INSERT\s+INTO\s+(\w+)\s*\(([^)]+)\)\s*VALUES\s*\((.+?)\)\s*;",
    re.IGNORECASE | re.DOTALL,
)


def split_values(s):
    """把 VALUES 元组按逗号切分，处理单引号转义与嵌套。"""
    out = []
    cur = []
    in_str = False
    i = 0
    while i < len(s):
        c = s[i]
        if in_str:
            if c == "'":
                if i + 1 < len(s) and s[i + 1] == "'":
                    cur.append("'")
                    i += 2
                    continue
                in_str = False
            cur.append(c)
        else:
            if c == "'":
                in_str = True
                cur.append(c)
            elif c == ",":
                out.append("".join(cur).strip())
                cur = []
            else:
                cur.append(c)
        i += 1
    if cur:
        out.append("".join(cur).strip())
    return out


def parse_value(v):
    """把 SQL 字面量转为 JSON 值。"""
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


def convert_one(src, dst_dir):
    """解析一份 SQL 文件，按表名输出多个 <table>.json 到 dst_dir。

    返回 dict: {table_name: row_count}。
    """
    tables = defaultdict(list)
    with open(src, encoding="utf-8") as f:
        content = f.read()

    for m in INSERT_RE.finditer(content):
        table = m.group(1)
        cols = [c.strip() for c in m.group(2).split(",")]
        vals = [parse_value(v) for v in split_values(m.group(3))]
        assert len(cols) == len(vals), f"列数不匹配 {table}: {cols} vs {vals}"
        tables[table].append(dict(zip(cols, vals)))

    os.makedirs(dst_dir, exist_ok=True)
    for table, rows in tables.items():
        path = os.path.join(dst_dir, f"{table}.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(rows, f, ensure_ascii=False, indent=2)
        print(f"  {table}: {len(rows)} 行 → {path}")

    print(f"\n共转换 {len(tables)} 张表（源: {src}）")
    return {t: len(r) for t, r in tables.items()}


def main(argv):
    if len(argv) == 3:
        src, dst_dir = argv[1], argv[2]
    elif len(argv) == 1:
        # 无参数：保持 Task 9 原默认行为
        src, dst_dir = DEFAULT_SRC, DEFAULT_DST_DIR
    else:
        print(
            "用法: python3 scripts/convert_seed_sql_to_json.py [<src_sql> <dst_dir>]",
            file=sys.stderr,
        )
        return 2
    convert_one(src, dst_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
