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

/** 判断字段是否为主键（兼容 isPrimaryKey:1/true 与 meta.pk 两种标记）。
 *  元数据字段可能用 isPrimaryKey（0/1 或 boolean）显式标记，也可能仅由 meta.pk 声明。 */
function isPrimaryKeyField (col, meta) {
  if (Number(col.isPrimaryKey) === 1 || col.isPrimaryKey === true) return true
  return !!meta.pk && col.name === meta.pk
}

/** 业务键（新增可填、保存后只读）：
 *  ① 字符串物理主键（isPrimaryKey:1 且 dataType 为 VARCHAR/CHAR/TEXT）；
 *  ② 字典业务编码字段（meta.codeField 指向且 dataType 为 VARCHAR/CHAR/TEXT）。
 *  codeField 虽非物理主键，但作为业务编码（通常唯一、有外键引用），修改会破坏一致性，
 *  故与字符串主键同等对待。整数物理主键（id，后端铸号）非业务键，前端不可编辑。 */
function isBusinessKey (col, meta) {
  const t = String(col.dataType || '').toUpperCase()
  const isString = t.includes('CHAR') || t.includes('TEXT') || t === 'STRING'
  if (!isString) return false
  // 字符串物理主键（isPrimaryKey 标记 或 meta.pk 声明）
  if (isPrimaryKeyField(col, meta)) return true
  // 字典业务编码字段（codeField）
  if (!!meta.codeField && col.name === meta.codeField) return true
  return false
}

/** 必填列判定（列头标识与保存校验共用，与 buildColumnModel 一致）：
 *  元数据 edit.required / 顶层 required 优先；其次 nullable=false 推断；业务键强制必填。 */
function isRequiredCol (c, meta) {
  if (isBusinessKey(c, meta)) return true
  const metaEdit = c.edit && typeof c.edit === 'object' ? c.edit : null
  if (metaEdit && metaEdit.required === true) return true
  if (c.required === true) return true
  if (c.nullable === false) return true
  return false
}

/** 必填校验用的空值判定：null/undefined/空串/纯空白 视为空。 */
function isEmptyValue (v) {
  return v == null || (typeof v === 'string' && v.trim() === '')
}

function showInTable (col, meta) {
  if (DERIVED_HIERARCHY.has(col.name)) return false
  // 元数据声明 visible:false → 列表隐藏（如 base 定义 id 列 visible:false）。
  // 规范（field-edit-display-modes §四 flcLayout）：visible 是字段固有属性，应被尊重。
  if (col.visible === false) return false
  return true
}

/** 行内是否可编辑：
 *  - 整数物理主键（id，后端铸号）→ 不可编辑
 *  - 业务键（字符串主键 / codeField）→ 可编辑（由 readonlyWhen 限定：仅新增行可填，已存在行只读）
 *  - 审计/系统标识/派生层级 → 不可编辑 */
function isEditable (col, meta) {
  // 整数物理主键（后端铸号）：不可编辑
  if (isPrimaryKeyField(col, meta) && !isBusinessKey(col, meta)) return false
  // 业务键（字符串主键 / codeField）：可编辑，由 readonlyWhen 限制存量行只读
  if (isBusinessKey(col, meta)) return true
  if (AUDIT_FIELDS.has(col.name)) return false
  if (SYSTEM_FLAG_FIELDS.has(col.name)) return false
  if (DERIVED_HIERARCHY.has(col.name)) return false
  return true
}

function colCaption (col) {
  if (col.caption && typeof col.caption === 'object') return col.caption.zh_CN || col.caption.en || col.name
  return col.caption || col.name
}

