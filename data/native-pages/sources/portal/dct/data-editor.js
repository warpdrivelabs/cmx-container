/**
 * data-editor —— 通用字典数据维护页（元数据驱动，native_pages）。
 *
 * 通过 props 注入字典坐标四元组，根据 dictCode 从 `/api/dct/meta` 加载 DictView，
 * 动态构建列模型（含 edit.mode 配置），在 cmx-revo-grid 中**行内编辑**维护数据。
 *
 * 设计要点：
 *   - 平级字典（selfHierarchy=false）：单表格分页（cmx-pager）
 *   - 树形字典（selfHierarchy=true）：左树右表，右表显示选中节点的直接子节点
 *   - 编辑：grid editable:true + editTrigger:'click'，单击单元格直接编辑（按列 edit.mode）
 *           引用字典列用 edit.mode='dict-select'（cmx-dict-select 弹出选择）
 *   - 新增/删除：grid.addRow / grid.removeRows（DataSet 模式）
 *   - 保存：commitGridEdits（收拢未提交编辑）→ 改动行（新增/修改/删除）→ /api/dct/save changeset
 *   - 分页：复用 cmx-pager 组件，监听 page-change 事件
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }；ctx.props 来自菜单。
 * CMX 类经 globalThis.__cmxDataComp 取用。
 *
 * 字段策略（依据元数据角色）：
 *   - 业务列（code/name/sort_no/status/own fields/refDict 列/parent 列/生效期/停用信息）→ 列表显示 + 行内可编辑
 *   - 审计列（create_by/create_time/update_by/update_time）→ 列表显示，不可编辑（后端自动维护）
 *   - 系统标识（is_system）→ 列表显示，不可编辑
 *   - 主键（id/code 作 PK）→ 列表显示，不可编辑
 *   - 派生层级（full_path/level_no/is_leaf）→ 列表与编辑均隐藏（后端 backfill）
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

/* ─────────────── 字段角色识别 ─────────────── */
const AUDIT_FIELDS = new Set(['create_by', 'create_time', 'update_by', 'update_time'])
const SYSTEM_FLAG_FIELDS = new Set(['is_system'])
const DERIVED_HIERARCHY = new Set(['full_path', 'level_no', 'is_leaf'])

/** 主键是否由后端自动生成（整数主键铸号）。
 *  判据与后端 pk_is_generated 一致：主键列 dataType 含 "INT" → 服务端生成，前端不填。
 *  反之（如 code 作 PK 的字符串业务键）→ 用户新增时必填，保存后不可改。 */
function isPkGenerated (meta) {
  const pkCol = (meta.columns || []).find((c) => c.name === meta.pk)
  if (!pkCol) return false
  return String(pkCol.dataType || '').toUpperCase().includes('INT')
}

function showInTable (col, meta) {
  if (DERIVED_HIERARCHY.has(col.name)) return false
  return true
}

/** 行内是否可编辑：
 *  - id 主键（整数，后端铸号）→ 不可编辑
 *  - code 主键（字符串业务键）→ 可编辑（用 readonlyWhen 限定：仅新增行可填，已存在行只读）
 *  - 审计/系统标识/派生层级 → 不可编辑 */
function isEditable (col, meta) {
  if (col.name === meta.pk) return !isPkGenerated(meta)
  if (AUDIT_FIELDS.has(col.name)) return false
  if (SYSTEM_FLAG_FIELDS.has(col.name)) return false
  if (DERIVED_HIERARCHY.has(col.name)) return false
  return true
}

function colCaption (col) {
  if (col.caption && typeof col.caption === 'object') return col.caption.zh_CN || col.caption.en || col.name
  return col.caption || col.name
}

function defaultWidthFor (col) {
  const t = String(col.dataType || '').toUpperCase()
  if (t === 'DATETIME') return '160px'
  if (t === 'DATE') return '130px'
  if (t === 'TEXT') return '240px'
  if (t === 'TINYINT') return '90px'
  if (t === 'INT' || t === 'BIGINT') return '110px'
  return '150px'
}

/** 把 DCT 元数据的 edit.mode 映射到 cmx-revo-grid 列的规范 edit.mode（EDIT_MODES 值域）。
 *
 * 规范值域见 cmx-field-uicontrol.js 的 EDIT_MODES：
 *   cmx-text-input / cmx-textarea-input / cmx-richtext-input / cmx-number-input /
 *   cmx-date-input / cmx-datetime-input / checkbox / select / ref / combo /
 *   ignite-combo / cmx-dict-selct / image / video / readonly / none
 *
 * 其中：
 *   - `ref` 是合法 edit.mode，但**无注册编辑器**（adapter 认但 runtime 退化），
 *     本页把 ref + refDict 转成 `cmx-dict-selct`（字典选择弹窗，有完整编辑器实现）。
 *   - `cmx-dict-selct` 是字典选择的规范存储值（历史拼写），runtime kind 映射到 dict-select。
 *
 * 元数据 column 自带的 edit.mode 可能是规范值，也可能是简写（input/text/number/date/datetime），
 * 这里统一收敛到规范值。返回 { mode, ...附加配置 }。 */
// 元数据简写 → 规范值的映射（非 EDIT_MODES 的简写收敛）
const META_MODE_TO_SPEC = {
  input: 'cmx-text-input',
  text: 'cmx-text-input',
  textarea: 'cmx-textarea-input',
  number: 'cmx-number-input',
  date: 'cmx-date-input',
  datetime: 'cmx-datetime-input',
}
// EDIT_MODES 规范值集合（用于判断 metaMode 是否已是规范值）
const SPEC_MODES = new Set([
  'cmx-text-input', 'cmx-textarea-input', 'cmx-richtext-input', 'cmx-number-input',
  'cmx-date-input', 'cmx-datetime-input', 'checkbox', 'select', 'ref', 'combo',
  'ignite-combo', 'cmx-dict-selct', 'image', 'video', 'readonly', 'none',
])

