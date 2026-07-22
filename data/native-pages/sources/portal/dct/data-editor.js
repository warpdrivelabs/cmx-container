/**
 * data-editor —— 通用字典数据维护页（元数据驱动，native_pages）。
 *
 * 通过 props 注入字典坐标四元组，根据 dictCode 从 `/api/dct/meta` 加载 DictView，
 * 动态构建列模型与编辑表单。支持：
 *   - 平级字典（selfHierarchy=false）：单表格分页查询 + 关键字/条件搜索 + 弹窗增删改 + changeset 显式保存
 *   - 树形字典（selfHierarchy=true）：左树右表（懒加载子节点）+ 父子关系维护
 *   - 顶部字典切换器：列出同坐标下所有字典，菜单 props 不预绑 dictCode 时按需切换
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }；ctx.props 来自菜单。
 * CMX 类经 globalThis.__cmxDataComp 取用。
 *
 * 字段策略（依据元数据角色，列表/弹窗分别处理）：
 *   - 业务列（code/name/sort_no/status/own fields/refDict 列/parent 列/生效期/停用信息）→ 列表显示 + 弹窗可编辑
 *   - 审计列（create_by/create_time/update_by/update_time）→ 列表显示（只读）+ 弹窗隐藏（后端自动维护）
 *   - 系统标识（is_system）→ 列表显示 + 弹窗隐藏
 *   - 主键（id/code 作 PK）→ 列表显示 + 弹窗隐藏（新增后端铸号或为业务键时编辑后不可改）
 *   - 派生层级（full_path/level_no/is_leaf）→ 列表与弹窗均隐藏（后端 backfill）
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

/* ─────────────── 字段角色识别（依据语义角色，不依赖 fieldSet 来源标签） ─────────────── */
const AUDIT_FIELDS = new Set(['create_by', 'create_time', 'update_by', 'update_time'])
const SYSTEM_FLAG_FIELDS = new Set(['is_system'])
const DERIVED_HIERARCHY = new Set(['full_path', 'level_no', 'is_leaf'])

/** 该列在列表中是否显示：派生层级噪声列隐藏，其余（含审计/主键/is_system）全部显示。 */
function showInTable (col, meta) {
  if (DERIVED_HIERARCHY.has(col.name)) return false
  return true
}

/** 该列在编辑弹窗中是否可编辑：主键/审计/系统标识/派生层级均隐藏，其余可编辑。 */
function isEditable (col, meta) {
  if (col.name === meta.pk) return false
  if (AUDIT_FIELDS.has(col.name)) return false
  if (SYSTEM_FLAG_FIELDS.has(col.name)) return false
  if (DERIVED_HIERARCHY.has(col.name)) return false
  return true
}

/** 列 caption 提取（兼容多语言对象/字符串）。 */
function colCaption (col) {
  if (col.caption && typeof col.caption === 'object') return col.caption.zh_CN || col.caption.en || col.name
  return col.caption || col.name
}

/** 按 dataType 给出列宽。 */
function defaultWidthFor (col) {
  const t = String(col.dataType || '').toUpperCase()
  if (t === 'DATETIME') return '160px'
  if (t === 'DATE') return '120px'
  if (t === 'TEXT') return '240px'
  if (t === 'TINYINT') return '80px'
  if (t === 'INT' || t === 'BIGINT') return '100px'
  return '140px'
}

/* ─────────────── 模块级 state（每次 content 入口重置） ─────────────── */
const state = {
  def: null,        // { domain, application, module, dbId }
  dicts: [],        // 同坐标下所有 DCT 字典清单（每项含 dictCode/dictName）
  meta: null,       // 当前 dictCode 的 DictView
  dictCode: '',     // 当前选中字典
  // 查询状态
  page: 1,
  pageSize: 50,
  q: '',
  conds: [],        // 高级条件：[{col, op, value}]
  // 树形专用
  treeNodes: {},    // nodeId -> children[]（懒加载缓存）
  currentParentId: null, // 右表当前显示的父节点（null=根）
  selectedTreeNodeId: null,
  // 数据
  rows: [],
  total: 0,
  grid: null,
  // 本地变更集（未提交）
  changes: { inserted: [], updated: [], deleted: [] },
  baselineMap: {},  // id -> update_time（乐观锁）
  _lastDictCode: null,  // 上次设过列模型的 dictCode（变化时重建 grid 元素，绕过列签名缓存）
  selectedRowId: null,  // 当前单选行 id（cmx-row-selected）
  selectedIds: [],      // 当前多选行 id 列表（cmx-row-selection-change）
}

/* ─────────────── HTTP 辅助 ─────────────── */

/** 归一响应（兼容信封/裸数据两种形态）。 */
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

