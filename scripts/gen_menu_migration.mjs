#!/usr/bin/env node
// gen_menu_migration.mjs -- 菜单文件(menu-pages JSON) -> cmx_menu INSERT SQL 生成器。
//
// 背景：菜单原以 JSON 文件存放在 data/menu-pages/<domain>/<app>/<module>/<file>.json，
// 现迁移到数据库 cmx_menu 表（节点级映射：每节点一行，workspace/dialogspace 等富数据入
// definition JSONB）。本脚本读取菜单文件，递归展平树、计算树形字段(depth/code_path/
// id_path/leaf/parent)、处理跨文件重复 code，输出可核对、可重跑的 INSERT 语句。
//
// 产物：
//   docs/sql/migrations/<日期>_001_menu_pages_to_cmx_menu.up.sql   （INSERT 语句）
//   docs/sql/migrations/<日期>_001_menu_pages_to_cmx_menu.down.sql （DELETE 回滚）
//
// 用法：
//   node scripts/gen_menu_migration.mjs
//
// 重复 code 处理：explorer-menu 先导（code 原样）；report-menu 与已存 code 冲突的追加
// `_rpt` 后缀，子节点 parent_code/parent_id 跟随父节点重命名同步。
//
// 树形字段格式与 MenuService::compute_tree_fields 一致：
//   根：code_path=/code，id_path=/id，depth=1
//   子：code_path={父code_path}/code，id_path={父id_path}/id，depth=父+1

import { readFileSync, writeFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')

// 待迁移的菜单文档（domain/application/module 从 menuRef 前 3 段解析）。
const DOCS = [
  { file: 'data/menu-pages/fi/cmxfico/gl/explorer-menu.json', domain: 'fi', app: 'cmxfico', module: 'gl' },
  { file: 'data/menu-pages/fi/cmxfico/report/report-menu.json', domain: 'fi', app: 'cmxfico', module: 'report' },
]

const DATE = '20260716'
const UP_PATH = `docs/sql/migrations/${DATE}_001_menu_pages_to_cmx_menu.up.sql`
const DOWN_PATH = `docs/sql/migrations/${DATE}_001_menu_pages_to_cmx_menu.down.sql`

/**
 * 递归展平一棵菜单树为节点数组。
 * @param {any[]} items 菜单节点数组
 * @param {string|null} parentCode 父节点（已重命名后的）code，顶层为 null
 * @param {number} depth 当前深度（根=1）
 * @param {string} codePath 父 code_path（根前为 ''）
 * @param {string} idPath 父 id_path（根前为 ''）
 * @param {string} domain / app / module
 * @param {Set<string>} usedCodes 已用 code 集合（跨文档去重）
 * @param {boolean} suffix 冲突时是否加 _rpt 后缀（report 文档用）
 * @returns {object[]} 扁平节点数组
 */
function flatten (items, parentCode, depth, codePath, idPath, domain, app, module, usedCodes, suffix) {
  const out = []
  if (!Array.isArray(items)) return out
  items.forEach((node, i) => {
    let code = String(node.id ?? '')
    if (!code) return
    // 跨文档重复 code 处理：report 冲突加 _rpt
    if (suffix && usedCodes.has(code)) code = code + '_rpt'
    usedCodes.add(code)

    const id = `${domain}.${app}.${module}/${code}`
    const newCodePath = codePath + '/' + code
    const newIdPath = idPath + '/' + id

    const caption = node.caption
    // name 列：caption 为字符串直接用，否则回退内部 name 或 code
    const nameCol = typeof caption === 'string' ? caption : (node.name ?? code)

    // definition JSONB：保留 caption(含 i18n 对象)/workspace/dialogspace/expanded/type/name
    const definition = {}
    if (caption != null) definition.caption = caption
    if (node.workspace) definition.workspace = node.workspace
    if (node.dialogspace) definition.dialogspace = node.dialogspace
    if (node.expanded != null) definition.expanded = node.expanded
    if (node.type) definition.type = node.type
    if (node.name) definition.name = node.name

    const children = Array.isArray(node.children) ? node.children : []
    out.push({
      id, code, name: String(nameCol), icon: node.icon ?? null, fun_code: node.permissionId ?? null,
      sort_order: i + 1, definition, domain_code: domain, application_code: app, module_code: module,
      parent_id: parentCode ? `${domain}.${app}.${module}/${parentCode}` : null,
      parent_code: parentCode, depth, leaf: children.length === 0 ? 1 : 0,
      code_path: newCodePath, id_path: newIdPath,
    })
    out.push(...flatten(children, code, depth + 1, newCodePath, newIdPath, domain, app, module, usedCodes, suffix))
  })
  return out
}

/** SQL 字面量转义 */
function sqlVal (v) {
  if (v == null) return 'NULL'
  if (typeof v === 'number') return String(v)
  return `'${String(v).replace(/'/g, "''")}'`
}

/** JSONB 字面量 */
function sqlJson (obj) {
  return `'${JSON.stringify(obj).replace(/'/g, "''")}'::jsonb`
}

// ── 主流程 ──
const usedCodes = new Set()
const all = []
for (const d of DOCS) {
  const raw = readFileSync(resolve(ROOT, d.file), 'utf8')
  const doc = JSON.parse(raw)
  const items = Array.isArray(doc) ? doc : (doc.items ?? [])
  // explorer 原样（suffix=false）；report 冲突加 _rpt（suffix=true）
  const suffix = d.module === 'report'
  all.push(...flatten(items, null, 1, '', '', d.domain, d.app, d.module, usedCodes, suffix))
}

const COLS = 'id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time'
const lines = all.map(n => {
  const vals = [
    sqlVal(n.id), sqlVal(n.code), sqlVal(n.name), sqlVal(n.icon), sqlVal(n.fun_code),
    sqlVal(n.sort_order), sqlJson(n.definition), sqlVal(n.domain_code), sqlVal(n.application_code), sqlVal(n.module_code),
    sqlVal(n.parent_id), sqlVal(n.parent_code), sqlVal(n.depth), sqlVal(n.leaf), sqlVal(n.code_path), sqlVal(n.id_path),
    '1', '1', '0', '0', 'now()', 'now()',
  ]
  return `INSERT INTO cmx_menu (${COLS}) VALUES (${vals.join(', ')}) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;`
})

const up = `-- 菜单文件(menu-pages)迁移到 cmx_menu（由 scripts/gen_menu_migration.mjs 生成，可重跑）
-- 节点级映射：workspace/dialogspace 等富数据入 definition JSONB；report 冲突 code 加 _rpt 后缀。

${lines.join('\n')}
`
writeFileSync(resolve(ROOT, UP_PATH), up)

const down = `-- 回滚：清理迁移导入的菜单（fi/cmxfico 下 gl、report 两模块）
DELETE FROM cmx_menu WHERE domain_code = 'fi' AND application_code = 'cmxfico' AND module_code IN ('gl', 'report');
`
writeFileSync(resolve(ROOT, DOWN_PATH), down)

console.log(`生成 ${all.length} 条 INSERT -> ${UP_PATH}`)
console.log(`  explorer(gl): ${all.filter(n => n.module_code === 'gl').length} 条`)
console.log(`  report: ${all.filter(n => n.module_code === 'report').length} 条（含 ${all.filter(n => n.code.endsWith('_rpt')).length} 个 _rpt 重命名）`)