function editModeFor (col, meta) {
  const name = col.name
  const t = String(col.dataType || '').toUpperCase()
  const metaEdit = col.edit && typeof col.edit === 'object' ? col.edit : null
  const metaMode = metaEdit ? String(metaEdit.mode || '') : ''
  const isParent = meta.selfHierarchy && name === meta.parentField

  // 1) ref + refDict（或树形字典父节点列）→ cmx-dict-selct 字典选择弹窗
  //    ref 本身无注册编辑器，统一转成 cmx-dict-selct（有完整实现）
  if (metaMode === 'ref' || (col.refDict && !metaMode) || isParent) {
    const dictCode = col.refDict || (isParent ? meta.dictCode : '')
    if (dictCode) {
      return {
        mode: 'cmx-dict-selct',
        dictCode,
        idField: col.refField || (isParent ? meta.pk : 'code'),
        labelField: col.displayField || (isParent ? meta.labelField : 'name'),
        parentField: isParent ? meta.parentField : undefined,
        hierarchical: !!isParent,
      }
    }
  }

  // 2) 元数据已是 EDIT_MODES 规范值 → 直接用（checkbox/select/combo/readonly/none/cmx-*-input 等）
  if (metaMode && SPEC_MODES.has(metaMode)) {
    const out = { mode: metaMode }
    if (metaMode === 'select') {
      out.options = (metaEdit && Array.isArray(metaEdit.options)) ? metaEdit.options
        : (name === 'status' ? [{ value: 1, label: '启用' }, { value: 0, label: '停用' }] : [])
    }
    return out
  }

  // 3) 元数据简写（input/text/number/date/datetime 等）→ 规范值
  const lower = metaMode.toLowerCase()
  if (META_MODE_TO_SPEC[lower]) {
    return { mode: META_MODE_TO_SPEC[lower] }
  }

  // 4) 兜底：按 dataType 推断（元数据未给 edit.mode 或未识别）
  if (t === 'DATE') return { mode: 'cmx-date-input' }
  if (t === 'DATETIME') return { mode: 'cmx-datetime-input' }
  if (t === 'TINYINT' && name === 'status') return { mode: 'checkbox' }
  if (t === 'INT' || t === 'BIGINT' || t === 'TINYINT' || t === 'DECIMAL') return { mode: 'cmx-number-input' }
  return { mode: 'cmx-text-input' }
}

/** 把 DCT 元数据的 display 配置映射到 cmx-revo-grid 列的 display 对象。
 *
 *  display.mode 规范取值（以 cmx-field-schema.js DISPLAY 段录入选项为准）：
 *    '' / 'text' / 'number' / 'badge' / 'link' / 'icon'
 *  - 'number' 是合法模式：联动显示 format/decimalDigits/thousandSeparator/zeroAsBlank/negativeColor
 *  - 'text' 原样字符串；'badge'/'link'/'icon' 各有专属属性（badgeMap/icon/link）
 *  - 元数据可能给非规范值（如 date/checkbox），这些由 dataType 自动派生，丢弃
 *
 *  数值类属性（schema 用 visibleWhen=displayModeIn(['','number']) 联动）：
 *    format / decimalDigits / thousandSeparator / zeroAsBlank / negativeColor
 *  全部透传给列 display。negativeColor 是 boolean（false 关闭负数红字，adapter 默认开）。 */
const SPEC_DISPLAY_MODES = new Set(['', 'text', 'number', 'badge', 'link', 'icon'])
function displayFor (col) {
  const d = col.display && typeof col.display === 'object' ? col.display : null
  if (!d) return undefined
  const out = {}
  // mode：只透传 schema 规范值（含 ''/text/number/badge/link/icon），非规范值（date/checkbox 等）丢弃
  const m = d.mode == null ? '' : String(d.mode).toLowerCase()
  if (SPEC_DISPLAY_MODES.has(m)) out.mode = m
  if (d.align) out.align = d.align
  if (d.format) out.format = d.format
  if (d.decimalDigits != null) out.decimalDigits = d.decimalDigits
  if (d.thousandSeparator != null) out.thousandSeparator = d.thousandSeparator
  if (d.zeroAsBlank != null) out.zeroAsBlank = d.zeroAsBlank
  if (d.negativeColor != null) out.negativeColor = d.negativeColor
  if (d.emptyText != null) out.emptyText = d.emptyText
  if (d.badgeMap) out.badgeMap = d.badgeMap
  if (d.icon) out.icon = d.icon
  if (d.link) out.link = d.link
  if (d.cellStyle) out.cellStyle = d.cellStyle
  return Object.keys(out).length ? out : undefined
}

/* ─────────────── 模块级 state（每次 content 入口重置） ─────────────── */
const state = {
  def: null,
  dicts: [],
  meta: null,
  dictCode: '',
  page: 1,
  pageSize: 50,
  q: '',
  conds: [],
  treeNodes: {},
  currentParentId: null,
  selectedTreeNodeId: null,
  rows: [],
  total: 0,
  grid: null,
  // 本地变更跟踪：用 cmx-cell-changed 收集 + grid.getSource() 收拢
  _lastDictCode: null,
  selectedRowId: null,
  selectedIds: [],
  // 行级变更：{ id -> { fields, baseline, isNew } }，用于 save 时构造 changeset
  dirtyMap: {},
  newIds: new Set(),  // 本次会话新增的行 id
  deletedIds: [],
  baselineMap: {},    // id -> update_time（乐观锁）
}

/* ─────────────── HTTP 辅助 ─────────────── */
function unwrap (res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) {
      const e = new Error(body.msg || `业务错误 code ${body.code}`)
      e.body = body
      throw e
    }
    return body.data
  }
  if (!res.ok) {
    const e = new Error((body && body.error) || `HTTP ${res.status}`)
    e.status = res.status
    e.body = body
    throw e
  }
  return body
}

async function apiGet (url, dbId) {
  const headers = { Accept: 'application/json' }
  if (dbId) headers.db_id = dbId
  const res = await fetch(url, { headers, credentials: 'same-origin' })
  const body = await res.json().catch(() => null)
  return unwrap(res, body)
}

async function apiPost (url, payload, dbId) {
  const headers = { 'Content-Type': 'application/json', Accept: 'application/json' }
  if (dbId) headers.db_id = dbId
  const res = await fetch(url, { method: 'POST', headers, credentials: 'same-origin', body: JSON.stringify(payload || {}) })
  const body = await res.json().catch(() => null)
  try {
    return unwrap(res, body)
  } catch (e) {
    e.status = res.status
    e.body = body
    throw e
  }
}