/* ─────────────── 样式（Neo 主题，套用 color-mix + var(--neo-*) + var(--sap-*)） ─────────────── */
function styleHtml () {
  return `<style>
.de-root{--neo-cyan:#00b4d8;--neo-mint:#10b981;--neo-warn:#f59e0b;--neo-red:#e90b0b;
  display:flex;flex-direction:column;height:100%;width:100%;box-sizing:border-box;padding:10px;gap:10px;
  min-width:0;font:13px/1.5 var(--sapFontFamily,Arial,sans-serif);
  color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#f5f6f7);overflow:hidden}
/* 顶栏：Neo 渐变标题条 */
.de-bar{flex:0 0 auto;display:flex;align-items:center;gap:10px;height:46px;box-sizing:border-box;padding:0 12px;
  border-bottom:1px solid color-mix(in srgb,var(--neo-cyan) 22%,var(--sapGroup_TitleBorderColor,#d9d9d9));
  background:color-mix(in srgb,var(--neo-cyan) 12%,var(--sapList_HeaderBackground,#eef2f6));border-radius:8px 8px 0 0}
.de-bar ui5-icon{width:1.2rem;height:1.2rem;color:var(--neo-cyan)}
.de-title{font-weight:700;font-size:14px;color:var(--neo-cyan)}
.de-subtitle{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-msg{margin-left:auto;font-size:12px;color:var(--neo-cyan)}
.de-msg.err{color:var(--neo-red)}
.de-msg.ok{color:var(--neo-mint)}
/* 工具栏 */
.de-toolbar{flex:0 0 auto;display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:6px 10px;
  background:var(--sapList_Background,#fff);border:1px solid color-mix(in srgb,var(--neo-cyan) 18%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  border-radius:6px}
.de-toolbar > ui5-label{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-dict-select{min-width:220px}
/* 筛选条（biz-bar 风格） */
.de-filter{flex:0 0 auto;display:flex;gap:8px;align-items:center;flex-wrap:wrap;padding:8px 10px;
  background:color-mix(in srgb,var(--neo-cyan) 6%,var(--sapList_Background,#fff));
  border:1px solid color-mix(in srgb,var(--neo-cyan) 22%,transparent);border-radius:6px;font-size:12px}
.de-cond{display:flex;gap:4px;align-items:center;background:var(--sapField_Background,#fff);
  border-radius:4px;padding:3px 6px;border:1px solid color-mix(in srgb,var(--neo-cyan) 25%,var(--sapField_BorderColor,#b3b3b3))}
/* 主体 */
.de-body{flex:1;display:flex;gap:10px;min-height:0;min-width:0}
.de-body.flat{flex-direction:column}
.de-right{flex:1;display:flex;flex-direction:column;min-width:0;min-height:0;
  background:var(--sapList_Background,#fff);border:1px solid color-mix(in srgb,var(--neo-cyan) 15%,var(--sapGroup_ContentBorderColor,#d9d9d9));
  border-radius:6px;overflow:hidden}
.de-grid-wrap{flex:1;min-height:0;position:relative}
.de-grid{display:block;width:100%;height:100%}
/* 左树（lvlbox 风格） */
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
/* 分页 */
.de-pager{flex:0 0 auto;display:flex;gap:8px;align-items:center;justify-content:space-between;padding:6px 10px;font-size:12px;
  background:var(--sapList_Background,#fff);border-top:1px solid color-mix(in srgb,var(--neo-cyan) 12%,var(--sapGroup_ContentBorderColor,#d9d9d9))}
.de-pager-info{color:var(--sapContent_LabelColor,#6a6d70)}
.de-pager-btns{display:flex;gap:4px;align-items:center}
.de-dirty{color:var(--neo-warn);font-weight:700}
.de-loading,.de-empty{padding:32px;text-align:center;color:var(--sapContent_LabelColor,#6a6d70);font-size:13px}
/* 弹窗内表单 */
.de-dialog-body{padding:14px;display:flex;flex-direction:column;gap:10px;max-height:60vh;overflow:auto;
  font:13px/1.5 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e)}
.de-dialog-row{display:grid;grid-template-columns:120px 1fr;gap:8px;align-items:center}
.de-dialog-row>label{font-size:13px;color:var(--sapContent_LabelColor,#6a6d70)}
.de-dialog-row .req{color:var(--neo-red);margin-left:2px}
.de-dialog-warn{padding:8px 12px;background:color-mix(in srgb,var(--neo-warn) 12%,var(--sapList_Background,#fff));
  border:1px solid var(--neo-warn);border-radius:4px;font-size:12px;color:color-mix(in srgb,var(--neo-warn) 70%,#000)}
.de-dialog-row ui5-input,.de-dialog-row ui5-select,.de-dialog-row ui5-combobox,.de-dialog-row ui5-date-picker,.de-dialog-row ui5-datetime-picker,.de-dialog-row ui5-step-input{width:100%}
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
    <span style="color:var(--sapContent_LabelColor,#6a6d70)">|</span>
    <ui5-button design="Transparent" icon="add" id="btnAddCond">+ 条件</ui5-button>
    <span id="deCondBox" style="display:flex;gap:4px;align-items:center;flex-wrap:wrap"></span>
  </div>
  <div class="${bodyClass}" id="deBody">
    ${treePart}
    <div class="de-right">
      <div class="de-grid-wrap"><!-- grid 由 applyRowsToGrid 动态创建 --></div>
      <div class="de-pager">
        <span class="de-pager-info" id="dePageInfo">—</span>
        <span class="de-pager-btns">
          <ui5-button design="Transparent" icon="navigation-left-arrow" id="btnPrev">上一页</ui5-button>
          <span id="dePageNum" style="min-width:60px;text-align:center"></span>
          <ui5-button design="Transparent" icon="navigation-right-arrow" id="btnNext">下一页</ui5-button>
        </span>
      </div>
    </div>
  </div>
</div>`
}

/* ─────────────── 提示辅助（替代 alert/confirm） ─────────────── */
function setMsg (root, text, kind) {
  const el = root.querySelector('#deMsg')
  if (!el) return
  el.textContent = text || ''
  el.className = 'de-msg' + (kind ? ' ' + kind : '')
}

/** 提示消息（替代 alert）。优先用 cmx-data-comp 的 cmxInfo/cmxWarn/cmxError；
 *  不可用时用 cmx-floating-dialog 兜底（非阻塞，自动延时关闭）。 */
