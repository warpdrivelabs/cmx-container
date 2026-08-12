/**
 * MDM 供应商主数据·列表页（native-page）。
 *
 * 列表与详情/新增/编辑为**并列门户标签页**（关闭一个不影响另一个）：
 *   新增供应商 → 打开 portal.mdm.cr-form（mode=create）
 *   变更     → 打开 portal.mdm.cr-form（mode=update + supplier）
 *   查看详情  → 打开 portal.mdm.supplier-detail（supplier）
 * 经 portal-app.openNode(node,{initialContext}) 开新 tab；目标页用 host.workspace.context.get() 读参。
 * 表格铺满屏幕（flex 列 + tbl-wrap flex:1）。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

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

const state = { suppliers: [], kw: '', page: 1, pageSize: 20, total: 0 }
// 供应商的 CR 单据类型（= activation.source_doc_type，与激活映射配置保持一致；
// cr-form 据此定位 activation 配置，渲染对应字段）
const DOC_TYPE = 'gys'

// 字典坐标四元组（domain/application/module/dbId），全部来自 ctx.props，代码中不写死。
let coord = null
function coordQs(extra = {}) {
  if (!coord) return new URLSearchParams(extra).toString()
  return new URLSearchParams({
    domain: coord.domain, application: coord.application, module: coord.module, ...extra,
  }).toString()
}
let rootEl = null

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
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { position:sticky; top:0; text-align:left; padding:9px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); background:var(--sapList_Background); }
  .tbl td { padding:9px 12px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-toolbar, cmx-filter-bar { display:block; }
  `
}

/**
 * 打开并列门户标签页。
 * @param {object} opts.single true=单例（重复点击复用/聚焦同一 tab，如「新增」）；
 *   false=按 context 的 crId/supplier.id 作为 tab id（不同行可多开，如「详情」）。
 * addTab 按 id 去重：同 id 复用并同步 context，不同 id 新开。
 */
function openTab(host, caption, nativePage, context, opts = {}) {
  let app = null
  try { app = document.querySelector('cmx-portal-app') } catch { app = null }
  if (!app || typeof app.openNode !== 'function') {
    let n = host
    for (let i = 0; i < 6 && n; i++) {
      if (typeof n.openNode === 'function') { app = n; break }
      const r = n.getRootNode && n.getRootNode(); n = r && r.host
    }
  }
  if (!app || typeof app.openNode !== 'function') { console.warn('[cr-editor] 未找到 portal-app.openNode'); return }
  const ctxKey = (context && (context.crId || (context.target && context.target.id) || context.supplierId)) || ''
  const key = opts.single ? 'single' : (ctxKey || Date.now())
  app.openNode({
    id: `${nativePage}-${key}`, name: nativePage, caption, type: 'workspace-node',
    // 带上域/应用（来自当前页 ctx.props，不写死）：F5 重建动态页时据此切换左侧菜单与右上角域。
    // 用 camelCase（domainCode）与 menu-cache 标准化一致，openNode 注入 workspace.context 也用此名。
    domainCode: (coord && coord.domain) || '', applicationCode: (coord && coord.application) || '',
    workspace: { content: { caption, views: [{ type: 'native_pages', native_page: nativePage, view: 'content' }] } },
  }, { initialContext: context })
}

// 分页加载供应商（POST /api/dct/data/search，遵循 AGENTS.md：列表分页走 body）
async function loadSuppliers() {
  if (!coord) { state.suppliers = []; state.total = 0; return }
  const d = (await apiPost(`/api/dct/data/search?${coordQs({ dict: 'supplier' })}`, {
    page: state.page, pageSize: state.pageSize, q: state.kw || '',
    sort: { field: 'create_time', order: 'desc' },
  }, coord.dbId)) || {}
  state.suppliers = d.rows || []
  state.total = Number(d.total) || 0
}