function qs (def, extra = {}) {
  return new URLSearchParams({
    domain: def.domain, application: def.application, module: def.module, ...extra,
  }).toString()
}

/* ─────────────── 样式（Neo 主题） ─────────────── */
function styleHtml () {
  return `<style>
.de-root{--neo-cyan:#00b4d8;--neo-mint:#10b981;--neo-warn:#f59e0b;--neo-red:#e90b0b;
  display:flex;flex-direction:column;height:100%;width:100%;box-sizing:border-box;padding:10px;gap:10px;
  min-width:0;font:13px/1.5 var(--sapFontFamily,Arial,sans-serif);
  color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#f5f6f7);overflow:hidden}
.de-bar{flex:0 0 auto;display:flex;align-items:center;gap:10px;height:46px;box-sizing:border-box;padding:0 12px;
  border-bottom:1px solid color-mix(in srgb,var(--neo-cyan) 22%,var(--sapGroup_TitleBorderColor,#d9d9d9));
  background:color-mix(in srgb,var(--neo-cyan) 12%,var(--sapList_HeaderBackground,#eef2f6));border-radius:8px 8px 0 0}
.de-bar ui5-icon{width:1.2rem;height:1.2rem;color:var(--neo-cyan)}
.de-title{font-weight:700;font-size:14px;color:var(--neo-cyan)}
.de-subtitle{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-msg{margin-left:auto;font-size:12px;color:var(--neo-cyan)}
.de-msg.err{color:var(--neo-red)}
.de-msg.ok{color:var(--neo-mint)}
.de-toolbar{flex:0 0 auto;display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:6px 10px;
  background:var(--sapList_Background,#fff);border:1px solid color-mix(in srgb,var(--neo-cyan) 18%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  border-radius:6px}
.de-toolbar > ui5-label{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-dict-select{min-width:220px}
.de-filter{flex:0 0 auto;display:flex;gap:8px;align-items:center;flex-wrap:wrap;padding:8px 10px;
  background:color-mix(in srgb,var(--neo-cyan) 6%,var(--sapList_Background,#fff));
  border:1px solid color-mix(in srgb,var(--neo-cyan) 22%,transparent);border-radius:6px;font-size:12px}
.de-cond{display:flex;gap:4px;align-items:center;background:var(--sapField_Background,#fff);
  border-radius:4px;padding:3px 6px;border:1px solid color-mix(in srgb,var(--neo-cyan) 25%,var(--sapField_BorderColor,#b3b3b3))}
.de-body{flex:1;display:flex;gap:10px;min-height:0;min-width:0}
.de-body.flat{flex-direction:column}
.de-right{flex:1;display:flex;flex-direction:column;min-width:0;min-height:0;
  background:var(--sapList_Background,#fff);border:1px solid color-mix(in srgb,var(--neo-cyan) 15%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  border-radius:6px;overflow:hidden}
.de-grid-wrap{flex:1;min-height:0;position:relative}
.de-grid{display:block;width:100%;height:100%}
.de-tree{flex:0 0 280px;display:flex;flex-direction:column;
  background:var(--sapList_Background,#fff);border:1px solid color-mix(in srgb,var(--neo-cyan) 15%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  border-radius:6px;overflow:hidden}
.de-tree-head{padding:8px 12px;border-bottom:1px solid color-mix(in srgb,var(--neo-cyan) 22%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  background:color-mix(in srgb,var(--neo-cyan) 8%,var(--sapList_HeaderBackground,#f5f6f7));font-weight:600;font-size:13px;color:var(--neo-cyan)}
.de-tree-body{flex:1;overflow:auto;padding:6px}
.de-tree-node{display:flex;align-items:center;gap:4px;padding:5px 8px;cursor:pointer;border-radius:4px;font-size:13px}
.de-tree-node:hover{background:color-mix(in srgb,var(--neo-cyan) 10%,var(--sapList_Background,#fff))}
.de-tree-node.active{background:color-mix(in srgb,var(--neo-cyan) 18%,var(--sapList_Background,#fff));font-weight:600;color:var(--neo-cyan)}
.de-tree-toggle{cursor:pointer;user-select:none;width:14px;display:inline-block;color:var(--sapContent_LabelColor,#6a6d70);font-size:11px}
.de-tree-label{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.de-tree-meta{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-tree-root{font-size:13px;padding:5px 8px;cursor:pointer;border-radius:4px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-tree-root:hover{background:color-mix(in srgb,var(--neo-cyan) 10%,var(--sapList_Background,#fff))}
.de-tree-root.active{color:var(--neo-cyan);font-weight:600;background:color-mix(in srgb,var(--neo-cyan) 14%,var(--sapList_Background,#fff))}
.de-children{margin-left:16px}
.de-dirty{color:var(--neo-warn);font-weight:700}
.de-loading,.de-empty{padding:32px;text-align:center;color:var(--sapContent_LabelColor,#6a6d70);font-size:13px}
</style>`
}

/* ─────────────── 页面骨架 ─────────────── */
function pageHtml () {
  const isTree = state.meta && state.meta.selfHierarchy
  const bodyClass = isTree ? 'de-body tree' : 'de-body flat'
  const treePart = isTree ? `
    <div class="de-tree">
      <div class="de-tree-head">字典结构</div>
      <div class="de-tree-body" id="deTreeBody"></div>
    </div>` : ''
  return `${styleHtml()}
<div class="de-root">
  <div class="de-bar">
    <ui5-icon name="database"></ui5-icon>
    <span class="de-title">字典数据维护</span>
    <span class="de-subtitle" id="deSubtitle"></span>
    <span class="de-msg" id="deMsg"></span>
  </div>
  <div class="de-toolbar">
    <ui5-label>字典：</ui5-label>
    <ui5-select id="deDictSelect" class="de-dict-select"></ui5-select>
    <ui5-button design="Emphasized" icon="refresh" id="btnReload">刷新</ui5-button>
    <ui5-button design="Default" icon="add" id="btnAdd">新增</ui5-button>
    <ui5-button design="Default" icon="delete" id="btnDel">删除</ui5-button>
    <ui5-button design="Positive" icon="save" id="btnSave">保存</ui5-button>
  </div>
  <div class="de-filter">
    <ui5-input id="deQ" placeholder="关键字（编码/名称模糊匹配）" style="max-width:240px"></ui5-input>
    <ui5-button design="Transparent" icon="search" id="btnSearch">搜索</ui5-button>
    <ui5-button design="Transparent" icon="clear-all" id="btnClearCond">清空</ui5-button>
  </div>
  <div class="${bodyClass}" id="deBody">
    ${treePart}
    <div class="de-right">
      <div class="de-grid-wrap"><!-- grid 由 applyRowsToGrid 动态创建 --></div>
      <cmx-pager id="dePager" page-size="50" page-sizes="20,50,100,200"></cmx-pager>
    </div>
  </div>
</div>`
}