function cmxNotify (kind, msg) {
  const C = cmx()
  const map = { info: 'cmxInfo', warn: 'cmxWarn', error: 'cmxError', ok: 'cmxInfo' }
  const fn = C && C[map[kind] || map.info]
  if (typeof fn === 'function') { try { fn(msg) } catch (_) {} return }
  // 兜底：cmx-floating-dialog 命令式
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

/** 确认对话框（基于 cmx-floating-dialog 命令式 API），返回 Promise<boolean>。 */
async function cmxConfirm (message, title) {
  const C = cmx()
  if (C && typeof C.cmxConfirm === 'function') {
    try { return !!(await C.cmxConfirm(message, title || '请确认')) } catch (_) {}
  }
  // 命令式五步：createElement → configure → setContent → appendChild → openModal
  const dlg = document.createElement('cmx-floating-dialog')
  if (typeof dlg.configure !== 'function') return false  // cmx-data-comp 未就绪
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
  try {
    const r = await dlg.openModal()
    return r && r.action === 'confirm'
  } catch (_) {
    return false
  }
}

/* ─────────────── 异步等待渲染（来自 doc-loader 范式） ─────────────── */
function whenRendered (host, selector, cb, tries) {
  const t = tries == null ? 60 : tries
  const root = host && host.renderRoot
  if (root && root.querySelector(selector)) { cb(root); return }
  if (t <= 0) return
  requestAnimationFrame(() => whenRendered(host, selector, cb, t - 1))
}

/* ─────────────── 校验并归一 props → def ─────────────── */
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

/* ─────────────── 字典清单（切换器） ─────────────── */
async function loadDictList (def) {
  const data = await apiGet(
    `/api/definitions/list?kind=DCT&domain=${encodeURIComponent(def.domain)}&application=${encodeURIComponent(def.application)}&module=${encodeURIComponent(def.module)}`,
    def.dbId,
  )
  // 优先取 isDefault=true 的文件，否则取 version 最大
  const items = (data && data.items) || []
  if (!items.length) return []
  const def_ = items.find((it) => it.isDefault) || items.slice().sort((a, b) => (Number(b.version) || 0) - (Number(a.version) || 0))[0]
  return (def_.dictionaries || []).map((d) => ({ dictCode: d.dictCode, dictName: d.dictName }))
}

/* ─────────────── 元数据加载 ─────────────── */
async function loadMeta (def, dictCode) {
  return apiGet(`/api/dct/meta?${qs(def, { dict: dictCode })}`, def.dbId)
}

/* ─────────────── 列模型（按字段策略过滤） ─────────────── */
function buildColumnModel (meta) {
  const C = cmx()
  if (!C.CmxColumnModel || !C.CmxColumn) return null
  const members = (meta.columns || [])
    .filter((c) => showInTable(c, meta))
    .map((c) => {
      const colOpts = {
        id: c.name,
        caption: colCaption(c),
        dataType: c.dataType,
        width: defaultWidthFor(c),
      }
      // 引用字典列：cmx-revo-grid 通过 enableDictEcho 把 code 回显为 name
      if (c.refDict) {
        colOpts.edit = { mode: 'readonly' }
        colOpts.refDict = c.refDict
        colOpts.refField = c.refField
        colOpts.displayField = c.displayField
      } else {
        // 列级只读（编辑统一走弹窗）
        colOpts.edit = { mode: 'readonly' }
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

/** 高级条件 conds → filters（仅等值；其他算子简化为 IN/等值，复杂算子后端 search 暂只支持等值）。 */
function buildFiltersFromConds (meta) {
  const f = {}
  const validCols = new Set((meta.columns || []).map((c) => c.name))
  for (const c of state.conds) {
    if (!c.col || !validCols.has(c.col)) continue
    const v = String(c.value || '').trim()
    if (!v) continue
    // 多值按逗号分隔时取第一个（后端等值过滤），简化处理
    f[c.col] = /^\-?\d+(\.\d+)?$/.test(v) ? Number(v) : v
  }
  return Object.keys(f).length ? f : undefined
}

/* ─────────────── 数据集填充 grid ───────────────
 * 决定性策略：列模型变化时**销毁并重建 grid 元素**，彻底绕过 cmx-revo-grid 的
 * _revoPropSigs 列签名缓存与 _isLayoutVisible 可见性守卫（这两者会导致切换字典时
 * setColumnModel 被静默跳过）。重建后新 grid 的 connectedCallback 同步赋值 _revo，
 * setColumnModel 必然命中 _syncToRevo。数据用 CmxDataSet 注入。
 */
async function applyRowsToGrid (root, rows) {
  const C = cmx()
  const wrap = root.querySelector('.de-grid-wrap')
  if (!wrap) return

  // 列模型变化 → 重建 grid（含首次）
  if (state._lastDictCode !== state.dictCode) {
    // 移除旧 grid
    const oldGrid = wrap.querySelector('cmx-revo-grid')
    if (oldGrid) oldGrid.remove()
    // 新建 grid 并立即插入 DOM（触发 connectedCallback 同步赋值 _revo）
    const newGrid = document.createElement('cmx-revo-grid')
    newGrid.className = 'de-grid'
    newGrid.id = 'deGrid'
    wrap.appendChild(newGrid)
    state.grid = newGrid
    // 设置列模型（此时 _revo 已非 null，_applyColumnModel → _syncToRevo 必然执行）
    const cm = buildColumnModel(state.meta)
    if (cm) newGrid.setColumnModel(cm)
    newGrid.setOptions && newGrid.setOptions({ selectionMode: 'multi', fillHeight: true, showRowIndex: true, editable: false })
    // 绑定行事件
    wireGridEvents(newGrid, root)
    state._lastDictCode = state.dictCode
    // 等 Stencil 内部首次渲染完成（双 rAF，仿 doc-loader.js:356）
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
  }

  const grid = state.grid
  if (!grid) return

  // 数据：注入 id 字段（cmx-revo-grid 内部用 id 作行标识）
  const pk = state.meta.pk
  const normRows = (rows || []).map((r) => ({ ...r, id: r[pk] }))
  if (C.CmxDataSet) {
    const ds = new C.CmxDataSet({})
    ds.setRows(normRows)
    grid.setDataSet(ds)
  } else if (grid.setDataSet) {
    grid.setDataSet(normRows)
  }

  // 强制刷新（兜底）
  try { grid.refreshLayout && grid.refreshLayout() } catch (_) {}
}

/** 绑定 grid 的行选中 / 双击编辑事件。 */
function wireGridEvents (grid, root) {
  // cmx-row-selected：detail 只有 id，记录到 state
  grid.addEventListener('cmx-row-selected', (ev) => {
    const id = ev.detail && ev.detail.id
    state.selectedRowId = id
  })
  grid.addEventListener('cmx-row-selection-change', (ev) => {
    const ids = (ev.detail && ev.detail.ids) || []
    state.selectedIds = ids
  })
  // 双击行打开编辑（cmx-revo-grid 无原生 dblclick 事件，用 DOM dblclick + composedPath 找行 id）
  grid.addEventListener('dblclick', (ev) => {
    const path = ev.composedPath ? ev.composedPath() : []
    let rowEl = null
    for (let i = 0; i < path.length; i++) {
      const el = path[i]
      if (el && el.classList && el.classList.contains('rgRow')) { rowEl = el; break }
    }
    const rowId = rowEl && rowEl.getAttribute ? rowEl.getAttribute('data-rgrow') : null
    if (rowId == null) return
    const pk = state.meta ? state.meta.pk : 'id'
    const row = state.rows.find((r) => String(r[pk]) === String(rowId))
    if (row) void openEditDialog(root, 'edit', row)
  })
}

function renderPageInfo (root) {
  const info = root.querySelector('#dePageInfo')
  const num = root.querySelector('#dePageNum')
  const dirty = state.changes.inserted.length + state.changes.updated.length + state.changes.deleted.length
  const dirtyText = dirty ? ` · <span class="de-dirty">未保存 ${dirty} 项</span>` : ''
  if (info) {
    info.innerHTML = `共 ${state.total} 条${state.meta && state.meta.selfHierarchy ? ` · 当前父节点：${state.currentParentId == null ? '根' : state.currentParentId}` : ''}${dirtyText}`
  }
  const totalPages = Math.max(1, Math.ceil(state.total / state.pageSize))
  if (num) num.textContent = `第 ${state.page} / ${totalPages} 页`
}

/* ─────────────── 重新装载（查询/分页/切换父节点后） ─────────────── */
async function reload (root) {
  if (!state.meta) return
  setMsg(root, '装载中…')
  try {
    const data = await loadData(state.def, state.dictCode, state.meta)
    state.rows = (data && data.rows) || []
    state.total = (data && Number(data.total)) || 0
    // 记录每行的 update_time 作为乐观锁 baseline
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

/* ─────────────── 弹窗：表单字段构建（按 isEditable 过滤） ─────────────── */
function editableColumns (meta) {
  return (meta.columns || []).filter((c) => isEditable(c, meta))
}

/** 构造弹窗字段 HTML：根据 dataType / refDict / parentField 派生控件。 */
function formFieldHtml (col, meta, value, row) {
  const cap = colCaption(col)
  const req = col.nullable === false ? '<span class="req">*</span>' : ''
  const valAttr = value == null ? '' : `value="${escAttr(String(value))}"`
  const t = String(col.dataType || '').toUpperCase()
  const name = col.name
  // 状态列（status）特殊化为启用/停用下拉
  if (name === 'status') {
    const v = value == null ? 1 : Number(value)
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-select data-field="${name}">
        <ui5-option value="1" ${v === 1 ? 'selected' : ''}>启用</ui5-option>
        <ui5-option value="0" ${v === 0 ? 'selected' : ''}>停用</ui5-option>
      </ui5-select></div>`
  }
  // 树形字典的 parent 列：选父节点（异步加载候选）
  if (meta.selfHierarchy && name === meta.parentField) {
    const v = value == null ? '' : String(value)
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-combobox data-field="${name}" data-parent-picker value="${escAttr(v)}" placeholder="（根节点留空）">
        <ui5-icon slot="icon" name="tree"></ui5-icon>
      </ui5-combobox></div>`
  }
  // 引用字典列：选另一字典的 code/name（异步加载候选）
  if (col.refDict) {
    const v = value == null ? '' : String(value)
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-combobox data-field="${name}" data-ref-dict="${escAttr(col.refDict)}" data-ref-field="${escAttr(col.refField || 'code')}" data-display-field="${escAttr(col.displayField || 'name')}" value="${escAttr(v)}">
        <ui5-icon slot="icon" name="value-help"></ui5-icon>
      </ui5-combobox></div>`
  }
  // DATE
  if (t === 'DATE') {
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-date-picker data-field="${name}" ${valAttr} format-pattern="yyyy-MM-dd"></ui5-date-picker></div>`
  }
  // DATETIME
  if (t === 'DATETIME') {
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-datetime-picker data-field="${name}" ${valAttr} format-pattern="yyyy-MM-dd HH:mm:ss"></ui5-datetime-picker></div>`
  }
  // 整数
  if (t === 'INT' || t === 'BIGINT' || t === 'TINYINT') {
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-step-input data-field="${name}" ${valAttr} step="1"></ui5-step-input></div>`
  }
  // DECIMAL
  if (t === 'DECIMAL') {
    return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
      <ui5-input data-field="${name}" type="Number" ${valAttr}></ui5-input></div>`
  }
  // 默认：VARCHAR / TEXT
  return `<div class="de-dialog-row"><label>${escHtml(cap)}${req}</label>
    <ui5-input data-field="${name}" ${valAttr}></ui5-input></div>`
}

/** HTML 转义。 */
function escHtml (s) {
  return String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c])
}
function escAttr (s) {
  return String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c])
}

/* ─────────────── 弹窗：新增/编辑（命令式 cmx-floating-dialog） ───────────────
 * cmx-floating-dialog 是一次性组件：createElement → configure → setContent →
 * appendChild → openModal，关闭后自动 remove。禁止 innerHTML / setAttribute / slot。
 */
async function openEditDialog (root, mode, row) {
  const meta = state.meta
  if (!meta) return
  const isNew = mode === 'add'
  const editable = editableColumns(meta)
  const isSystemRow = !!(row && Number(row.is_system) === 1)
  // 新增时预填 parent_id（树形字典）
  const prefill = isNew ? {} : { ...(row || {}) }
  if (isNew && meta.selfHierarchy && meta.parentField) {
    prefill[meta.parentField] = state.currentParentId
  }
  // 构建表单容器（普通 div，innerHTML 注入 UI5 控件）
  const formRoot = document.createElement('div')
  formRoot.className = 'de-dialog-body'
  const fieldsHtml = editable.map((col) => formFieldHtml(col, meta, prefill[col.name], prefill)).join('')
  const warnHtml = isSystemRow ? `<div class="de-dialog-warn">⚠️ 该项为系统预置（is_system=1），修改可能影响系统运行，请谨慎操作。</div>` : ''
  formRoot.innerHTML = warnHtml + fieldsHtml

  // 创建对话框
  const dlg = document.createElement('cmx-floating-dialog')
  if (typeof dlg.configure !== 'function') {
    cmxNotify('error', 'cmx-floating-dialog 未就绪，无法打开编辑窗口')
    return
  }
  const title = isNew ? `新增 · ${meta.dictName || meta.dictCode}` : `编辑 · ${meta.dictName || meta.dictCode} · ${row ? (row[meta.pk] || '') : ''}`
  dlg.configure({
    title,
    icon: isNew ? 'add' : 'edit',
    confirmText: '确定',
    cancelText: '取消',
    dialogWidth: '640px',
    dialogHeight: 'auto',
    // beforeClose 返回 false 阻止关闭（用于校验失败）
    beforeClose: async ({ action }) => {
      if (action !== 'confirm') return true
      const values = collectFormValues(formRoot, meta)
      if (!values) return false  // 校验失败，保持弹窗
      await applyEdit(root, mode, row, values)
      return true
    },
  })
  dlg.setContent(formRoot)
  document.body.appendChild(dlg)

  // 等表单挂到 DOM 后再异步填 combobox 候选
  requestAnimationFrame(() => { void fillComboboxOptions(formRoot, meta, row) })

  // 等待用户操作（关闭后自动 remove）
  try { await dlg.openModal() } catch (_) { /* 用户关闭 */ }
}

/** 异步为 combobox（引用字典 + 父节点选择）填充候选选项。 */
async function fillComboboxOptions (formRoot, meta, editingRow) {
  const C = cmx()
  // 引用字典列
  const refCombos = formRoot.querySelectorAll('ui5-combobox[data-ref-dict]')
  for (const cb of refCombos) {
    const refDict = cb.dataset.refDict
    const refField = cb.dataset.refField || 'code'
    const displayField = cb.dataset.displayField || 'name'
    try {
      const data = await apiPost(
        `/api/dct/data/search?${qs(state.def, { dict: refDict })}`,
        { page: 1, pageSize: 200 },
        state.def.dbId,
      )
      const rows = (data && data.rows) || []
      cb.innerHTML = rows.map((r) => {
        const v = r[refField]
        const l = r[displayField]
        const cur = cb.getAttribute('value')
        return `<ui5-cb-item text="${escAttr(l || '')}" additional-text="${escAttr(String(v == null ? '' : v))}" value="${escAttr(String(v == null ? '' : v))}" ${String(v) === cur ? 'selected' : ''}></ui5-cb-item>`
      }).join('')
    } catch (e) { /* 忽略，用户可手输 */ }
  }
  // 树形字典父节点选择
  const parentCombo = formRoot.querySelector('ui5-combobox[data-parent-picker]')
  if (parentCombo && meta.selfHierarchy) {
    const editingId = editingRow ? editingRow[meta.pk] : null
    try {
      // 拉全量节点供选父（排除自身；理想是排除自身子孙，但前端简化只排除自身）
      const data = await apiPost(
        `/api/dct/data/search?${qs(state.def, { dict: state.dictCode })}`,
        { page: 1, pageSize: 2000 },
        state.def.dbId,
      )
      const rows = ((data && data.rows) || []).filter((r) => String(r[meta.pk]) !== String(editingId))
      parentCombo.innerHTML = rows.map((r) => {
        const v = r[meta.pk]
        const lblFld = meta.labelField || 'name'
        const codeFld = meta.codeField || 'code'
        const l = r[lblFld] || r[codeFld] || r[meta.pk] || ''
        const cur = parentCombo.getAttribute('value')
        return `<ui5-cb-item text="${escAttr(String(l))}" additional-text="${escAttr(String(v))}" value="${escAttr(String(v))}" ${String(v) === cur ? 'selected' : ''}></ui5-cb-item>`
      }).join('')
    } catch (e) { /* 忽略 */ }
  }
}

/** 从弹窗收集表单值，含必填/类型校验；失败返回 null 并 cmxWarn。 */
function collectFormValues (formRoot, meta) {
  const out = {}
  const editable = editableColumns(meta)
  let firstError = ''
  for (const col of editable) {
    const el = formRoot.querySelector(`[data-field="${col.name}"]`)
    if (!el) continue
    let raw
    if (el.tagName === 'UI5-SELECT' || el.tagName === 'UI5-COMBOBOX') {
      // ui5-select 取选中 option value；ui5-combobox 取 input value
      const opt = el.querySelector('ui5-option[selected]') || el.querySelector('ui5-cb-item[selected]')
      raw = opt ? opt.getAttribute('value') : (el.value || el.getAttribute('value') || '')
    } else {
      raw = el.value != null ? el.value : el.getAttribute('value')
    }
    raw = raw == null ? '' : String(raw).trim()
    const t = String(col.dataType || '').toUpperCase()
    // 必填
    if (col.nullable === false && !raw) {
      firstError = firstError || `字段「${colCaption(col)}」必填`
      continue
    }
    if (!raw) {
      out[col.name] = null
      continue
    }
    // 类型转换
    if (t === 'INT' || t === 'BIGINT' || t === 'TINYINT') {
      if (!/^-?\d+$/.test(raw)) { firstError = firstError || `字段「${colCaption(col)}」须为整数`; continue }
      out[col.name] = Number(raw)
    } else if (t === 'DECIMAL') {
      const n = Number(raw)
      if (!Number.isFinite(n)) { firstError = firstError || `字段「${colCaption(col)}」须为数字`; continue }
      out[col.name] = n
    } else {
      out[col.name] = raw
    }
  }
  if (firstError) {
    cmxNotify('warn', firstError)
    return null
  }
  return out
}

/** 把表单值累积到本地 changes，并即时刷新本地显示。 */
async function applyEdit (root, mode, row, values) {
  const meta = state.meta
  const pk = meta.pk
  if (mode === 'add') {
    // 临时 id 占位（字符串），保存时后端会铸号
    const tempId = `t${Date.now()}${Math.floor(Math.random() * 1000)}`
    const newRow = { [pk]: tempId, ...values }
    state.changes.inserted.push({ id: tempId, fields: { ...newRow } })
    // 本地展示
    state.rows.push(newRow)
  } else {
    // 编辑：写 changes.updated（带 baseline update_time）
    const id = row[pk]
    const baseline = state.baselineMap[String(id)]
    state.changes.updated.push({
      id,
      fields: { ...values },
      ...(baseline != null ? { baseline } : {}),
    })
    // 若该行是之前同次会话插入的，更新 inserted 而非再压一份 updated
    const ins = state.changes.inserted.find((x) => String(x.id) === String(id))
    if (ins) {
      Object.assign(ins.fields, values)
    } else {
      // 同步更新本地行
      const idx = state.rows.findIndex((r) => String(r[pk]) === String(id))
      if (idx >= 0) state.rows[idx] = { ...state.rows[idx], ...values }
    }
  }
  await applyRowsToGrid(root, state.rows)
  renderPageInfo(root)
  setMsg(root, '已暂存变更，点击「保存」提交', 'ok')
}

/* ─────────────── 删除 ─────────────── */
async function deleteSelected (root) {
  const meta = state.meta
  const grid = state.grid
  if (!meta || !grid) return
  const pk = meta.pk
  // cmx-revo-grid 的选择 API：getSelectedIds() 返回 string[]
  const ids = (grid.getSelectedIds ? grid.getSelectedIds() : []) || []
  // 字符串化比较，避免数字/字符串类型不匹配
  const idSet = new Set(ids.map((x) => String(x)))
  if (!idSet.size) {
    cmxNotify('info', '请先在表格中选择要删除的行')
    return
  }
  // 按 id 从本地 rows 找出完整行
  const selRows = state.rows.filter((r) => idSet.has(String(r[pk])))
  if (!selRows.length) {
    cmxNotify('warn', '无法定位选中行（可能已过期）')
    return
  }
  for (const r of selRows) {
    if (Number(r.is_system) === 1) {
      const lbl = meta.labelField ? (r[meta.labelField] || '') : ''
      cmxNotify('warn', `「${lbl || r[pk]}」为系统预置项，不可删除`)
      return
    }
  }
  const ok = await cmxConfirm(`确认删除选中的 ${selRows.length} 项？`, '删除确认')
  if (!ok) return
  for (const r of selRows) {
    const id = r[pk]
    // 若是同次会话插入的，直接从 inserted 移除
    const insIdx = state.changes.inserted.findIndex((x) => String(x.id) === String(id))
    if (insIdx >= 0) {
      state.changes.inserted.splice(insIdx, 1)
    } else {
      state.changes.deleted.push(id)
      // 同时移除已存在的 updated（若此前编辑过该行）
      state.changes.updated = state.changes.updated.filter((u) => String(u.id) !== String(id))
    }
    // 从本地行移除
    state.rows = state.rows.filter((x) => String(x[pk]) !== String(id))
  }
  await applyRowsToGrid(root, state.rows)
  renderPageInfo(root)
  setMsg(root, '已暂存删除，点击「保存」提交', 'ok')
}

/* ─────────────── 保存（changeset） ─────────────── */
async function save (root) {
  const meta = state.meta
  const def = state.def
  if (!meta) return
  const { inserted, updated, deleted } = state.changes
  const dirty = inserted.length + updated.length + deleted.length
  if (!dirty) { cmxNotify('info', '无变更可保存'); return }
  setMsg(root, '保存中…')
  const tableName = meta.tableName
  const payload = {
    saveMode: 'merge',
    changes: { [tableName]: { inserted, updated, deleted } },
  }
  try {
    const r = await apiPost(`/api/dct/save?${qs(def, { dict: state.dictCode })}`, payload, def.dbId)
    const aff = (r && r.affected) || 0
    cmxNotify('ok', `保存成功：影响 ${aff} 行（新增 ${inserted.length} / 更新 ${updated.length} / 删除 ${deleted.length}）`)
    // 清空 baseline 后重新装载（reload 会按新行 update_time 重建 baselineMap）
    state.baselineMap = {}
    state.changes = { inserted: [], updated: [], deleted: [] }
    // 重新装载（取最新数据 + 重建 baseline）
    await reload(root)
    // 若后端返回了 updatedAt，覆盖（更精确）
    if (r && Array.isArray(r.updatedAt)) {
      for (const u of r.updatedAt) {
        if (u.id != null && u.updateTime != null) state.baselineMap[String(u.id)] = u.updateTime
      }
    }
  } catch (e) {
    const status = e.status
    if (status === 409) {
      cmxNotify('error', '字典项已被他人修改，已自动刷新到最新版本')
      state.changes = { inserted: [], updated: [], deleted: [] }
      await reload(root)
    } else if (status === 422 && e.body && e.body.data && Array.isArray(e.body.data.violations)) {
      presentViolations(e.body.data.violations)
      setMsg(root, `保存失败：${e.message}（${e.body.data.violations.length} 处校验错误）`, 'err')
    } else {
      cmxNotify('error', `保存失败：${e.message}`)
      setMsg(root, `保存失败：${e.message}`, 'err')
    }
  }
}

/** 展示列校验错误（violations）。 */
function presentViolations (violations) {
  const lines = (violations || []).slice(0, 50).map((v) => {
    const where = [v.row || v.id, v.field, v.message].filter(Boolean).map((x) => String(x)).join(' · ')
    return `• ${where || JSON.stringify(v)}`
  })
  cmxNotify('error', `校验错误（${violations.length} 处）：\n${lines.join('\n')}`)
}

/* ─────────────── 树形：渲染左侧树 ─────────────── */
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
  // 根节点（虚拟）
  const rootChildren = state.treeNodes['null'] || state.treeNodes[null] || []
  const isRootActive = state.currentParentId == null
  body.innerHTML = `
    <div class="de-tree-root ${isRootActive ? 'active' : ''}" data-node-id="__root__">▶ 全部根节点（${rootChildren.length}）</div>
    <div class="de-children" id="deTreeChildren">
      ${renderTreeNodes(rootChildren, pk)}
    </div>
  `
  // 绑定点击
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
    const code = n[state.meta.codeField]
    const meta = state.meta.codeField ? ` <span class="de-tree-meta">${escHtml(code == null ? '' : String(code))}</span>` : ''
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
  // 点击节点：切换右表父级
  body.querySelectorAll('.de-tree-node[data-node-id]').forEach((el) => {
    el.addEventListener('click', async (ev) => {
      if (ev.target && ev.target.dataset && ev.target.dataset.toggle) return // 由 toggle 处理
      const id = el.dataset.nodeId
      state.selectedTreeNodeId = id
      state.currentParentId = id
      state.page = 1
      renderTree(root)
      await reload(root)
    })
  })
  // 点击 toggle：懒加载子节点
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
  state.changes = { inserted: [], updated: [], deleted: [] }
  state.baselineMap = {}
  setMsg(root, `加载字典元数据：${dictCode} …`)
  try {
    state.meta = await loadMeta(state.def, dictCode)
  } catch (e) {
    setMsg(root, `元数据加载失败：${e.message}`, 'err')
    return
  }
  // 副标题
  const sub = root.querySelector('#deSubtitle')
  if (sub) sub.textContent = `${state.meta.dictName || ''}（${state.meta.dictCode}）${state.meta.selfHierarchy ? ' · 树形' : ' · 平级'}`
  // 重建布局：根据 selfHierarchy 切换 body class & 树显隐
  const body = root.querySelector('#deBody')
  if (state.meta.selfHierarchy) {
    body.classList.remove('flat'); body.classList.add('tree')
    if (!body.querySelector('.de-tree')) {
      const tree = document.createElement('div')
      tree.className = 'de-tree'
      tree.innerHTML = `<div class="de-tree-head">字典结构</div><div class="de-tree-body" id="deTreeBody"></div>`
      body.insertBefore(tree, body.firstChild)
    }
    // 加载根级节点
    try { await loadTreeChildren(root, null) } catch (e) { /* 忽略 */ }
    renderTree(root)
  } else {
    body.classList.remove('tree'); body.classList.add('flat')
    const tree = body.querySelector('.de-tree')
    if (tree) tree.remove()
  }
  // 重建列模型（cmx-revo-grid 无 getter，用模块级 _lastDictCode 跟踪）
  state._lastDictCode = null  // 强制 applyRowsToGrid 下次重设
  // 同步字典切换器的 selected 状态（防止初始进入与 switchDict 后 option 不同步）
  const sel = root.querySelector('#deDictSelect')
  if (sel) {
    sel.querySelectorAll('ui5-option').forEach((o) => {
      if (o.getAttribute('value') === dictCode) o.setAttribute('selected', '')
      else o.removeAttribute('selected')
    })
  }
  await reload(root)
}

/* ─────────────── 初始化 dictSelect ───────────────
 * UI5 Select 的事件名是 'change'（不是 'selection-change'），值在
 * e.detail.selectedOption.value。参照 cluster-datasource.js:1809 范式。
 */
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

/* ─────────────── 高级条件 UI ─────────────── */
function renderCondRow (root) {
  const meta = state.meta
  if (!meta) return
  const cols = editableColumns(meta)
  const colOpts = cols.map((c) => `<ui5-option value="${escAttr(c.name)}">${escHtml(colCaption(c))}</ui5-option>`).join('')
  const box = root.querySelector('#deCondBox')
  const span = document.createElement('span')
  span.className = 'de-cond'
  span.innerHTML = `
    <ui5-select data-f="col">${colOpts}</ui5-select>
    <ui5-select data-f="op"><ui5-option value="eq" selected>等于</ui5-option></ui5-select>
    <ui5-input data-f="value" placeholder="值" style="max-width:140px"></ui5-input>
    <ui5-icon name="decline" data-act="del-cond" style="cursor:pointer;width:14px;height:14px;color:var(--sapContent_LabelColor,#6a6d70)"></ui5-icon>`
  box.appendChild(span)
}

/** 从 ui5-select / ui5-input 读取值。ui5-select 须读 selectedOption.value。 */
function ui5SelectValue (sel) {
  if (!sel) return ''
  // 优先用 selectedOption
  const opt = sel.querySelector('ui5-option[selected]') || (sel.selectedOption)
  if (opt) return opt.getAttribute('value') || ''
  // 兜底：first option
  const first = sel.querySelector('ui5-option')
  return first ? (first.getAttribute('value') || '') : ''
}

function readConds (root) {
  const out = []
  root.querySelectorAll('.de-cond').forEach((el) => {
    const colEl = el.querySelector('[data-f="col"]')
    const opEl = el.querySelector('[data-f="op"]')
    const valEl = el.querySelector('[data-f="value"]')
    const col = ui5SelectValue(colEl)
    const op = ui5SelectValue(opEl) || 'eq'
    const value = valEl ? (valEl.value || '') : ''
    if (col && value) out.push({ col, op, value })
  })
  return out
}

/* ─────────────── 绑定页面事件 ───────────────
 * 注意：grid 的行事件绑定在 wireGridEvents 里，grid 重建时自动调用，不在这里绑。
 */
function bindPage (root) {
  root.querySelector('#btnReload')?.addEventListener('click', () => { state.page = 1; void reload(root) })
  root.querySelector('#btnAdd')?.addEventListener('click', () => void openEditDialog(root, 'add'))
  root.querySelector('#btnDel')?.addEventListener('click', () => void deleteSelected(root))
  root.querySelector('#btnSave')?.addEventListener('click', () => void save(root))

  // 搜索
  const search = () => {
    state.q = (root.querySelector('#deQ')?.value || '').trim()
    state.conds = readConds(root)
    state.page = 1
    void reload(root)
  }
  root.querySelector('#btnSearch')?.addEventListener('click', search)
  root.querySelector('#deQ')?.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') search() })
  root.querySelector('#btnClearCond')?.addEventListener('click', () => {
    if (root.querySelector('#deQ')) root.querySelector('#deQ').value = ''
    root.querySelector('#deCondBox').innerHTML = ''
    state.q = ''; state.conds = []; state.page = 1
    void reload(root)
  })

  // 高级条件
  root.querySelector('#btnAddCond')?.addEventListener('click', () => renderCondRow(root))
  root.querySelector('#deCondBox')?.addEventListener('click', (ev) => {
    const t = ev.target
    if (t && t.dataset && t.dataset.act === 'del-cond') t.closest('.de-cond')?.remove()
  })

  // 分页
  root.querySelector('#btnPrev')?.addEventListener('click', () => {
    if (state.page > 1) { state.page--; void reload(root) }
  })
  root.querySelector('#btnNext')?.addEventListener('click', () => {
    const totalPages = Math.max(1, Math.ceil(state.total / state.pageSize))
    if (state.page < totalPages) { state.page++; void reload(root) }
  })

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
      state.q = ''
      state.conds = []
      state.treeNodes = {}
      state.currentParentId = null
      state.selectedTreeNodeId = null
      state.rows = []
      state.total = 0
      state.grid = null
      state.changes = { inserted: [], updated: [], deleted: [] }
      state.baselineMap = {}
      state._lastDictCode = null
      state.selectedRowId = null
      state.selectedIds = []

      const def = readDef(ctx)
      if (!def) {
        return `<div style="padding:12px;color:var(--sapNegativeColor,#bb0000);font-size:13px">
          通用字典维护页缺少必要 props：需 { domain, application, module }（可选 dictCode/dbId）。
        </div>`
      }
      state.def = def

      // 先拉字典清单（用于切换器）
      try {
        state.dicts = await loadDictList(def)
      } catch (e) {
        // 失败不阻断：若 props 给了 dictCode 仍可单字典使用
        state.dicts = def.initialDictCode ? [{ dictCode: def.initialDictCode, dictName: def.initialDictCode }] : []
      }

      if (host) whenRendered(host, '.de-root', (root) => bindPage(root))
      return pageHtml()
    },
  },
}