function viewHtml() {
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">供应商列表</div>
      <div class="pg-sub">浏览已发布供应商；新增/变更/详情以并列标签页打开</div></div>
    <div class="card">
      <div class="card-hd"><div class="card-title">供应商列表（共 ${state.total} 条）</div>
        <cmx-toolbar><ui5-button design="Emphasized" icon="add" id="ceAdd">新增供应商</ui5-button><ui5-button design="Transparent" icon="refresh" slot="actions" id="ceReload">刷新</ui5-button></cmx-toolbar></div>
      <cmx-filter-bar id="ceFilter" search-placeholder="名称/编码/信用代码"></cmx-filter-bar>
      <div class="tbl-wrap"><cmx-revo-grid id="ceGrid"></cmx-revo-grid></div>
      <cmx-pager id="cePager" page-size="20" page-sizes="10,20,50,100"></cmx-pager>
    </div></div>`
}

// 供应商列表用 cmx-revo-grid（只读 + 操作列）。每次 bind 重建（refresh 会重渲染 DOM）。
function buildListGrid() {
  const C = cmx(); const wrap = rootEl && rootEl.querySelector('.tbl-wrap'); if (!wrap) return
  const old = wrap.querySelector('cmx-revo-grid'); if (old) old.remove()
  const grid = document.createElement('cmx-revo-grid')
  // 主内容区列表页：套 Neo 皮肤（cmx-grid-neo）+ 声明式 fill-height，与设计器列表页风格一致。
  // 不用 data-cmx-embed（那是 combo/dict 弹层内嵌场景，会跳过 Neo 皮肤导致朴素灰白外观）。
  grid.setAttribute('data-cmx-fill-height', '')
  grid.setAttribute('data-cmx-options', '{"editable":false,"showTotals":false,"showRequiredMark":false}')
  grid.classList.add('cmx-grid-neo')
  wrap.appendChild(grid)
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'suppliers' })
    cm.setMembers([
      new C.CmxColumn({ id: 'code', caption: '编码', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'name', caption: '名称', dataType: 'VARCHAR', width: '180px' }),
      new C.CmxColumn({ id: 'tax_no', caption: '税号', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'credit_code', caption: '信用代码', dataType: 'VARCHAR', width: '180px' }),
      new C.CmxColumn({ id: 'short_name', caption: '简称', dataType: 'VARCHAR', width: '120px' }),
      new C.CmxColumn({ id: 'published_version', caption: '版本', dataType: 'INT', width: '70px',display: { mode: 'text' } }),
      new C.CmxColumn({ id: '_action', caption: '操作', dataType: 'VARCHAR', width: '150px', frozen: 'right', edit: { mode: 'readonly' },
        display: { mode: 'actions', actions: [
          { text: '查看详情', actionRef: 'view', icon: 'show' },
          { text: '变更', actionRef: 'edit', icon: 'edit' },
        ] } }),
    ])
    grid.setColumnModel(cm)
  }
  grid.setOptions?.({ selectionMode: 'none', fillHeight: true, showRowIndex: true, showTotals: false })
  // 操作列点击：rowId 为 revo 行索引，反查真实行
  grid.addEventListener('cmx-cell-link-click', (e) => {
    const d = e.detail || {}; const ds = grid._ds
    const row = (ds && ds.rows && !isNaN(parseInt(d.rowId, 10))) ? ds.rows[parseInt(d.rowId, 10)] : null
    const s = row ? (row.toPlainObject ? row.toPlainObject() : row) : null
    if (!s) return
    if (d.actionRef === 'view') openTab(currentHost, `供应商·${s.name || ''}`, 'portal.mdm.supplier-detail', { supplierId: s.id, supplierName: s.name, domain: coord && coord.domain, application: coord && coord.application, module: (coord && coord.module) || 'mdm', dbId: coord && coord.dbId })
    else if (d.actionRef === 'edit') openTab(currentHost, `变更·${s.name || ''}`, 'portal.mdm.cr-form', { docType: DOC_TYPE, crType: 'update', target: s })
  })
  const fill = () => {
    if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows(state.suppliers); grid.setDataSet(ds) }
    else grid.setDataSet?.(state.suppliers)
    grid.refreshLayout?.()
  }
  requestAnimationFrame(() => requestAnimationFrame(fill))
  listGrid = grid
}
let listGrid = null

// 事件委托挂在 document（模块在主 realm 执行；composed 事件必冒泡到 document），
// 在 content() 里立即挂载，不依赖 DOM 挂载时机，规避 shadow 内逐个绑定/whenRendered 时机失效。
// 逐元素绑定（与 activation-mapper 相同的成熟模式）
function bind(root) {
  rootEl = root
  const host = currentHost
  // 新增=单例（只开一个）；详情/变更=按行 id 多开
  root.querySelector('#ceAdd')?.addEventListener('click', () => openTab(host, '新增供应商', 'portal.mdm.cr-form', { docType: DOC_TYPE, crType: 'create' }, { single: true }))
  root.querySelector('#ceReload')?.addEventListener('click', () => { loadSuppliers().then(refresh) })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-search', (e) => { state.kw = e.detail?.text || ''; state.page = 1; loadSuppliers().then(refresh) })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-reset', () => { state.kw = ''; state.page = 1; loadSuppliers().then(refresh) })
  // 分页（cmx-pager 独立模式）
  const pager = root.querySelector('#cePager')
  if (pager) {
    pager.total = state.total; pager.page = state.page; pager.pageSize = state.pageSize
    pager.addEventListener('page-change', (e) => {
      const d = e.detail || {}
      if (d.pageSize && d.pageSize !== state.pageSize) { state.pageSize = d.pageSize; state.page = 1 }
      else state.page = d.page || 1
      loadSuppliers().then(refresh)
    })
  }
  buildListGrid()
}

function refresh() {
  const host = currentHost; if (!host) return
  const root = host.renderRoot || host.shadowRoot; if (!root) return
  root.innerHTML = `<style>${styleCss()}</style>${viewHtml()}`
  bind(root)
}
let currentHost = null
function whenRendered(host, sel, cb, t) {
  const n = t == null ? 60 : t
  const root = host && (host.renderRoot || host.shadowRoot)
  if (root && root.querySelector(sel)) { cb(root); return }
  if (n <= 0) return
  requestAnimationFrame(() => whenRendered(host, sel, cb, n - 1))
}

// 从 workspace.context（框架 openNode 注入）或 ctx.props 读取字典坐标四元组（不写死默认值）；
// 缺 domain/application/module 返回 null。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  const c = {
    domain: get('domain') || p.domain || '',
    application: get('application') || p.application || '',
    module: get('module') || p.module || '',
    dbId: p.dbId || p.db_id || '',
  }
  return (c.domain && c.application && c.module) ? c : null
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      coord = readCoord(ctx)
      try { await loadSuppliers() } catch (e) { console.error('[cr-editor] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