/* ─────────────── 提示辅助 ─────────────── */
function setMsg (root, text, kind) {
  const el = root.querySelector('#deMsg')
  if (!el) return
  el.textContent = text || ''
  el.className = 'de-msg' + (kind ? ' ' + kind : '')
}

function cmxNotify (kind, msg) {
  const C = cmx()
  const map = { info: 'cmxInfo', warn: 'cmxWarn', error: 'cmxError', ok: 'cmxInfo' }
  const fn = C && C[map[kind] || map.info]
  if (typeof fn === 'function') { try { fn(msg) } catch (_) {} return }
  const dlg = document.createElement('cmx-floating-dialog')
  if (typeof dlg.configure !== 'function') { console.log(`[dict-editor:${kind}]`, msg); return }
  const iconMap = { info: 'message-information', warn: 'message-warning', error: 'message-error', ok: 'message-success' }
  dlg.configure({
    title: { info: '提示', warn: '警告', error: '错误', ok: '成功' }[kind] || '提示',
    icon: iconMap[kind] || 'message-information',
    confirmText: '知道了',
    showCancel: false,
    dialogWidth: '480px',
    dialogHeight: 'auto',
  })
  const body = document.createElement('div')
  body.style.cssText = 'padding:16px;font-size:13px;color:var(--sapTextColor,#1d2d3e);line-height:1.6;white-space:pre-wrap;max-height:60vh;overflow:auto'
  body.textContent = String(msg == null ? '' : msg)
  dlg.setContent(body)
  document.body.appendChild(dlg)
  void dlg.openModal().then(() => {}).catch(() => {})
}

/** 确认对话框（基于 cmx-floating-dialog 命令式 API）。 */
async function cmxConfirm (message, title) {
  const C = cmx()
  if (C && typeof C.cmxConfirm === 'function') {
    try { return !!(await C.cmxConfirm(message, title || '请确认')) } catch (_) {}
  }
  const dlg = document.createElement('cmx-floating-dialog')
  if (typeof dlg.configure !== 'function') return false
  dlg.configure({
    title: title || '请确认',
    icon: 'message-information',
    confirmText: '确定',
    cancelText: '取消',
    dialogWidth: '420px',
    dialogHeight: 'auto',
  })
  const body = document.createElement('div')
  body.style.cssText = 'padding:16px;font-size:13px;color:var(--sapTextColor,#1d2d3e);line-height:1.6;white-space:pre-wrap'
  body.textContent = String(message == null ? '' : message)
  dlg.setContent(body)
  document.body.appendChild(dlg)
  try { const r = await dlg.openModal(); return r && r.action === 'confirm' } catch (_) { return false }
}

/* ─────────────── 异步等待渲染 ─────────────── */
function whenRendered (host, selector, cb, tries) {
  const t = tries == null ? 60 : tries
  const root = host && host.renderRoot
  if (root && root.querySelector(selector)) { cb(root); return }
  if (t <= 0) return
  requestAnimationFrame(() => whenRendered(host, selector, cb, t - 1))
}

/* ─────────────── HTML 转义 ─────────────── */
function escHtml (s) {
  return String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c])
}
function escAttr (s) {
  return String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c])
}

/* ─────────────── props 归一 ─────────────── */
function readDef (ctx) {
  const p = (ctx && ctx.props) || {}
  const def = {
    domain: p.domain || '',
    application: p.application || '',
    module: p.module || '',
    dbId: p.dbId || p.db_id || '',
    initialDictCode: p.dictCode || p.dict || '',
  }
  return (def.domain && def.application && def.module) ? def : null
}

/* ─────────────── 字典清单 ─────────────── */
async function loadDictList (def) {
  const data = await apiGet(
    `/api/definitions/list?kind=DCT&domain=${encodeURIComponent(def.domain)}&application=${encodeURIComponent(def.application)}&module=${encodeURIComponent(def.module)}`,
    def.dbId,
  )
  const items = (data && data.items) || []
  if (!items.length) return []
  const def_ = items.find((it) => it.isDefault) || items.slice().sort((a, b) => (Number(b.version) || 0) - (Number(a.version) || 0))[0]
  return (def_.dictionaries || []).map((d) => ({ dictCode: d.dictCode, dictName: d.dictName }))
}

async function loadMeta (def, dictCode) {
  return apiGet(`/api/dct/meta?${qs(def, { dict: dictCode })}`, def.dbId)
}