/* 列模型构建已下移到 cmx-data-comp 的 metaTableFieldsToColumns（init-page-models.js）。
   以下仅保留页面级逻辑所需的字段角色判定函数（save/addRow 使用）。 */

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
.de-tree-leaf{width:14px;display:inline-block;color:var(--neo-mint,#10b981);font-size:8px;text-align:center;line-height:14px;opacity:.7}
.de-tree-label{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.de-tree-meta{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-tree-root{font-size:13px;padding:5px 8px;cursor:pointer;border-radius:4px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-tree-root:hover{background:color-mix(in srgb,var(--neo-cyan) 10%,var(--sapList_Background,#fff))}
.de-tree-root.active{color:var(--neo-cyan);font-weight:600;background:color-mix(in srgb,var(--neo-cyan) 14%,var(--sapList_Background,#fff))}
/* "全部"虚拟节点：与 .de-tree-root 对齐视觉但用 --neo-mint 强调（区分"全量"与"根级"语义）。
   选中态用更明显的背景 + 左侧 3px 强调条 + 阴影，让用户能直接看出"我在全部模式下"。 */
.de-tree-virtual{font-size:13px;padding:5px 8px;cursor:pointer;border-radius:4px;
  color:var(--sapContent_LabelColor,#6a6d70);display:flex;align-items:center;gap:4px;
  position:relative;border:1px solid transparent}
.de-tree-virtual:hover{background:color-mix(in srgb,var(--neo-mint) 10%,var(--sapList_Background,#fff))}
.de-tree-virtual.active{color:var(--neo-mint,#10b981);font-weight:700;
  background:color-mix(in srgb,var(--neo-mint) 16%,var(--sapList_Background,#fff));
  border-color:color-mix(in srgb,var(--neo-mint) 35%,transparent);
  box-shadow:inset 3px 0 0 var(--neo-mint,#10b981)}
.de-tree-count{margin-left:auto;font-size:11px;color:var(--sapContent_LabelColor,#6a6d70);
  background:color-mix(in srgb,var(--neo-mint) 12%,transparent);padding:1px 6px;border-radius:8px}
.de-tree-virtual.active .de-tree-count{color:var(--neo-mint,#10b981);
  background:color-mix(in srgb,var(--neo-mint) 22%,transparent);font-weight:600}
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
    <ui5-button design="Default" icon="download" id="btnExport">导出</ui5-button>
    <ui5-button design="Default" icon="upload" id="btnImport">导入</ui5-button>
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
  // with_props=true：让后端把字段扁平属性（width/visible/pattern/enumValues/required/
  // intDigits/decimalDigits 等）一并下发，供 buildColumnModel 严格按 field-edit-display-modes
  // 规范构建列模型（列宽/隐藏/校验正则/枚举下拉/必填）。
  return apiGet(`/api/dct/meta?${qs(def, { dict: dictCode, with_props: 'true' })}`, def.dbId)
}

/* ─────────────── 列模型（委托 cmx-data-comp metaTableFieldsToColumns 增强路径） ─────────────── */
function buildColumnModel (meta) {
  const C = cmx()
  if (!C.CmxColumnModel || !C.metaTableFieldsToColumns) return null
  const cols = C.metaTableFieldsToColumns(meta.columns || [], {
    kind: 'DCT',
    pk: meta.pk,
    codeField: meta.codeField,
    selfHierarchy: meta.selfHierarchy,
    parentField: meta.parentField,
    dictCode: meta.dictCode || state.dictCode,
    labelField: meta.labelField,
    domain: meta.domain || (state.def && state.def.domain) || '',
    application: meta.application || (state.def && state.def.application) || '',
    module: meta.module || (state.def && state.def.module) || '',
  }, {
    respectOrder: false,
    coord: {
      domain: (state.def && state.def.domain) || '',
      application: (state.def && state.def.application) || '',
      module: (state.def && state.def.module) || '',
      ...(state.def && state.def.dbId ? { dbId: state.def.dbId } : {}),
    },
  })
  const cm = new C.CmxColumnModel({ datasetId: 'dict' })
  cm.setMembers(cols)
  return cm
}

/* ─────────────── 数据装载 ─────────────── */
async function loadData (def, dictCode, meta) {
  const body = {
    q: state.q || undefined,
    page: state.page,
    pageSize: state.pageSize,
    filters: buildFiltersFromConds(meta),
  }
  if (meta.selfHierarchy) {
    /* "全部"虚拟节点：state.currentParentId=undefined → body 不带 parentId 键 → 后端全量。
       "全部根节点"：state.currentParentId=null → body.parentId=null → 后端 IS NULL → 根级。
       具体节点：state.currentParentId=<id> → body.parentId=<id> → 后端等值匹配 → 直接子级。 */
    if (state.currentParentId !== undefined) body.parentId = state.currentParentId
  }
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
    // showRequiredMark: 必填列头显示红色 * 标识（编辑页开启，只读页默认关闭）
    newGrid.setOptions && newGrid.setOptions({
      selectionMode: 'multi',
      fillHeight: true,
      showRowIndex: true,
      editable: true,
      editTrigger: 'click',
      showTotals: false,
      showRequiredMark: true,
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
      //fixme 0727 性能问题，注释字典回显
      // await grid.enableDictEcho({ coord: state.def, dbId: state.def.dbId }, undefined)
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
  // 临时行标识（grid 内部用 id 字段做行标识，cmx-revo-grid 要求每行有 id）
  const tempId = `t${Date.now()}${Math.floor(Math.random() * 1000)}`
  const newRow = { id: tempId }
  // 主键字段：整数主键（后端铸号）填临时 id（保存时后端替换）；
  // 字符串主键（业务键）留空让用户填，readonlyWhen 用 id 的 't' 前缀判断新增态
  for (const c of (meta.columns || [])) {
    if (!isPrimaryKeyField(c, meta)) continue
    newRow[c.name] = isBusinessKey(c, meta) ? '' : tempId
  }
  // 可编辑非主键列给默认值：未填字段用 null（后端 build_upsert_sql 对 null 用 SQL NULL 字面量，
  // 正确处理；空字符串 "" 会被当真实值插入，对 INT/DATE 等类型报错）。
  // 业务键（如 codeField，非物理主键）留空待填；status=1（启用）、sort_no=0 给业务默认值。
  const editableCols = (meta.columns || []).filter((c) => isEditable(c, meta) && !isPrimaryKeyField(c, meta))
  for (const c of editableCols) {
    if (isBusinessKey(c, meta)) { newRow[c.name] = ''; continue }
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
  // 区分新增行（未入库，仅本地移除）与已入库行（调接口删除）
  const realIds = []
  for (const id of idSet) {
    if (state.newIds.has(id)) {
      state.newIds.delete(id)
    } else {
      realIds.push(id)
      delete state.dirtyMap[id]  // 清理该行的未保存修改
    }
  }
  // grid 内即时移除（视觉反馈）
  if (grid.removeRows) grid.removeRows(ids)
  // 仅删除未入库的新增行：无需调接口
  if (!realIds.length) {
    cmxNotify('ok', `已移除 ${ids.length} 项未保存的新增行`)
    setMsg(root, `已移除 ${ids.length} 项`, 'ok')
    renderPageInfo(root)
    return
  }
  const def = state.def
  if (!def) { cmxNotify('error', '缺少字典坐标，无法删除'); return }
  // 直接调用接口删除（即时生效，不再暂存等"保存"按钮）
  setMsg(root, '删除中…')
  try {
    const payload = {
      saveMode: 'merge',
      changes: { [meta.tableName]: { inserted: [], updated: [], deleted: realIds } },
    }
    const r = await apiPost(`/api/dct/save?${qs(def, { dict: state.dictCode })}`, payload, def.dbId)
    const aff = (r && r.affected) || 0
    cmxNotify('ok', `已删除 ${realIds.length} 项（影响 ${aff} 行）`)
    setMsg(root, `已删除 ${realIds.length} 项`, 'ok')
    await reload(root)
    // 树形字典：删除节点后左侧结构也要刷新（保持展开态）
    await refreshTree(root)
  } catch (e) {
    cmxNotify('error', `删除失败：${e.message}`)
    setMsg(root, `删除失败：${e.message}`, 'err')
    // 失败重载恢复：grid.removeRows 已移除显示，需从服务端拉回
    await reload(root)
    await refreshTree(root)
  }
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

    // 保存前必填校验：检查 inserted（全字段）+ updated（仅 fields 里出现的字段）。
    // 后端只认 nullable，不认 edit.required（如 nullable=true 但 required=true 的列后端不兜底），
    // 故前端必须自校验。判定与列头标识、buildColumnModel 共用 isRequiredCol。
    const requiredCols = (meta.columns || [])
      .filter((c) => showInTable(c, meta) && isEditable(c, meta) && isRequiredCol(c, meta))
    const violations = []
    for (const ins of inserted) {
      for (const c of requiredCols) {
        if (isEmptyValue(ins.fields[c.name])) {
          violations.push({ id: ins.id, field: c.name, message: `${colCaption(c)} 为必填项` })
        }
      }
    }
    for (const upd of updated) {
      // 仅校验 fields 里出现的必填字段（用户没改的必填字段即使历史为空也不拦，避免"我没改这行却报错"）
      for (const c of requiredCols) {
        if (c.name in upd.fields && isEmptyValue(upd.fields[c.name])) {
          violations.push({ id: upd.id, field: c.name, message: `${colCaption(c)} 为必填项` })
        }
      }
    }
    if (violations.length) {
      presentViolations(violations)
      setMsg(root, `保存失败：${violations.length} 处必填项为空`, 'err')
      return
    }

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
      // 新增/删除/修改入库后，树形字典的左侧结构也要刷新（保持展开态）
      await refreshTree(root)
    } catch (e) {
      if (e.status === 409) {
        cmxNotify('error', '字典项已被他人修改，已自动刷新到最新版本')
        state.dirtyMap = {}
        state.newIds = new Set()
        state.deletedIds = []
        await reload(root)
        await refreshTree(root)
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

/* ─────────────── 导出（fetch + Blob 下载，支持 JWT header） ───────────────
 * 项目用 JWT header 鉴权（cmx-api mw_auth），iframe/`<a download>` 无法携带 Authorization，
 * 必须用 fetch。浏览器 Blob > 1MB 自动落盘（file-backed），100w 行规模 JS heap 占用极低。
 * token 由前端框架统一注入（native-pages host 拦截 fetch），data-editor.js 直接 fetch 即可。
 */

async function onExportClick (root) {
  const fmt = await pickExportFormat(root)
  if (!fmt) return
  await doExport(root, fmt)
}

/** 弹出格式选择对话框：返回 'json' / 'csv' / null（取消）。 */
function pickExportFormat (_root) {
  return new Promise((resolve) => {
    const dlg = document.createElement('cmx-floating-dialog')
    dlg.configure({
      title: '选择导出格式',
      icon: 'download',
      confirmText: '导出',
      cancelText: '取消',
      showCancel: true,
      dialogWidth: '420px',
      dialogHeight: 'auto',
    })
    const body = document.createElement('div')
    body.style.cssText = 'padding:16px;font-size:13px;color:var(--sapTextColor,#1d2d3e);font-family:var(--sapFontFamily,Arial,sans-serif);display:flex;flex-direction:column;gap:10px'
    body.innerHTML = `
      <label style="display:flex;align-items:center;gap:6px;cursor:pointer;color:inherit">
        <input type="radio" name="fmt" value="json" checked style="accent-color:var(--neo-cyan,#00b4d8);cursor:pointer">
        <span>JSON（NDJSON 流式，推荐大数据量）</span>
      </label>
      <label style="display:flex;align-items:center;gap:6px;cursor:pointer;color:inherit">
        <input type="radio" name="fmt" value="csv" style="accent-color:var(--neo-cyan,#00b4d8);cursor:pointer">
        <span>CSV（含表头，Excel 友好）</span>
      </label>
      <div style="color:var(--sapContent_LabelColor,#6a6d70);font-size:12px;margin-top:6px">
        将导出当前字典全表数据。
      </div>
    `
    dlg.setContent(body)
    document.body.appendChild(dlg)
    dlg.openModal().then((r) => {
      if (r && r.action === 'confirm') {
        resolve(body.querySelector('input[name="fmt"]:checked')?.value || 'json')
      } else {
        resolve(null)
      }
    }).catch(() => resolve(null))
  })
}

async function doExport (root, fmt) {
  const def = state.def
  const dictCode = state.dictCode
  if (!def || !dictCode) { cmxNotify('warn', '请先选择字典'); return }
  setMsg(root, `导出中（${fmt.toUpperCase()}）…`)
  try {
    // fetch 带 db_id header（Authorization 由前端框架统一注入）
    const url = `/api/dct/export?${qs(def, { dict: dictCode, format: fmt })}`
    const headers = { Accept: fmt === 'csv' ? 'text/csv' : 'application/x-ndjson' }
    if (def.dbId) headers.db_id = def.dbId
    const res = await fetch(url, { headers, credentials: 'same-origin' })
    if (!res.ok) {
      const txt = await res.text().catch(() => '')
      throw new Error(`HTTP ${res.status}${txt ? `: ${txt.slice(0, 200)}` : ''}`)
    }
    // res.blob() 内部流式接收 chunked 响应；浏览器在 Blob > 1MB 时自动落盘到临时文件（file-backed）
    const blob = await res.blob()
    // 触发下载（文件名由后端 Content-Disposition 头指定，前端 download 属性仅作 hint）
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `${def.module}_${dictCode}.${fmt === 'csv' ? 'csv' : 'json'}`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    // 延迟释放 Blob 引用，确保下载已开始
    setTimeout(() => URL.revokeObjectURL(a.href), 1000)
    setMsg(root, '导出完成', 'ok')
    cmxNotify('ok', `导出已下载：${a.download}`)
  } catch (e) {
    setMsg(root, `导出失败：${e.message}`, 'err')
    cmxNotify('error', `导出失败：${e.message}`)
  }
}

/* ─────────────── 导入（弹窗选文件 + 模式 + multipart 上传） ─────────────── */

async function onImportClick (root) {
  const picked = await pickImportFileAndMode(root)
  if (!picked) return
  await doImport(root, picked.file, picked.mode)
}

/** 弹出导入配置对话框：选文件 + 写入模式，返回 { file, mode } 或 null（取消）。 */
function pickImportFileAndMode (_root) {
  return new Promise((resolve) => {
    const dlg = document.createElement('cmx-floating-dialog')
    dlg.configure({
      title: '导入字典数据',
      icon: 'upload',
      confirmText: '开始导入',
      cancelText: '取消',
      showCancel: true,
      dialogWidth: '520px',
      dialogHeight: 'auto',
    })
    const body = document.createElement('div')
    body.style.cssText = 'padding:16px;font-size:13px;color:var(--sapTextColor,#1d2d3e);font-family:var(--sapFontFamily,Arial,sans-serif);display:flex;flex-direction:column;gap:12px'
    body.innerHTML = `
      <div>
        <div style="margin-bottom:6px;font-weight:600;color:inherit">文件：</div>
        <input type="file" id="impFile" accept=".json,.ndjson,.csv" style="width:100%;color:inherit">
        <div style="color:var(--sapContent_LabelColor,#6a6d70);font-size:12px;margin-top:4px">
          支持 JSON（NDJSON）/ CSV，后端自动识别格式
        </div>
      </div>
      <div>
        <div style="margin-bottom:6px;font-weight:600;color:inherit">写入模式：</div>
        <label style="display:flex;align-items:center;gap:6px;margin:4px 0;cursor:pointer;color:inherit">
          <input type="radio" name="imode" value="upsert" checked style="accent-color:var(--neo-cyan,#00b4d8);cursor:pointer">
          <span><b>合并（Upsert）</b> — 按主键存在则更新、不存在则插入（推荐）</span>
        </label>
        <label style="display:flex;align-items:center;gap:6px;margin:4px 0;cursor:pointer;color:inherit">
          <input type="radio" name="imode" value="insert_only" style="accent-color:var(--neo-cyan,#00b4d8);cursor:pointer">
          <span><b>仅新增（InsertOnly）</b> — 主键冲突跳过</span>
        </label>
        <label style="display:flex;align-items:center;gap:6px;margin:4px 0;cursor:pointer;color:inherit">
          <input type="radio" name="imode" value="replace" style="accent-color:var(--sapNegativeColor,#bb0000);cursor:pointer">
          <span><b style="color:var(--sapNegativeColor,#bb0000)">替换（Replace）</b> — 先清空目标表再插入（危险）</span>
        </label>
      </div>
    `
    dlg.setContent(body)
    document.body.appendChild(dlg)

    // 未选择文件前禁用「开始导入」按钮：cmx-floating-dialog 的 confirm 按钮在 shadow DOM
    // 内部（id='dlg-confirm-btn'），用 ui5-button 原生 disabled 属性控制。open shadow root
    // 允许外部 host.shadowRoot.getElementById 访问；组件 id 是稳定文档化标识。
    const fileInput = body.querySelector('#impFile')
    const updateConfirmState = () => {
      const btn = dlg.shadowRoot?.getElementById('dlg-confirm-btn')
      if (!btn) return
      if (fileInput.files.length === 0) btn.setAttribute('disabled', '')
      else btn.removeAttribute('disabled')
    }
    fileInput.addEventListener('change', updateConfirmState)
    // 初始禁用（appendChild 后 shadowRoot 已渲染）
    Promise.resolve().then(updateConfirmState)

    dlg.openModal().then((r) => {
      if (r && r.action === 'confirm') {
        const file = fileInput.files[0]
        if (!file) { cmxNotify('warn', '请先选择文件'); resolve(null); return }
        const mode = body.querySelector('input[name="imode"]:checked')?.value || 'upsert'
        resolve({ file, mode })
      } else {
        resolve(null)
      }
    }).catch(() => resolve(null))
  })
}

async function doImport (root, file, mode) {
  const def = state.def
  const dictCode = state.dictCode
  if (!def || !dictCode) { cmxNotify('warn', '请先选择字典'); return }
  if (mode === 'replace') {
    const ok = await cmxConfirm(
      `⚠️ 替换模式将先清空字典表 ${dictCode} 的全部数据，再从文件插入。\n此操作不可恢复，确定继续？`,
      '危险操作确认'
    )
    if (!ok) return
  }
  setMsg(root, `导入中（${mode}）…`)
  try {
    const fd = new FormData()
    fd.append('file', file)
    fd.append('mode', mode)
    const url = `/api/dct/import?${qs(def, { dict: dictCode })}`
    const res = await fetch(url, { method: 'POST', body: fd, credentials: 'same-origin' })
    const body = await res.json().catch(() => null)
    const data = unwrap(res, body)
    const s = data || {}
    const errs = s.errors || []
    const errMsgs = errs.slice(0, 20).map((e) => `行 ${e.row}：${e.message}`).join('\n')
    const tail = errs.length > 20 ? `\n…（共 ${errs.length} 处错误，仅显示前 20）` : ''
    cmxNotify('ok',
      `导入完成：总计 ${s.total || 0} 行 / 成功 ${s.affected || 0} 行 / 跳过 ${s.skipped || 0} 行` +
      (errMsgs ? `\n\n错误明细：\n${errMsgs}${tail}` : '')
    )
    setMsg(root, `导入完成：${s.affected || 0} 行`, 'ok')
    await reload(root)  // 刷新表格显示
    // 树形字典：导入可能新增/改了节点，左侧结构同步刷新（保持展开态）
    await refreshTree(root)
  } catch (e) {
    setMsg(root, `导入失败：${e.message}`, 'err')
    cmxNotify('error', `导入失败：${e.message}`)
  }
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

/** 判断节点是否叶子（无下级）。分级字典的 is_leaf 字段（TINYINT，1=叶子/0=有子）。
 *  兼容整数 1 / 布尔 true / 字符串 '1' 三种形态（参照 cmx-dct-source.js normalizeDictRow 的判定）。
 *  注意：cmx-dct 后端不维护父子 is_leaf 联动（新增子节点不会把父改 0），is_leaf 只作"已知叶子"提示——
 *  判 true 时省去展开箭头与下级请求；判 false（含缺失）时仍渲染箭头，点开后若返回空也能正常呈现。 */
function isLeafNode (row) {
  const v = row && row.is_leaf
  return v === 1 || v === true || v === '1'
}

function renderTree (root) {
  const body = root.querySelector('#deTreeBody')
  if (!body) return
  const meta = state.meta
  const pk = meta.pk
  const rootChildren = state.treeNodes['null'] || state.treeNodes[null] || []
  /* "全部"=全量（mode='all'）：搜索/浏览跨所有层级。
     "全部根节点"=根级（mode='root'）：只显示 parent_id 为空的根行。
     默认初始 selectedTreeNodeId='__all__'——让用户进入就看到全量。
     selectedTreeNodeId='__root__' = 全部根节点；其他 = 具体节点。 */
  const isAllActive = state.selectedTreeNodeId === '__all__'
  const isRootActive = state.selectedTreeNodeId === '__root__'
  body.innerHTML = `
    <div class="de-tree-virtual ${isAllActive ? 'active' : ''}" data-node-id="__all__" title="显示全部数据（跨所有层级）">⊕ 全部 <span class="de-tree-count">全量</span></div>
    <div class="de-tree-root ${isRootActive ? 'active' : ''}" data-node-id="__root__">▶ 全部根节点（${rootChildren.length}）</div>
    <div class="de-children" id="deTreeChildren">
      ${renderTreeNodes(rootChildren, pk)}
    </div>
  `
  body.querySelector('[data-node-id="__all__"]').addEventListener('click', async () => {
    state.selectedTreeNodeId = '__all__'
    state.currentParentId = undefined   // undefined 让 searchReq 不传 parentId 键 → 后端全量
    state.page = 1
    highlightTreeNode(body)
    await reload(root)
  })
  body.querySelector('[data-node-id="__root__"]').addEventListener('click', async () => {
    state.currentParentId = null       // null 让 searchReq 传 parentId:null → 后端 IS NULL → 根级
    state.selectedTreeNodeId = '__root__'
    state.page = 1
    highlightTreeNode(body)
    await reload(root)
  })
  wireTreeNodeClick(root, body)
}

/** 局部更新树节点选中高亮，不重建 DOM（保留展开/收起状态）。
 *  点击节点本身或根节点时调用——这类操作只改选中态 + 右侧数据，树结构应保持原样。 */
function highlightTreeNode (body) {
  const sel = state.selectedTreeNodeId
  const isAll = sel === '__all__'
  const isRoot = sel === '__root__'
  // 普通节点：仅当非虚拟节点选中且 node-id 匹配时高亮
  body.querySelectorAll('.de-tree-node[data-node-id]').forEach((el) => {
    el.classList.toggle('active', !isAll && !isRoot && String(el.dataset.nodeId) === String(sel))
  })
  // 全部虚拟节点
  const allEl = body.querySelector('.de-tree-virtual[data-node-id="__all__"]')
  if (allEl) allEl.classList.toggle('active', isAll)
  // 全部根节点虚拟节点
  const rootEl = body.querySelector('.de-tree-root[data-node-id="__root__"]')
  if (rootEl) rootEl.classList.toggle('active', isRoot)
}

function renderTreeNodes (nodes, pk) {
  if (!nodes || !nodes.length) return ''
  return nodes.map((n) => {
    const id = n[pk]
    const label = nodeLabel(n)
    const isActive = String(state.selectedTreeNodeId) === String(id)
    const code = state.meta.codeField ? n[state.meta.codeField] : ''
    const meta = code !== '' ? ` <span class="de-tree-meta">${escHtml(String(code))}</span>` : ''
    // 叶子节点（is_leaf=1）：不渲染展开箭头与子容器，省去无意义的下级请求。
    // 用 de-tree-leaf 占位 span 保持与非叶节点箭头同宽（14px），对齐美观。
    if (isLeafNode(n)) {
      return `<div>
        <div class="de-tree-node ${isActive ? 'active' : ''}" data-node-id="${escAttr(String(id))}">
          <span class="de-tree-leaf">●</span>
          <span class="de-tree-label">${escHtml(label)}${meta}</span>
        </div>
      </div>`
    }
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
      // 仅更新高亮，不重建树——避免点击节点导致已展开的子树收起（renderTree 只渲染第一级）。
      // 展开态由 .de-tree-toggle 的 click handler 独立管理，不应受节点选中影响。
      const treeBody = root.querySelector('#deTreeBody')
      if (treeBody) highlightTreeNode(treeBody)
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
      // 已加载过子级 → 直接展开（避免重复请求）
      if (!state.treeNodes[String(id)]) {
        try { await loadTreeChildren(root, id) } catch (e) { /* 忽略 */ }
      }
      const children = state.treeNodes[String(id)] || []
      // 兜底：若下级实际为空（数据 is_leaf 不准或子节点被删光），恢复箭头并标记为叶，
      // 避免留下一个空的展开容器和误导性的 ▾ 图标。
      if (!children.length) {
        tg.textContent = '▸'
        childBox.style.display = 'none'
        return
      }
      childBox.innerHTML = renderTreeNodes(children, state.meta.pk)
      childBox.style.display = 'block'
      wireTreeNodeClick(root, childBox)
    })
  })
}

/* ─────────────── 刷新左侧树（数据变更后保持展开态） ───────────────
 * 删除 / 新增入库 / 导入 / 手动刷新时调用：清空 treeNodes 缓存重新拉取，
 * 并恢复刷新前已展开的节点（▾ 态），避免每次变更后整棵树收起、用户体验割裂。
 * 仅对树形字典（selfHierarchy）生效；平级字典是空操作（reload 已覆盖表格）。 */
async function refreshTree (root) {
  if (!state.meta || !state.meta.selfHierarchy) return
  // 1) 记录刷新前已展开的节点 id（toggle 文本是 ▾）
  const oldBody = root.querySelector('#deTreeBody')
  const expandedIds = []
  if (oldBody) {
    oldBody.querySelectorAll('[data-toggle]').forEach((tg) => {
      if ((tg.textContent || '').trim() === '▾') expandedIds.push(tg.dataset.toggle)
    })
  }
  // 2) 清空缓存，重新加载根节点 + 之前展开的节点（并行；不同 parentId 写入不同 key 不冲突）
  state.treeNodes = {}
  await Promise.all([
    loadTreeChildren(root, null),
    ...expandedIds.map((id) => loadTreeChildren(root, id).catch(() => {})),
  ])
  // 3) 重新渲染（renderTree 只渲染第一级，子级靠下方恢复展开态）
  renderTree(root)
  // 4) 恢复展开态：用已加载的最新子级数据填充子容器
  const newBody = root.querySelector('#deTreeBody')
  if (!newBody) return
  for (const id of expandedIds) {
    const tg = newBody.querySelector(`[data-toggle="${CSS.escape(id)}"]`)
    const childBox = newBody.querySelector(`[data-children-of="${CSS.escape(id)}"]`)
    if (!tg || !childBox) continue       // 节点已被删除等场景
    const children = state.treeNodes[String(id)] || []
    if (!children.length) continue        // 子级被删光，保持收起
    tg.textContent = '▾'
    childBox.innerHTML = renderTreeNodes(children, state.meta.pk)
    childBox.style.display = 'block'
    wireTreeNodeClick(root, childBox)
  }
}

/* ─────────────── 切换字典 ─────────────── */
async function switchDict (root, dictCode) {
  state.dictCode = dictCode
  state.meta = null
  state.page = 1
  state.q = ''
  state.conds = []
  /* 默认"全部"模式：currentParentId=undefined → loadData searchReq 不传 parentId → 后端全量。
     用户主动选"全部根节点"（__root__）则 currentParentId=null（IS NULL），选具体节点则 =<id>。 */
  state.currentParentId = undefined
  state.selectedTreeNodeId = '__all__'
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
  // 刷新：表格 + 树形字典的左侧结构都重载（树刷新会保留展开态）
  root.querySelector('#btnReload')?.addEventListener('click', () => {
    state.page = 1
    void reload(root).then(() => refreshTree(root))
  })
  root.querySelector('#btnAdd')?.addEventListener('click', () => addRow(root))
  root.querySelector('#btnDel')?.addEventListener('click', () => void deleteSelected(root))
  root.querySelector('#btnSave')?.addEventListener('click', () => void save(root))
  root.querySelector('#btnExport')?.addEventListener('click', () => void onExportClick(root))
  root.querySelector('#btnImport')?.addEventListener('click', () => void onImportClick(root))

  // 搜索
  const search = () => {
    state.q = (root.querySelector('#deQ')?.value || '').trim()
    state.page = 1
    /* 关键字搜索强制切到"全部"模式：保证跨层级命中都能看到，不被 parentId 过滤。
       后端 /api/dct/data/search 接 q 后对 code/label 模糊匹配，parentId 不传时全量返回。
       与"全部"虚拟节点（renderTree 中 data-node-id="__all__"）的语义对齐：
       用户在树形字典下搜索时，无论当前选中哪个节点，搜索结果都应该是全量。 */
    if (state.meta && state.meta.selfHierarchy) {
      state.currentParentId = undefined   // 触发 loadData 不带 parentId 键 → 后端全量
      state.selectedTreeNodeId = '__all__'
      // 同步更新左侧树高亮（保持按钮 active 一致）
      const treeBody = root.querySelector('#deTreeBody')
      if (treeBody) highlightTreeNode(treeBody)
    }
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
