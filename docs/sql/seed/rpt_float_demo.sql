-- 报表浮动行列（动态明细展开）P1 演示种子数据
-- 报表 AR_FLOAT_D「应收账款浮动明细（示例）」：一个 is_repeatable=1 的浮动数据区，
-- 内含一条 row_type='float' 模板行，模板列公式带 {{cust_code}} 维度占位符 + {{r}}/{{total}} 语义锚点。
-- 数据源 = 'sample'（内置前 5 大客户），展开引擎运行时把模板行复制成 5 条实例行。
-- 幂等：固定 id + ON CONFLICT，可反复执行。ID 均在 pk52 真号段（< 2^52），不撞派生实例号段。

BEGIN;

-- 报表主档
INSERT INTO cr_report_list (code, name, report_type, report_category, period_type, is_statutory, sort_no, status, create_time)
VALUES ('AR_FLOAT_D', '应收账款浮动明细(示例)', 'stat', 'statutory', 'month', 0, 900, 1, CURRENT_TIMESTAMP)
ON CONFLICT (code) DO UPDATE SET name=EXCLUDED.name, report_category=EXCLUDED.report_category, period_type=EXCLUDED.period_type;

-- 浮动数据区：is_repeatable=1，data_source='sample'（P1 内置示例源）
INSERT INTO cr_report_region
  (report_code, version_code, sheet_code, region_code, region_name, region_type,
   is_repeatable, data_source, start_row, start_col, end_row, end_col,
   is_merged, freeze_flag, sort_no, status, create_time)
VALUES
  ('AR_FLOAT_D', 'R1', 'Sheet1', 'RG_CUST', '客户明细浮动区', 'data',
   1, 'sample', 2, 1, 2, 4,
   0, 0, 1, 1, CURRENT_TIMESTAMP)
ON CONFLICT (report_code, version_code, sheet_code, region_code)
  DO UPDATE SET is_repeatable=EXCLUDED.is_repeatable, data_source=EXCLUDED.data_source,
                start_row=EXCLUDED.start_row, region_type=EXCLUDED.region_type;

-- 合计行（固定，row_type='total'，物理行 1）
INSERT INTO cr_report_row
  (id, code, name, report_code, version_code, sheet_code, region_code, row_no,
   row_type, full_path, level_no, is_leaf, is_bold, sort_no, status, create_time)
VALUES
  (51306400000001, 'TOTAL', '应收账款合计', 'AR_FLOAT_D', 'R1', 'Sheet1', 'RG_CUST', 0,
   'total', 'TOTAL', 1, 0, 1, 0, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, row_type=EXCLUDED.row_type;

-- 浮动模板行（row_type='float'，name 用 {{label}} 占位，物理行 2）
INSERT INTO cr_report_row
  (id, code, name, report_code, version_code, sheet_code, region_code, row_no,
   row_type, full_path, level_no, is_leaf, is_bold, sort_no, status, create_time)
VALUES
  (51306400000002, 'TPL_CUST', '{{label}}', 'AR_FLOAT_D', 'R1', 'Sheet1', 'RG_CUST', 1,
   'float', 'TPL_CUST', 1, 1, 0, 1, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, row_type=EXCLUDED.row_type;

-- 列：A 项目 / B 期末余额 / C 账龄 / D 占比
INSERT INTO cr_report_col
  (id, code, name, report_code, version_code, sheet_code, region_code, col_no,
   col_letter, col_type, full_path, level_no, is_leaf, is_hidden, sort_no, status, create_time)
VALUES
  (51306400001001, 'A', '项目',     'AR_FLOAT_D','R1','Sheet1','RG_CUST', 0, 'A', 'text', 'A', 1, 1, 0, 0, 1, CURRENT_TIMESTAMP),
  (51306400001002, 'B', '期末余额', 'AR_FLOAT_D','R1','Sheet1','RG_CUST', 1, 'B', 'period', 'B', 1, 1, 0, 1, 1, CURRENT_TIMESTAMP),
  (51306400001003, 'C', '账龄<1年', 'AR_FLOAT_D','R1','Sheet1','RG_CUST', 2, 'C', 'period', 'C', 1, 1, 0, 2, 1, CURRENT_TIMESTAMP),
  (51306400001004, 'D', '占比',     'AR_FLOAT_D','R1','Sheet1','RG_CUST', 3, 'D', 'calc', 'D', 1, 1, 0, 3, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, col_letter=EXCLUDED.col_letter;

-- 模板行的列公式（row_id = 模板行 id）：带 {{cust_code}} 维度 + {{r}}/{{total}} 锚点
-- B 期末余额 = QM 取该客户余额；C 账龄；D 占比 = B本行/B合计
INSERT INTO cr_cell_element_map
  (id, code, report_code, version_code, sheet_code, region_code, row_id, col_id,
   cell_ref, calc_formula, is_editable, sort_no, status, create_time)
VALUES
  (51306400002001, 'M_B', 'AR_FLOAT_D','R1','Sheet1','RG_CUST', 51306400000002, 51306400001002,
   'B2', 'QM(0,@current,''{{cust_code}}'')', 0, 0, 1, CURRENT_TIMESTAMP),
  (51306400002002, 'M_C', 'AR_FLOAT_D','R1','Sheet1','RG_CUST', 51306400000002, 51306400001003,
   'C2', 'QC(0,@current,''{{cust_code}}'')', 0, 1, 1, CURRENT_TIMESTAMP),
  (51306400002003, 'M_D', 'AR_FLOAT_D','R1','Sheet1','RG_CUST', 51306400000002, 51306400001004,
   'D2', '=B{{r}}/B{{total}}', 0, 2, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET calc_formula=EXCLUDED.calc_formula, cell_ref=EXCLUDED.cell_ref;

COMMIT;

-- 校验
SELECT 'report' AS kind, code AS k, name AS v FROM cr_report_list WHERE code='AR_FLOAT_D'
UNION ALL SELECT 'region', region_code, region_type||' rep='||is_repeatable||' src='||COALESCE(data_source,'')
  FROM cr_report_region WHERE report_code='AR_FLOAT_D'
UNION ALL SELECT 'row', code, row_type FROM cr_report_row WHERE report_code='AR_FLOAT_D'
UNION ALL SELECT 'cellmap', code, calc_formula FROM cr_cell_element_map WHERE report_code='AR_FLOAT_D';