/* ─────────────── 列模型（含 edit.mode 行内编辑配置） ─────────────── */
function buildColumnModel (meta) {
  const C = cmx()
  if (!C.CmxColumnModel || !C.CmxColumn) return null
  const pk = meta.pk
  const pkGenerated = isPkGenerated(meta)
  const members = (meta.columns || [])
    .filter((c) => showInTable(c, meta))
    .map((c) => {
      const editable = isEditable(c, meta)
      const colOpts = {
        id: c.name,
        caption: colCaption(c),
        dataType: c.dataType,
        width: defaultWidthFor(c),
      }
      // 应用元数据的 display 配置（align/decimalDigits/format）
      const disp = displayFor(c)
      if (disp) colOpts.display = disp
      // 引用字典列：挂 refDict/displayField/refField 供 grid 回显（code → name）
      if (c.refDict) {
        colOpts.refDict = c.refDict
        colOpts.refField = c.refField || 'code'
        colOpts.displayField = c.displayField || 'name'
      }

      if (editable) {
        const em = editModeFor(c, meta)
        colOpts.edit = { mode: em.mode, trigger: 'click' }
        if (em.options) colOpts.edit.options = em.options
        // 字典选择列（cmx-dict-selct）需要 editSettings 传字典坐标（cmx-dict-select 弹窗用）
        if (em.mode === 'cmx-dict-selct') {
          colOpts.editSettings = {
            dictCode: em.dictCode,
            idCol: em.idField,
            labelCol: em.labelField,
            hierarchical: !!em.hierarchical,
            // 字典坐标：cmx-dict-select 拼 /api/dct/data/search URL 的必需来源
            // （运行时 host 无坐标，组件唯一取数来源是 editSettings.coord）
            coord: {
              domain: meta.domain || (state.def && state.def.domain) || '',
              application: meta.application || (state.def && state.def.application) || '',
              module: meta.module || (state.def && state.def.module) || '',
              ...(state.def && state.def.dbId ? { dbId: state.def.dbId } : {}),
            },
          }
          if (em.parentField) colOpts.editSettings.parentCol = em.parentField
        }
        // 必填校验（元数据 nullable=false）
        if (c.nullable === false) colOpts.edit.required = true
        // code 主键（业务键）：新增行可填，保存后只读
        // 用 grid 内部 id 字段的 't' 前缀判断新增态；readonlyWhen 是 formula-eval 表达式
        if (!pkGenerated && c.name === pk) {
          colOpts.edit.readonlyWhen = `NOT(STARTSWITH(id, 't'))`
          colOpts.edit.required = true
        }
      } else {
        // 不可编辑列：保留元数据的 edit.mode（如 checkbox 显示复选框样式），否则 readonly
        const metaMode = c.edit && c.edit.mode ? String(c.edit.mode).toLowerCase() : ''
        colOpts.edit = (metaMode === 'checkbox') ? { mode: 'checkbox' } : { mode: 'readonly' }
      }
      return new C.CmxColumn(colOpts)
    })
  return new C.CmxColumnModel({ members })
}

/* ─────────────── 数据装载 ─────────────── */
async function loadData (def, dictCode, meta) {
  const body = {
    q: state.q || undefined,
    page: state.page,
    pageSize: state.pageSize,
    filters: buildFiltersFromConds(meta),
  }
  if (meta.selfHierarchy) body.parentId = state.currentParentId
  return apiPost(`/api/dct/data/search?${qs(def, { dict: dictCode })}`, body, def.dbId)
}

function buildFiltersFromConds (meta) {
  const f = {}
  const validCols = new Set((meta.columns || []).map((c) => c.name))
  for (const c of state.conds) {
    if (!c.col || !validCols.has(c.col)) continue
    const v = String(c.value || '').trim()
    if (!v) continue
    f[c.col] = /^\-?\d+(\.\d+)?$/.test(v) ? Number(v) : v
  }
  return Object.keys(f).length ? f : undefined
}

/* ─────────────── grid 重建 + 数据填充（切换字典时重建 grid 绕过列缓存） ─────────────── */
async function applyRowsToGrid (root, rows) {
  const wrap = root.querySelector('.de-grid-wrap')
  if (!wrap) return

  // 列模型变化 → 重建 grid（含首次）
  if (state._lastDictCode !== state.dictCode) {
    const oldGrid = wrap.querySelector('cmx-revo-grid')
    if (oldGrid) oldGrid.remove()
    const newGrid = document.createElement('cmx-revo-grid')
    newGrid.className = 'de-grid'
    newGrid.id = 'deGrid'
    wrap.appendChild(newGrid)
    state.grid = newGrid
    const cm = buildColumnModel(state.meta)
    if (cm) newGrid.setColumnModel(cm)
    // 行内编辑：editable 总开关 + 单击触发；关闭合计行（字典维护不需要汇总）
    newGrid.setOptions && newGrid.setOptions({
      selectionMode: 'multi',
      fillHeight: true,
      showRowIndex: true,
      editable: true,
      editTrigger: 'click',
      showTotals: false,
    })
    wireGridEvents(newGrid, root)
    state._lastDictCode = state.dictCode
    // 等 Stencil 内部首次渲染完成
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
  }

  const grid = state.grid
  if (!grid) return

  const C = cmx()
  const pk = state.meta.pk
  const normRows = (rows || []).map((r) => ({ ...r, id: r[pk] }))
  if (C.CmxDataSet) {
    const ds = new C.CmxDataSet({})
    ds.setRows(normRows)
    grid.setDataSet(ds)
  } else if (grid.setDataSet) {
    grid.setDataSet(normRows)
  }
  // 引用字典列回显：让 code/id 自动显示为字典名称（country_code → 国家名）
  if (typeof grid.enableDictEcho === 'function') {
    try {
      await grid.enableDictEcho({ coord: state.def, dbId: state.def.dbId }, undefined)
    } catch (_) { /* 回显失败不阻断 */ }
  }
  try { grid.refreshLayout && grid.refreshLayout() } catch (_) {}
}

/** 绑定 grid 事件：行选中、单元格编辑变更收集。 */
function wireGridEvents (grid, root) {
  grid.addEventListener('cmx-row-selected', (ev) => {
    state.selectedRowId = ev.detail && ev.detail.id
  })
  grid.addEventListener('cmx-row-selection-change', (ev) => {
    state.selectedIds = (ev.detail && ev.detail.ids) || []
  })
  // 单元格编辑完成 → 收集到 dirtyMap（新增行不记，save 时全量提交）
  grid.addEventListener('cmx-cell-changed', (ev) => {
    const d = ev.detail || {}
    const id = d.id
    const key = d.key
    const value = d.value
    if (id == null || key == null) return
    if (state.newIds.has(String(id))) return  // 新增行整体提交，不单记
    if (!state.dirtyMap[id]) {
      const baseline = state.baselineMap[String(id)]
      state.dirtyMap[id] = { fields: {}, ...(baseline != null ? { baseline } : {}) }
    }
    state.dirtyMap[id].fields[key] = value
  })
}

