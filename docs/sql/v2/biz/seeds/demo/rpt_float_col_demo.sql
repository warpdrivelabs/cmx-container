-- P3 列浮动演示：报表 COL_FLOAT_D「按月横向铺列（示例）」。
-- 一个 axis=col 浮动区，内含一条 col_type='float' 模板列；模板列在第2行放收入公式(QM按月)、
-- 第3行放占比(本列/汇总列B)。数据源 sample-cols = 近6个月，运行时把模板列铺成 6 列。
-- 幂等：固定 id + ON CONFLICT。ID 在 pk52 真号段(< 2^52)。

BEGIN;

INSERT INTO cr_report_list (code, name, report_type, report_category, period_type, is_statutory, sort_no, status, create_time)
VALUES ('COL_FLOAT_D', '按月横向铺列(示例)', 'stat', 'statutory', 'month', 0, 901, 1, CURRENT_TIMESTAMP)
ON CONFLICT (code) DO UPDATE SET name=EXCLUDED.name, report_category=EXCLUDED.report_category, period_type=EXCLUDED.period_type;

-- 列浮动区：data_source='sample-cols'（近6月）。区域类型 data。
INSERT INTO cr_report_region
  (report_code, version_code, sheet_code, region_code, region_name, region_type,
   is_repeatable, data_source, start_row, start_col, end_row, end_col,
   is_merged, freeze_flag, sort_no, status, create_time)
VALUES
  ('COL_FLOAT_D', 'R1', 'Sheet1', 'RG_MONTH', '月份横向浮动区', 'data',
   1, 'sample-cols', 1, 2, 3, 2,
   0, 0, 1, 1, CURRENT_TIMESTAMP)
ON CONFLICT (report_code, version_code, sheet_code, region_code)
  DO UPDATE SET is_repeatable=EXCLUDED.is_repeatable, data_source=EXCLUDED.data_source,
                start_col=EXCLUDED.start_col, region_type=EXCLUDED.region_type;

-- 固定行标（A列）：行2=营业收入、行3=占比。行1=表头。用普通 data 行占位。
INSERT INTO cr_report_row
  (id, code, name, report_code, version_code, sheet_code, region_code, row_no,
   row_type, full_path, level_no, is_leaf, is_bold, sort_no, status, create_time)
VALUES
  (51306500000001, 'RH', '项目',     'COL_FLOAT_D','R1','Sheet1','RG_MONTH', 0, 'title', 'RH', 1, 1, 1, 0, 1, CURRENT_TIMESTAMP),
  (51306500000002, 'R_REV', '营业收入','COL_FLOAT_D','R1','Sheet1','RG_MONTH', 1, 'data', 'R_REV', 1, 1, 0, 1, 1, CURRENT_TIMESTAMP),
  (51306500000003, 'R_PCT', '占比',   'COL_FLOAT_D','R1','Sheet1','RG_MONTH', 2, 'data', 'R_PCT', 1, 1, 0, 2, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, row_type=EXCLUDED.row_type;

-- 列：A 项目 / B 汇总（固定）/ FLOAT 模板列(col_type='float')
INSERT INTO cr_report_col
  (id, code, name, report_code, version_code, sheet_code, region_code, col_no,
   col_letter, col_type, full_path, level_no, is_leaf, is_hidden, sort_no, status, create_time)
VALUES
  (51306500001001, 'A', '项目',   'COL_FLOAT_D','R1','Sheet1','RG_MONTH', 0, 'A', 'text',  'A', 1, 1, 0, 0, 1, CURRENT_TIMESTAMP),
  (51306500001002, 'B', '全年汇总','COL_FLOAT_D','R1','Sheet1','RG_MONTH', 1, 'B', 'period','B', 1, 1, 0, 1, 1, CURRENT_TIMESTAMP),
  (51306500001003, 'TPL_M', '{{label}}','COL_FLOAT_D','R1','Sheet1','RG_MONTH', 2, 'C', 'float', 'TPL_M', 1, 1, 0, 2, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, col_type=EXCLUDED.col_type, col_letter=EXCLUDED.col_letter;

-- 模板列的各行公式（col_id = 模板列 id；行由 cell_ref 定位）：
--   行2(C2) 营业收入 = QM 按月取；行3(C3) 占比 = 本列/B列(全年汇总)。{{c}} = 本实例列列标。
INSERT INTO cr_cell_element_map
  (id, code, report_code, version_code, sheet_code, region_code, row_id, col_id,
   cell_ref, calc_formula, is_editable, sort_no, status, create_time)
VALUES
  (51306500002001, 'CM_REV', 'COL_FLOAT_D','R1','Sheet1','RG_MONTH', 51306500000002, 51306500001003,
   'C2', 'QM(''{{period_code}}'',@current,''6001'')', 0, 0, 1, CURRENT_TIMESTAMP),
  (51306500002002, 'CM_PCT', 'COL_FLOAT_D','R1','Sheet1','RG_MONTH', 51306500000003, 51306500001003,
   'C3', '={{c}}2/B2', 0, 1, 1, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO UPDATE SET calc_formula=EXCLUDED.calc_formula, cell_ref=EXCLUDED.cell_ref;

COMMIT;

SELECT 'region' AS kind, region_code AS k, region_type||' src='||COALESCE(data_source,'') AS v FROM cr_report_region WHERE report_code='COL_FLOAT_D'
UNION ALL SELECT 'col', code, col_type FROM cr_report_col WHERE report_code='COL_FLOAT_D'
UNION ALL SELECT 'cellmap', code, calc_formula FROM cr_cell_element_map WHERE report_code='COL_FLOAT_D';
