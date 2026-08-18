/**
 * MDM 主数据·通用列表页（native-page，元数据驱动公共页）。
 *
 * 每种主数据挂**独立菜单节点**，差异经菜单节点 props 注入（见 mdm-menu.json）：
 *   dictCode（必填，主数据字典码）/ docType（必填，CR 单据类型）/ title（必填）/
 *   entityName（可选，按钮文案）/ icon（可选）/ columns（可选，列子集/顺序）/ searchPlaceholder（可选）。
 * 列模型从 `GET /api/dct/meta?dict=…&with_props=true` 派生：默认剔除平台/审计/治理列并尊重
 * visible:false；props.columns 有值则以其为最终显示清单。数据走 `POST /api/dct/data/search`。
 *
 * 并列门户标签页（关闭互不影响）：
 *   新增   → portal.mdm.cr-form（mode=create + docType，单例）
 *   变更   → portal.mdm.cr-form（mode=update + docType + targetId）
 *   详情   → portal.mdm.master-detail（dictCode + recordId，透传 columns 保持一致）
 *
 * 多实例安全：state 按 host 隔离（WeakMap），同页多菜单节点/多 tab 并存互不串数据。
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

// 平台/审计/治理/scope/系统列默认隐藏集合（props.columns 可覆盖）。业务列 code/name/status 不在其中。
const PLATFORM_COLS = new Set([
  'id', 'sort_no',
  'create_by', 'create_time', 'update_by', 'update_time',
  'lifecycle_status', 'published_version', 'effective_date', 'effective_from', 'effective_to',
  'disabled_reason', 'disabled_time',
  'scope_type', 'entity_id', 'is_system',
  'level_no', 'full_path', 'is_leaf', 'parent_id', 'parent_code',
])

function unwrap(res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) { const e = new Error(body.msg || `业务错误 ${body.code}`); e.body = body; throw e }
    return body.data
  }
  if (!res.ok) { const e = new Error((body && body.error) || `HTTP ${res.status}`); e.status = res.status; throw e }
  return body
}
async function apiPost(url, payload, dbId) {
  const h = { 'Content-Type': 'application/json', Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const r = await fetch(url, { method: 'POST', headers: h, credentials: 'same-origin', body: JSON.stringify(payload || {}) })
  return unwrap(r, await r.json().catch(() => null))
}
async function apiGet(url, dbId) {
  const h = { Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const r = await fetch(url, { method: 'GET', headers: h, credentials: 'same-origin' })
  return unwrap(r, await r.json().catch(() => null))
}

// ── 按 host 隔离的 state（多实例安全）──────────────────────────────────────
const _hostState = new WeakMap()
function initState() {
  return {
    coord: null, dbId: '',
    dictCode: '', docType: '', title: '', entityName: '', icon: '',
    columns: null, searchPlaceholder: '',
    dictMeta: null, rows: [], kw: '', page: 1, pageSize: 20, total: 0, cfgErr: '', grid: null,
  }
}
function getState(host) { if (host && !_hostState.has(host)) _hostState.set(host, initState()); return host ? _hostState.get(host) : null }

// 坐标四元组：统一 cr-form 版本（module 回退 mdm，dbId 兼读 workspace.context）。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  return {
    domain: get('domain') || p.domain || p.domainCode || '',
    application: get('application') || p.application || p.applicationCode || '',
    module: get('module') || p.module || 'mdm',
    dbId: p.dbId || p.db_id || get('dbId') || get('db_id') || '',
  }
}
function coordQs(st, extra = {}) {
  const c = st.coord || {}
  return new URLSearchParams({ domain: c.domain || '', application: c.application || '', module: c.module || 'mdm', ...extra }).toString()
}
function coordCtx(st) {
  const c = st.coord || {}
  if (!c.domain && !c.application) return {}
  return { domain: c.domain, application: c.application, module: c.module || 'mdm', dbId: c.dbId }
}

function styleCss() {
  return `
  .pg { height:100%; overflow:hidden; display:flex; flex-direction:column; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:10px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .card { display:flex; flex-direction:column; flex:1; min-height:0;
    background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; }
  .card-hd { display:flex; justify-content:space-between; align-items:center; gap:8px; margin-bottom:10px; }
  .card-title { font-size:15px; font-weight:600; color:var(--sapTitleColor); }
  .tbl-wrap { flex:1; min-height:0; overflow:hidden; display:flex; flex-direction:column; margin-top:10px; }
  .tbl-wrap cmx-revo-grid { display:flex; width:100%; flex:1 1 0%; min-width:0; min-height:0; flex-direction:column; }
  .cfg-err { padding:24px; color:var(--sapNegativeTextColor,#b00); font-size:13px; }
  cmx-toolbar, cmx-filter-bar { display:block; }
  `
}

// 打开并列门户标签页。key 含 recordId/targetId（多开去重），single=单例。
function openTab(host, st, caption, nativePage, context, opts = {}) {
  let app = null
  try { app = document.querySelector('cmx-portal-app') } catch { app = null }
  if (!app || typeof app.openNode !== 'function') {
    let n = host
    for (let i = 0; i < 6 && n; i++) {
      if (typeof n.openNode === 'function') { app = n; break }
      const r = n.getRootNode && n.getRootNode(); n = r && r.host
    }
  }
  if (!app || typeof app.openNode !== 'function') { console.warn('[master-list] 未找到 portal-app.openNode'); return }
  const ctxKey = (context && (context.crId || context.recordId || (context.target && context.target.id) || context.targetId)) || ''
  const key = opts.single ? 'single' : (ctxKey || Date.now())
  const c = st.coord || {}
  app.openNode({
    id: `${nativePage}-${key}`, name: nativePage, caption, type: 'workspace-node',
    domainCode: c.domain || '', applicationCode: c.application || '',
    workspace: { content: { caption, views: [{ type: 'native_pages', native_page: nativePage, view: 'content' }] } },
  }, { initialContext: context })
}

// ── 元数据与列模型 ────────────────────────────────────────────────────────
async function loadDictMeta(st) {
  const m = await apiGet(`/api/dct/meta?${coordQs(st, { dict: st.dictCode })}&with_props=true`, st.dbId)
  return (m && m.columns) ? m : null
}
// 全量列 → 显示列：props.columns 为最终清单；否则默认过滤平台列 + visible!==false。
function buildColumns(st) {
  const C = cmx()
  if (!C.metaTableFieldsToColumns || !st.dictMeta) return []
  const c = st.coord || {}
  let cols = C.metaTableFieldsToColumns(st.dictMeta.columns || [], {
    kind: 'DCT', pk: st.dictMeta.pk, codeField: st.dictMeta.codeField, selfHierarchy: st.dictMeta.selfHierarchy,
    parentField: st.dictMeta.parentField, dictCode: st.dictMeta.dictCode, labelField: st.dictMeta.labelField,
    domain: c.domain, application: c.application, module: c.module,
  }, {
    respectOrder: true,
    coord: { domain: c.domain, application: c.application, module: c.module, ...(c.dbId ? { dbId: c.dbId } : {}) },
  })
  if (Array.isArray(st.columns) && st.columns.length) {
    cols = st.columns.map((id) => cols.find((col) => col.id === id)).filter(Boolean)
  } else {
    cols = cols.filter((col) => !PLATFORM_COLS.has(col.id) && col.visible !== false)
  }
  return cols
}
// docType 启动校验：须存在 source_doc_type=docType 的激活映射（配置脱节快速暴露）。
async function validateConfig(st) {
  if (!st.docType) { st.cfgErr = '缺少 props.docType（CR 单据类型）'; return }
  const list = (await apiGet(`/api/mdm/activations?targetDict=${encodeURIComponent(st.dictCode)}`, st.dbId)) || []
  const hit = list.some((a) => a.source_doc_type === st.docType)
  if (!hit) st.cfgErr = `未找到 dictCode=${st.dictCode} 且 source_doc_type=${st.docType} 的激活映射，请先在「激活映射配置器」配置。`
}

// 分页加载（POST /api/dct/data/search，遵循 AGENTS.md：列表分页走 body）
async function loadRows(st) {
  if (!st.coord || !st.dictCode) { st.rows = []; st.total = 0; return }
  const d = (await apiPost(`/api/dct/data/search?${coordQs(st, { dict: st.dictCode })}`, {
    page: st.page, pageSize: st.pageSize, q: st.kw || '',
    sort: { field: 'create_time', order: 'desc' },
  }, st.dbId)) || {}
  st.rows = d.rows || []
  st.total = Number(d.total) || 0
}

// 搜索占位：props.searchPlaceholder 优先，否则按 labelField/codeField caption 拼。
function placeholderOf(st) {
  if (st.searchPlaceholder) return st.searchPlaceholder
  const dm = st.dictMeta || {}
  const capOf = (fid) => {
    const col = (dm.columns || []).find((x) => x.id === fid)
    const cap = col && col.caption
    return (cap && (cap.zh_CN || cap)) || (col && col.name) || fid
  }
  const parts = []
  if (dm.labelField) parts.push(capOf(dm.labelField))
  if (dm.codeField && dm.codeField !== dm.labelField) parts.push(capOf(dm.codeField))
  return parts.length ? parts.join('/') : '搜索'
}

function viewHtml(st) {
  const ent = st.entityName || st.title || '主数据'
  if (st.cfgErr) return `<div class="pg"><div class="pg-head"><div class="pg-title">${esc(st.title || '主数据列表')}</div></div><div class="card"><div class="cfg-err">⚠ ${esc(st.cfgErr)}</div></div></div>`
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">${esc(st.title || '主数据列表')}</div>
      <div class="pg-sub">浏览已发布${esc(ent)}；新增/变更/详情以并列标签页打开</div></div>
    <div class="card">
      <div class="card-hd"><div class="card-title" id="mlTotal">${esc(st.title || '主数据列表')}（共 ${st.total} 条）</div>
        <cmx-toolbar><ui5-button design="Emphasized" icon="add" id="mlAdd">新增${esc(ent)}</ui5-button><ui5-button design="Transparent" icon="refresh" slot="actions" id="mlReload">刷新</ui5-button></cmx-toolbar></div>
      <cmx-filter-bar id="mlFilter" search-placeholder="${esc(placeholderOf(st))}"></cmx-filter-bar>
      <div class="tbl-wrap"><cmx-revo-grid id="mlGrid"></cmx-revo-grid></div>
      <cmx-pager id="mlPager" page-size="20" page-sizes="10,20,50,100"></cmx-pager>
    </div></div>`
}
function esc(s) { return String(s ?? '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])) }

// 列表 grid（元数据列 + 操作列）：仅建列模型与事件（bind 时一次）；
// 数据填充由 applyData 负责——页面局部更新，不整页重绘（保留输入框文字/焦点/滚动/列宽）。
function buildListGrid(host) {
  const st = getState(host); if (!st) return
  const C = cmx(); const root = host && (host.renderRoot || host.shadowRoot)
  const wrap = root && root.querySelector('.tbl-wrap'); if (!wrap) return
  // 复用模板里的 grid 壳（.tbl-wrap 内唯一），仅配列模型/选项/事件——不新建，避免双框。
  const grid = wrap.querySelector('cmx-revo-grid')
  if (!grid) return
  grid.setAttribute('data-cmx-fill-height', '')
  grid.setAttribute('data-cmx-options', '{"editable":false,"showTotals":false,"showRequiredMark":false}')
  grid.classList.add('cmx-grid-neo')
  st.grid = grid
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'master-list' })
    const cols = buildColumns(st)
    cols.push(new C.CmxColumn({ id: '_action', caption: '操作', dataType: 'VARCHAR', width: '150px', frozen: 'right', edit: { mode: 'readonly' },
      display: { mode: 'actions', actions: [
        { text: '查看详情', actionRef: 'view', icon: 'show' },
        { text: '变更', actionRef: 'edit', icon: 'edit' },
      ] } }))
    cm.setMembers(cols)
    grid.setColumnModel(cm)
  }
  grid.setOptions?.({ selectionMode: 'none', fillHeight: true, showRowIndex: true, showTotals: false })
  grid.addEventListener('cmx-cell-link-click', (e) => {
    const d = e.detail || {}; const ds = grid._ds
    const row = (ds && ds.rows && !isNaN(parseInt(d.rowId, 10))) ? ds.rows[parseInt(d.rowId, 10)] : null
    const rec = row ? (row.toPlainObject ? row.toPlainObject() : row) : null
    if (!rec) return
    const s = getState(host); if (!s) return
    const label = rec[(s.dictMeta && s.dictMeta.labelField) || 'name'] || ''
    if (d.actionRef === 'view') openTab(host, s, `${s.entityName || ''}·${label}`, 'portal.mdm.master-detail', { dictCode: s.dictCode, recordId: rec.id, title: s.title, icon: s.icon, columns: s.columns, ...coordCtx(s) })
    else if (d.actionRef === 'edit') openTab(host, s, `变更·${label}`, 'portal.mdm.cr-form', { mode: 'update', docType: s.docType, crType: 'update', targetId: rec.id, targetName: label, ...coordCtx(s) })
  })
}

// 数据落地（局部更新）：只动 total 文案、grid 数据、pager 属性——DOM/事件/焦点/滚动/列宽全保留。
// first=true（bind 后首帧）双 rAF 等 grid 布局就绪再填，其后直接填。
function applyData(host, first = false) {
  const st = getState(host); if (!st) return
  const C = cmx()
  const root = host && (host.renderRoot || host.shadowRoot); if (!root) return
  const t = root.querySelector('#mlTotal')
  if (t) t.textContent = `${st.title || '主数据列表'}（共 ${st.total} 条）`
  const pager = root.querySelector('#mlPager')
  if (pager) { pager.total = st.total; pager.page = st.page; pager.pageSize = st.pageSize }
  const grid = st.grid
  if (!grid) return
  const fill = () => {
    if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows(st.rows); grid.setDataSet(ds) }
    else grid.setDataSet?.(st.rows)
    grid.refreshLayout?.()
  }
  if (first) requestAnimationFrame(() => requestAnimationFrame(fill))
  else fill()
}

function bind(host, root) {
  const st = getState(host); if (!st) return
  root.querySelector('#mlAdd')?.addEventListener('click', () => openTab(host, st, `新增${st.entityName || ''}`, 'portal.mdm.cr-form', { mode: 'create', docType: st.docType, crType: 'create', ...coordCtx(st) }, { single: true }))
  root.querySelector('#mlReload')?.addEventListener('click', () => { loadRows(st).then(() => applyData(host)) })
  root.querySelector('#mlFilter')?.addEventListener('cmx-filter-search', (e) => { st.kw = e.detail?.text || ''; st.page = 1; loadRows(st).then(() => applyData(host)) })
  root.querySelector('#mlFilter')?.addEventListener('cmx-filter-reset', () => { st.kw = ''; st.page = 1; loadRows(st).then(() => applyData(host)) })
  const pager = root.querySelector('#mlPager')
  if (pager) {
    pager.addEventListener('page-change', (e) => {
      const d = e.detail || {}
      if (d.pageSize && d.pageSize !== st.pageSize) { st.pageSize = d.pageSize; st.page = 1 }
      else st.page = d.page || 1
      loadRows(st).then(() => applyData(host))
    })
  }
  buildListGrid(host)
  applyData(host, true)
}
function whenRendered(host, sel, cb, t) {
  const n = t == null ? 60 : t
  const root = host && (host.renderRoot || host.shadowRoot)
  if (root && root.querySelector(sel)) { cb(root); return }
  if (n <= 0) return
  requestAnimationFrame(() => whenRendered(host, sel, cb, n - 1))
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host
      const p = (ctx && ctx.props) || {}
      const st = getState(host)
      st.coord = readCoord(ctx)
      st.dbId = st.coord.dbId || p.dbId || p.db_id || ''
      st.dictCode = p.dictCode || ''
      st.docType = p.docType || ''
      st.title = p.title || '主数据列表'
      st.entityName = p.entityName || ''
      st.icon = p.icon || ''
      st.columns = Array.isArray(p.columns) ? p.columns : null
      st.searchPlaceholder = p.searchPlaceholder || ''
      try {
        if (!st.dictCode) { st.cfgErr = '缺少 props.dictCode（主数据字典码）' }
        else {
          await validateConfig(st)
          if (!st.cfgErr) { st.dictMeta = await loadDictMeta(st); await loadRows(st) }
        }
      } catch (e) { st.cfgErr = `初始化失败：${e.message}`; console.error('[master-list] init fail', e) }
      if (host && !st.cfgErr) whenRendered(host, '.pg', (r) => bind(host, r))
      return `<style>${styleCss()}</style>${viewHtml(st)}`
    },
  },
}