/* ─────────────── 分页信息渲染 ─────────────── */
function renderPageInfo (root) {
  const pager = root.querySelector('#dePager')
  if (!pager) return
  // 同步 total 到 cmx-pager
  pager.total = state.total
  pager.page = state.page
  pager.pageSize = state.pageSize
  const dirty = Object.keys(state.dirtyMap).length + state.newIds.size + state.deletedIds.length
  // cmx-pager 自带显示，这里只更新副标题的 dirty 提示
  const sub = root.querySelector('#deSubtitle')
  if (sub) {
    const m = state.meta
    const base = `${m ? (m.dictName || '') + '（' + m.dictCode + '）' : ''}${m && m.selfHierarchy ? ' · 树形' : ' · 平级'}`
    sub.textContent = dirty ? `${base} · 未保存 ${dirty} 项` : base
  }
}

/* ─────────────── 重新装载 ─────────────── */
async function reload (root) {
  if (!state.meta) return
  setMsg(root, '装载中…')
  try {
    const data = await loadData(state.def, state.dictCode, state.meta)
    state.rows = (data && data.rows) || []
    state.total = (data && Number(data.total)) || 0
    // 记录 update_time 作为乐观锁 baseline
    const utCol = (state.meta.columns || []).find((c) => c.name === 'update_time')
    if (utCol) {
      const pk = state.meta.pk
      for (const r of state.rows) {
        if (r[pk] != null && r.update_time != null) state.baselineMap[String(r[pk])] = r.update_time
      }
    }
    await applyRowsToGrid(root, state.rows)
    renderPageInfo(root)
    setMsg(root, `已装载 ${state.rows.length} 条`, 'ok')
  } catch (e) {
    setMsg(root, `装载失败：${e.message}`, 'err')
  }
}

/* ─────────────── 新增行（grid.addRow，不进编辑态，用户单击单元格编辑） ─────────────── */
function addRow (root) {
  const meta = state.meta
  const grid = state.grid
  if (!meta || !grid || !grid.addRow) return
  const pk = meta.pk
  const pkGenerated = isPkGenerated(meta)
  // 临时行标识（grid 内部用 id 字段做行标识，cmx-revo-grid 要求每行有 id）
  const tempId = `t${Date.now()}${Math.floor(Math.random() * 1000)}`
  const newRow = { id: tempId }
  if (pkGenerated) {
    // id 主键（整数，后端铸号）：pk 字段填临时值，保存时后端替换
    newRow[pk] = tempId
  } else {
    // code 主键（字符串业务键）：pk 字段（code）留空让用户填，readonlyWhen 用 id 的 't' 前缀判断新增态
    newRow[pk] = ''
  }
  // 可编辑列给默认值：未填字段用 null（后端 build_upsert_sql 对 null 用 SQL NULL 字面量，
  // 正确处理；空字符串 "" 会被当真实值插入，对 INT/DATE 等类型报错）。
  // 仅 status=1（启用）、sort_no=0 给业务默认值。
  const editableCols = (meta.columns || []).filter((c) => isEditable(c, meta))
  for (const c of editableCols) {
    if (c.name === pk) continue  // pk 已处理
    if (c.name === 'status') newRow.status = 1
    else if (c.name === 'sort_no') newRow.sort_no = 0
    else newRow[c.name] = null
  }
  if (meta.selfHierarchy && meta.parentField) {
    newRow[meta.parentField] = state.currentParentId
  }
  state.newIds.add(tempId)
  grid.addRow(newRow)
  setMsg(root, '已新增空行，单击单元格编辑', 'ok')
  renderPageInfo(root)
}

/* ─────────────── 删除选中行 ─────────────── */
async function deleteSelected (root) {
  const meta = state.meta
  const grid = state.grid
  if (!meta || !grid) return
  const pk = meta.pk
  const ids = (grid.getSelectedIds ? grid.getSelectedIds() : []) || []
  const idSet = new Set(ids.map((x) => String(x)))
  if (!idSet.size) {
    cmxNotify('info', '请先在表格中选择要删除的行')
    return
  }
  // 系统预置项拦截
  for (const r of state.rows) {
    if (idSet.has(String(r[pk])) && Number(r.is_system) === 1) {
      const lbl = meta.labelField ? (r[meta.labelField] || '') : ''
      cmxNotify('warn', `「${lbl || r[pk]}」为系统预置项，不可删除`)
      return
    }
  }
  const ok = await cmxConfirm(`确认删除选中的 ${idSet.size} 项？`, '删除确认')
  if (!ok) return
  for (const id of idSet) {
    // 新增行直接从 newIds 移除
    if (state.newIds.has(id)) {
      state.newIds.delete(id)
    } else {
      state.deletedIds.push(id)
      // 移除 dirtyMap 中对应记录
      delete state.dirtyMap[id]
    }
  }
  // grid 内删除（视觉即时反馈）
  if (grid.removeRows) grid.removeRows(ids)
  setMsg(root, '已暂存删除，点击「保存」提交', 'ok')
  renderPageInfo(root)
}

/* ─────────────── commitGridEdits：收拢未提交的行内编辑（仿 dictflat-content.html） ─────────────── */
function commitGridEdits (cb) {
  try {
    const deepActive = (r) => {
      const a = r && r.activeElement
      if (a && a.shadowRoot && a.shadowRoot.activeElement) return deepActive(a.shadowRoot)
      return a
    }
    const ae = deepActive(document)
    if (ae && ae !== document.body) {
      try { ae.dispatchEvent(new Event('change', { bubbles: true })) } catch (_) {}
      if (typeof ae.blur === 'function') { try { ae.blur() } catch (_) {} }
    }
  } catch (_) {}
  requestAnimationFrame(() => requestAnimationFrame(() => { try { cb() } catch (_) {} }))
}

