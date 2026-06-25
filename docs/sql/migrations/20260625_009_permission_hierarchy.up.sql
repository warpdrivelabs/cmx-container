-- =============================================
-- 权限表层级字段补全：parent_code / full_code_path / is_leaf / level
-- 用于支持基于路径的子权限 LIKE 查询与层级展示，替代低性能的递归 CTE。
-- =============================================

-- 1. 新增 4 个字段（full_code_path 暂允许 NULL，回填后再设 NOT NULL）
ALTER TABLE cmx_permission ADD COLUMN IF NOT EXISTS parent_code VARCHAR(200);
ALTER TABLE cmx_permission ADD COLUMN IF NOT EXISTS full_code_path VARCHAR(1000);
ALTER TABLE cmx_permission ADD COLUMN IF NOT EXISTS is_leaf INT4 DEFAULT 1;
ALTER TABLE cmx_permission ADD COLUMN IF NOT EXISTS level INT4 DEFAULT 1;

-- 2. 根节点 full_code_path 初始化（parent_id 为 NULL）
UPDATE cmx_permission
SET full_code_path = '/' || code
WHERE parent_id IS NULL AND full_code_path IS NULL;

-- 3. 递归回填所有节点的 full_code_path / level / parent_code（一次性脚本）
WITH RECURSIVE perm_path AS (
    SELECT id, code, parent_id, ('/' || code)::varchar AS path, 1 AS lvl
    FROM cmx_permission
    WHERE parent_id IS NULL
    UNION ALL
    SELECT c.id, c.code, c.parent_id, (p.path || '/' || c.code)::varchar, p.lvl + 1
    FROM cmx_permission c
    JOIN perm_path p ON c.parent_id = p.id
)
UPDATE cmx_permission x
SET full_code_path = pp.path,
    level = pp.lvl,
    parent_code = (SELECT code FROM cmx_permission WHERE id = x.parent_id)
FROM perm_path pp
WHERE x.id = pp.id;

-- 4. 回填 is_leaf：有子节点的父置 0
UPDATE cmx_permission
SET is_leaf = 0
WHERE id IN (
    SELECT DISTINCT parent_id
    FROM cmx_permission
    WHERE parent_id IS NOT NULL
);

-- 5. 补 NOT NULL 约束（回填完成后强制）
ALTER TABLE cmx_permission ALTER COLUMN full_code_path SET NOT NULL;

-- 6. 新增索引（支持 LIKE 前缀查询与 parent_code 查询）
CREATE INDEX IF NOT EXISTS idx_cmx_permission_full_path ON cmx_permission (full_code_path);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_parent_code ON cmx_permission (parent_code);