/* ─────────────── 保存（changeset） ─────────────── */
async function save (root) {
  const meta = state.meta
  const def = state.def
  if (!meta) return
  setMsg(root, '保存中…')
  commitGridEdits(async () => {
    const pk = meta.pk
    const grid = state.grid
    // 收拢 grid 当前所有行（含未触发 cmx-cell-changed 的尾随编辑）
    const allRows = grid && grid.getSource ? grid.getSource() : state.rows
    // 新增行：从 allRows 取本次会话新增的完整行
    // 用 meta.columns 白名单过滤，剔除 CmxDataSet 注入的内部字段（_children/_ds/__cmxRowClass 等）
    const validCols = new Set((meta.columns || []).map((c) => c.name))
    const inserted = []
    for (const id of state.newIds) {
      const r = allRows.find((x) => String(x[pk]) === String(id) || String(x.id) === String(id))
      if (r) {
        const fields = {}
        for (const k of Object.keys(r)) {
          if (validCols.has(k)) fields[k] = r[k]
        }
        inserted.push({ id, fields })
      }
    }
    // 修改行
    const updated = []
    for (const id of Object.keys(state.dirtyMap)) {
      const rec = state.dirtyMap[id]
      updated.push({ id, fields: { ...rec.fields }, ...(rec.baseline != null ? { baseline: rec.baseline } : {}) })
    }
    // 删除行
    const deleted = state.deletedIds.slice()

    const dirty = inserted.length + updated.length + deleted.length
    if (!dirty) { cmxNotify('info', '无变更可保存'); setMsg(root, ''); return }

    const payload = {
      saveMode: 'merge',
      changes: { [meta.tableName]: { inserted, updated, deleted } },
    }
    try {
      const r = await apiPost(`/api/dct/save?${qs(def, { dict: state.dictCode })}`, payload, def.dbId)
      const aff = (r && r.affected) || 0
      cmxNotify('ok', `保存成功：影响 ${aff} 行（新增 ${inserted.length} / 更新 ${updated.length} / 删除 ${deleted.length}）`)
      if (r && Array.isArray(r.updatedAt)) {
        for (const u of r.updatedAt) {
          if (u.id != null && u.updateTime != null) state.baselineMap[String(u.id)] = u.updateTime
        }
      }
      // 清空本地变更状态
      state.dirtyMap = {}
      state.newIds = new Set()
      state.deletedIds = []
      state.baselineMap = {}
      await reload(root)
    } catch (e) {
      if (e.status === 409) {
        cmxNotify('error', '字典项已被他人修改，已自动刷新到最新版本')
        state.dirtyMap = {}
        state.newIds = new Set()
        state.deletedIds = []
        await reload(root)
      } else if (e.status === 422 && e.body && e.body.data && Array.isArray(e.body.data.violations)) {
        presentViolations(e.body.data.violations)
        setMsg(root, `保存失败：${e.message}`, 'err')
      } else {
        cmxNotify('error', `保存失败：${e.message}`)
        setMsg(root, `保存失败：${e.message}`, 'err')
      }
    }
  })
}

function presentViolations (violations) {
  const lines = (violations || []).slice(0, 50).map((v) => {
    const where = [v.row || v.id, v.field, v.message].filter(Boolean).map((x) => String(x)).join(' · ')
    return `• ${where || JSON.stringify(v)}`
  })
  cmxNotify('error', `校验错误（${violations.length} 处）：\n${lines.join('\n')}`)
}

/* ─────────────── 树形：左侧树 ─────────────── */
async function loadTreeChildren (root, parentId) {
  const data = await apiPost(
    `/api/dct/data/search?${qs(state.def, { dict: state.dictCode })}`,
    { parentId, page: 1, pageSize: 500 },
    state.def.dbId,
  )
  const rows = (data && data.rows) || []
  state.treeNodes[String(parentId)] = rows
  return rows
}

function nodeLabel (row) {
  const meta = state.meta
  if (!meta) return String(row.id || '')
  const lbl = meta.labelField ? row[meta.labelField] : ''
  const code = meta.codeField ? row[meta.codeField] : ''
  return lbl || code || row[meta.pk] || ''
}

function renderTree (root) {
  const body = root.querySelector('#deTreeBody')
  if (!body) return
  const meta = state.meta
  const pk = meta.pk
  const rootChildren = state.treeNodes['null'] || state.treeNodes[null] || []
  const isRootActive = state.currentParentId == null
  body.innerHTML = `
    <div class="de-tree-root ${isRootActive ? 'active' : ''}" data-node-id="__root__">▶ 全部根节点（${rootChildren.length}）</div>
    <div class="de-children" id="deTreeChildren">
      ${renderTreeNodes(rootChildren, pk)}
    </div>
  `
  body.querySelector('[data-node-id="__root__"]').addEventListener('click', async () => {
    state.currentParentId = null
    state.selectedTreeNodeId = null
    state.page = 1
    renderTree(root)
    await reload(root)
  })
  wireTreeNodeClick(root, body)
}

function renderTreeNodes (nodes, pk) {
  if (!nodes || !nodes.length) return ''
  return nodes.map((n) => {
    const id = n[pk]
    const label = nodeLabel(n)
    const isActive = String(state.selectedTreeNodeId) === String(id)
    const code = state.meta.codeField ? n[state.meta.codeField] : ''
    const meta = code !== '' ? ` <span class="de-tree-meta">${escHtml(String(code))}</span>` : ''
    return `<div>
      <div class="de-tree-node ${isActive ? 'active' : ''}" data-node-id="${escAttr(String(id))}">
        <span class="de-tree-toggle" data-toggle="${escAttr(String(id))}">▸</span>
        <span class="de-tree-label">${escHtml(label)}${meta}</span>
      </div>
      <div class="de-children" data-children-of="${escAttr(String(id))}" style="display:none"></div>
    </div>`
  }).join('')
}

function wireTreeNodeClick (root, body) {
  body.querySelectorAll('.de-tree-node[data-node-id]').forEach((el) => {
    el.addEventListener('click', async (ev) => {
      if (ev.target && ev.target.dataset && ev.target.dataset.toggle) return
      const id = el.dataset.nodeId
      state.selectedTreeNodeId = id
      state.currentParentId = id
      state.page = 1
      renderTree(root)
      await reload(root)
    })
  })
  body.querySelectorAll('[data-toggle]').forEach((tg) => {
    tg.addEventListener('click', async (ev) => {
      ev.stopPropagation()
      const id = tg.dataset.toggle
      const childBox = body.querySelector(`[data-children-of="${CSS.escape(id)}"]`)
      if (!childBox) return
      const expanded = tg.textContent === '▾'
      if (expanded) {
        childBox.style.display = 'none'
        tg.textContent = '▸'
        return
      }
      tg.textContent = '▾'
      if (!state.treeNodes[String(id)]) {
        try { await loadTreeChildren(root, id) } catch (e) { /* 忽略 */ }
      }
      const children = state.treeNodes[String(id)] || []
      childBox.innerHTML = renderTreeNodes(children, state.meta.pk)
      childBox.style.display = 'block'
      wireTreeNodeClick(root, childBox)
    })
  })
}

/* ─────────────── 切换字典 ─────────────── */
async function switchDict (root, dictCode) {
  state.dictCode = dictCode
  state.meta = null
  state.page = 1
  state.q = ''
  state.conds = []
  state.currentParentId = null
  state.selectedTreeNodeId = null
  state.treeNodes = {}
  state.dirtyMap = {}
  state.newIds = new Set()
  state.deletedIds = []
  state.baselineMap = {}
  setMsg(root, `加载字典元数据：${dictCode} …`)
  try {
    state.meta = await loadMeta(state.def, dictCode)
  } catch (e) {
    setMsg(root, `元数据加载失败：${e.message}`, 'err')
    return
  }
  // 重建布局
  const body = root.querySelector('#deBody')
  if (state.meta.selfHierarchy) {
    body.classList.remove('flat'); body.classList.add('tree')
    if (!body.querySelector('.de-tree')) {
      const tree = document.createElement('div')
      tree.className = 'de-tree'
      tree.innerHTML = `<div class="de-tree-head">字典结构</div><div class="de-tree-body" id="deTreeBody"></div>`
      body.insertBefore(tree, body.firstChild)
    }
    try { await loadTreeChildren(root, null) } catch (e) { /* 忽略 */ }
    renderTree(root)
  } else {
    body.classList.remove('tree'); body.classList.add('flat')
    const tree = body.querySelector('.de-tree')
    if (tree) tree.remove()
  }
  // 强制 grid 重建（新字典新列模型）
  state._lastDictCode = null
  // 同步字典切换器 selected 状态
  const sel = root.querySelector('#deDictSelect')
  if (sel) {
    sel.querySelectorAll('ui5-option').forEach((o) => {
      if (o.getAttribute('value') === dictCode) o.setAttribute('selected', '')
      else o.removeAttribute('selected')
    })
  }
  await reload(root)
}

/* ─────────────── 初始化 dictSelect ─────────────── */
function initDictSelect (root) {
  const sel = root.querySelector('#deDictSelect')
  if (!sel) return
  sel.innerHTML = state.dicts.map((d) => {
    const selAttr = d.dictCode === state.dictCode ? 'selected' : ''
    return `<ui5-option value="${escAttr(d.dictCode)}" ${selAttr}>${escHtml(d.dictName || d.dictCode)}（${escHtml(d.dictCode)}）</ui5-option>`
  }).join('')
  sel.addEventListener('change', (ev) => {
    const opt = ev.detail && ev.detail.selectedOption
    const code = opt ? opt.getAttribute('value') : ''
    if (code && code !== state.dictCode) void switchDict(root, code)
  })
}

/* ─────────────── 绑定页面事件 ─────────────── */
function bindPage (root) {
  root.querySelector('#btnReload')?.addEventListener('click', () => { state.page = 1; void reload(root) })
  root.querySelector('#btnAdd')?.addEventListener('click', () => addRow(root))
  root.querySelector('#btnDel')?.addEventListener('click', () => void deleteSelected(root))
  root.querySelector('#btnSave')?.addEventListener('click', () => void save(root))

  // 搜索
  const search = () => {
    state.q = (root.querySelector('#deQ')?.value || '').trim()
    state.page = 1
    void reload(root)
  }
  root.querySelector('#btnSearch')?.addEventListener('click', search)
  root.querySelector('#deQ')?.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') search() })
  root.querySelector('#btnClearCond')?.addEventListener('click', () => {
    if (root.querySelector('#deQ')) root.querySelector('#deQ').value = ''
    state.q = ''; state.page = 1
    void reload(root)
  })

  // cmx-pager 翻页
  const pager = root.querySelector('#dePager')
  if (pager) {
    pager.addEventListener('page-change', (ev) => {
      const d = (ev && ev.detail) || {}
      if (d.pageSize && d.pageSize !== state.pageSize) {
        state.pageSize = d.pageSize
        state.page = 1
      } else if (d.page) {
        state.page = d.page
      }
      void reload(root)
    })
  }

  // 字典切换器
  initDictSelect(root)

  // 启动：选初始字典
  const initial = state.def.initialDictCode || (state.dicts[0] && state.dicts[0].dictCode) || ''
  if (initial) void switchDict(root, initial)
  else setMsg(root, '该坐标下未找到 DCT 字典定义', 'err')
}

/* ─────────────── export ─────────────── */
export default {
  defaultView: 'content',
  views: {
    async content (ctx) {
      const host = ctx && ctx.host
      // 每次进入重置实例状态
      state.def = null
      state.dicts = []
      state.meta = null
      state.dictCode = ''
      state.page = 1
      state.pageSize = 50
      state.q = ''
      state.conds = []
      state.treeNodes = {}
      state.currentParentId = null
      state.selectedTreeNodeId = null
      state.rows = []
      state.total = 0
      state.grid = null
      state._lastDictCode = null
      state.selectedRowId = null
      state.selectedIds = []
      state.dirtyMap = {}
      state.newIds = new Set()
      state.deletedIds = []
      state.baselineMap = {}

      const def = readDef(ctx)
      if (!def) {
        return `<div style="padding:12px;color:var(--sapNegativeColor,#bb0000);font-size:13px">
          通用字典维护页缺少必要 props：需 { domain, application, module }（可选 dictCode/dbId）。
        </div>`
      }
      state.def = def

      try {
        state.dicts = await loadDictList(def)
      } catch (e) {
        state.dicts = def.initialDictCode ? [{ dictCode: def.initialDictCode, dictName: def.initialDictCode }] : []
      }

      if (host) whenRendered(host, '.de-root', (root) => bindPage(root))
      return pageHtml()
    },
  },
}
