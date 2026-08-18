/**
 * MDM 单据列表台（native-page · 企业级重设计）——通用页，按菜单 props 参数化：
 *   docType  过滤单据类型（= 激活映射 source_doc_type，如 gys）；缺省=全类型
 *   title    页面标题（缺省「单据列表」）
 *
 * 布局：页头 → 列表面板（cmx-filter-bar + 企业表格 + 行内操作）→ 详情整页（cr-form）。
 * 纯发起人视角：提交 / 撤回 / 驳回重提 / 作废；审批办理在流程待办中心，本页不承载。
 * 提示统一 cmxInfo/cmxWarn/cmxError/cmxConfirm（禁 alert/confirm/prompt）。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

function unwrap(res, body) {
  // 后端错误响应有两种字段名：ApiResp 用 msg，cmx_api_types::Error 用 error；两者都兼容。
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) { const e = new Error(body.msg || body.error || `业务错误 ${body.code}`); e.body = body; throw e }
    return body.data
  }
  if (!res.ok) { const e = new Error((body && (body.msg || body.error)) || `HTTP ${res.status}`); e.status = res.status; throw e }
  return body
}
async function apiGet(url, dbId) {
  const h = { Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const r = await fetch(url, { headers: h, credentials: 'same-origin' })
  return unwrap(r, await r.json().catch(() => null))
}
async function apiPost(url, payload, dbId) {
  const h = { 'Content-Type': 'application/json', Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const r = await fetch(url, { method: 'POST', headers: h, credentials: 'same-origin', body: JSON.stringify(payload || {}) })
  return unwrap(r, await r.json().catch(() => null))
}

// 详情/编辑为整页+面包屑（不用弹框），便于后续叠加流程展示。
let rootEl = null

const STATUS_META = {
  draft: { name: '草稿', tone: 'neutral' },
  approving: { name: '审批中', tone: 'warning' },
  activating: { name: '激活中', tone: 'info' },
  approved: { name: '已通过', tone: 'info' },
  activated: { name: '已激活', tone: 'success' },
  rejected: { name: '已驳回', tone: 'danger' },
  aborted: { name: '已作废', tone: 'neutral' },
}
const state = { dbId: '', docType: '', title: '单据列表', filter: 'all', keyword: '', list: [], domain: '', application: '', page: 1, pageSize: 20, total: 0 }

function styleCss() {
  return `
  .pg { height:100%; overflow:hidden; display:flex; flex-direction:column; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  /* 列表卡片撑满剩余高度，仅表格内部滚动 */
  .list-card { display:flex; flex-direction:column; flex:1; min-height:0;
    background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; }
  .tbl-wrap { flex:1; min-height:0; overflow:hidden; display:flex; flex-direction:column; margin-top:10px; }
  .tbl-wrap cmx-revo-grid { display:flex; width:100%; flex:1 1 0%; min-width:0; min-height:0; flex-direction:column; }
  .tbl th { position:sticky; top:0; }
  .crumb { display:flex; align-items:center; gap:6px; font-size:13px; margin-bottom:10px; color:var(--sapContent_LabelColor); }
  .crumb a { color:var(--sapLinkColor,#0a6ed1); cursor:pointer; }
  .crumb .cur { color:var(--sapTitleColor); font-weight:600; }
  .card { background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; margin-bottom:12px; }
  .card-title { font-size:14px; font-weight:600; color:var(--sapTitleColor); margin-bottom:8px; }
  .pg-head { margin-bottom:14px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:10px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); background:var(--sapList_HeaderBackground,transparent); }
  .tbl td { padding:10px 12px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-panel, cmx-toolbar, cmx-filter-bar { display:block; }
  .mask { position:fixed; inset:0; background:rgba(0,0,0,.45); display:flex; align-items:center; justify-content:center; z-index:999; }
  .dlg { width:720px; max-height:82vh; overflow:auto; border-radius:10px; padding:20px;
    background:var(--sapList_Background); color:var(--sapTextColor); border:1px solid var(--sapList_BorderColor); }
  .dlg h3 { margin:0 0 14px; font-size:16px; color:var(--sapTitleColor); }
  .dlg .sec { margin:16px 0 8px; font-size:13px; font-weight:600; color:var(--sapTitleColor); }
  `
}

function actionsHtml(r) {
  const id = r.id; const s = r.doc_status
  const b = (act, design, icon, text) => `<ui5-button design="${design}" icon="${icon}" data-act="${act}" data-id="${id}">${text}</ui5-button>`
  if (s === 'draft') return b('submit', 'Default', 'paper-plane', '提交') + b('abort', 'Transparent', 'cancel', '作废')
  if (s === 'approving') return b('approve', 'Emphasized', 'accept', '通过') + b('reject', 'Transparent', 'decline', '驳回')
  if (s === 'rejected') return b('resubmit', 'Default', 'edit', '修改重提')
  return b('view', 'Transparent', 'show', '查看')
}

function fmtTime(t) { if (!t) return ''; const s = String(t); return s.length > 19 ? s.slice(0, 19).replace('T', ' ') : s }

// function tableHtml() {
//   const rows = filtered()
//   if (!rows.length) {
//     return `<cmx-empty-state icon="document" title="暂无变更申请" description="调整过滤条件或到录入台新建申请"></cmx-empty-state>`
//   }
//   const trs = rows.map((r) => {
//     const m = STATUS_META[r.doc_status] || { name: r.doc_status, tone: 'neutral' }
//     return `<tr>
//       <td class="muted">${r.id}</td><td>${r.doc_no || ''}</td><td>${r.subject_name || ''}</td><td>${r.cr_type || ''}</td>
//       <td><cmx-status-tag tone="${m.tone}" variant="subtle" dot size="sm">${m.name}</cmx-status-tag></td>
//       <td class="muted">${fmtTime(r.create_time)}</td><td>${actionsHtml(r)}</td></tr>`
//   }).join('')
//   return `<table class="tbl"><thead><tr><th>ID</th><th>单据号</th><th>名称</th><th>类型</th><th>状态</th><th>创建时间</th><th>操作</th></tr></thead><tbody>${trs}</tbody></table>`
// }

function viewHtml() {
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">${state.title}</div>
      <div class="pg-sub">提交 / 撤回 / 驳回重提 / 作废；审批通过后自动激活落字典</div></div>
    <div class="list-card">
      <div class="card-title" id="ctTotal">申请列表（共 ${state.total} 条）</div>
      <cmx-filter-bar id="ctFilter" search-placeholder="单据号/名称">
        <ui5-select id="ctStatus">
          <ui5-option value="all" ${state.filter === 'all' ? 'selected' : ''}>全部</ui5-option>
          <ui5-option value="draft" ${state.filter === 'draft' ? 'selected' : ''}>草稿</ui5-option>
          <ui5-option value="approving" ${state.filter === 'approving' ? 'selected' : ''}>待审批</ui5-option>
          <ui5-option value="rejected" ${state.filter === 'rejected' ? 'selected' : ''}>已驳回</ui5-option>
          <ui5-option value="activated" ${state.filter === 'activated' ? 'selected' : ''}>已激活</ui5-option>
          <ui5-option value="aborted" ${state.filter === 'aborted' ? 'selected' : ''}>已作废</ui5-option>
        </ui5-select>
        <ui5-button slot="actions" design="Transparent" icon="refresh" id="ctReload">刷新</ui5-button>
      </cmx-filter-bar>
      <div class="tbl-wrap"><cmx-revo-grid id="ctGrid"></cmx-revo-grid></div>
      <cmx-pager id="ctPager" page-size="20" page-sizes="10,20,50,100"></cmx-pager>
    </div>
  </div>`
}

// 单据列表用 cmx-revo-grid（只读 + 操作列）。操作列走 display.mode='actions'，
// 按钮按 doc_status 通过 visible(model) 显隐；点击派发 cmx-cell-link-click（与 master-list 同模式）。
// 仅建列模型与事件（bind 时一次）；数据填充由 applyData 负责——页面局部更新，不整页重绘。
let listGrid = null
function buildListGrid() {
  const C = cmx(); const wrap = rootEl && rootEl.querySelector('.tbl-wrap'); if (!wrap) return
  // 复用模板里的 grid 壳（.tbl-wrap 内唯一），仅配列模型/选项/事件——不新建，避免双框。
  const grid = wrap.querySelector('cmx-revo-grid')
  if (!grid) return
  // 主内容区列表页：套 Neo 皮肤（cmx-grid-neo）+ 声明式 fill-height，与设计器列表页风格一致。
  // 不用 data-cmx-embed（那是 combo/dict 弹层内嵌场景，会跳过 Neo 皮肤导致朴素灰白外观）。
  grid.setAttribute('data-cmx-fill-height', '')
  grid.setAttribute('data-cmx-options', '{"editable":false,"showTotals":false,"showRequiredMark":false}')
  grid.classList.add('cmx-grid-neo')
  listGrid = grid
  const is = (s) => (m) => m.doc_status === s
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'crList' })
    cm.setMembers([
      new C.CmxColumn({ id: 'id', caption: 'ID', dataType: 'VARCHAR', width: '110px' }),
      new C.CmxColumn({ id: 'doc_no', caption: '单据号', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'subject_name', caption: '数据名称', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'remark', caption: '业务事由', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'status_name', caption: '状态', dataType: 'VARCHAR', width: '80px' }),
      new C.CmxColumn({ id: 'create_time', caption: '创建时间', dataType: 'VARCHAR', width: '150px', display: {
        mode: 'text', format: 'datetime:YYYY-MM-DD HH:mm:ss', align: 'center',
      } }),
      new C.CmxColumn({ id: '_action', caption: '操作', dataType: 'VARCHAR', width: '200px', frozen: 'right', edit: { mode: 'readonly' },
        display: { mode: 'actions', actions: [
          // M7：审批动作上收流程待办中心（mdm_approver 候选池），本页仅保留业务视角操作。
          { text: '详情', actionRef: 'view', icon: 'detail-view' },
          { text: '提交',   actionRef: 'submit',  visible: is('draft') },
          { text: '作废',   actionRef: 'abort',   variant: 'negative', visible: is('draft') },
          { text: '撤回',   actionRef: 'withdraw', variant: 'negative', visible: is('approving') },
          { text: '修改重提', actionRef: 'resubmit',  visible: is('rejected') },
        ] } }),
    ])
    grid.setColumnModel(cm)
  }
  grid.setOptions?.({ selectionMode: 'none', fillHeight: true, showRowIndex: true, showTotals: false, allowTextSelect: true, resize: true })
  // 操作列点击：rowId 为 revo 行索引，反查真实行
  grid.addEventListener('cmx-cell-link-click', (e) => {
    const d = e.detail || {}; const ds = grid._ds
    const row = (ds && ds.rows && !isNaN(parseInt(d.rowId, 10))) ? ds.rows[parseInt(d.rowId, 10)] : null
    if (!row) return
    const r = row.toPlainObject ? row.toPlainObject() : row
    if (r.id == null) return
    doAction(d.actionRef, String(r.id))
  })
}

// 数据落地（局部更新）：只动 total 文案、grid 数据、pager 属性——DOM/事件/焦点/滚动/列宽全保留。
// first=true（bind 后首帧）双 rAF 等 grid 布局就绪再填，其后直接填。
function applyData(first = false) {
  const C = cmx()
  const t = rootEl && rootEl.querySelector('#ctTotal')
  if (t) t.textContent = `申请列表（共 ${state.total} 条）`
  const pager = rootEl && rootEl.querySelector('#ctPager')
  if (pager) { pager.total = state.total; pager.page = state.page; pager.pageSize = state.pageSize }
  const rows = state.list.map((r) => ({ ...r, status_name: (STATUS_META[r.doc_status] || {}).name || r.doc_status }))
  const grid = listGrid
  if (!grid) return
  const fill = () => {
    if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows(rows); grid.setDataSet(ds) }
    else grid.setDataSet?.(rows)
    grid.refreshLayout?.()
  }
  if (first) requestAnimationFrame(() => requestAnimationFrame(fill))
  else fill()
}

// ── 操作 ─────────────────────────────────────────────────────────────────────
async function doAction(act, id) {
  const crId = Number(id); const M = cmx()
  try {
    if (act === 'submit') {
      const ok = await M.cmxConfirm?.({ title: '提交审批', message: `确认提交 CR-${crId}？提交后进入流程审批。`, danger: false })
      if (ok === false) return
      await apiPost('/api/mdm/change-requests/submit', { crId }, state.dbId); M.cmxInfo?.(`CR-${crId} 已提交`)
    }
    else if (act === 'withdraw') {
      // 撤回（发起人专属，后端校验）：终止当前审批实例 + CR 回草稿，修改后重提发新实例。
      const ok = await M.cmxConfirm?.({ title: '撤回申请', message: `确认撤回 CR-${crId}？当前审批将终止，单据回到草稿可修改后重新提交。`, danger: true })
      if (ok === false) return
      await apiPost('/api/mdm/change-requests/withdraw', { crId }, state.dbId)
      M.cmxInfo?.(`CR-${crId} 已撤回，回到草稿`)
    } else if (act === 'resubmit') {
      // 修改重提：驳回后在「原单据」上直接编辑重新提交——后端 submit 支持 rejected→approving，
      // 无需 clone 新 CR。打开原单据 view 页并 autoEdit 直接进编辑态；cr-form 按 rejected 状态显示编辑/提交。
      openTab(currentHost, `单据·CR-${crId}`, 'portal.mdm.cr-form',
        { mode: 'view', crId, autoEdit: true, domain: state.domain, application: state.application, module: 'mdm', dbId: state.dbId })
      return
    } else if (act === 'abort') {
      const ok = await M.cmxConfirm?.({ title: '作废', message: `确认作废 CR-${crId}？`, danger: true })
      if (ok === false) return
      await apiPost('/api/mdm/change-requests/abort', { crId }, state.dbId); M.cmxInfo?.(`CR-${crId} 已作废`)
    } else if (act === 'view') { openTab(currentHost, `单据·CR-${crId}`, 'portal.mdm.cr-form', { mode: 'view', crId, domain: state.domain, application: state.application, module: 'mdm', dbId: state.dbId }); return }
    await load(); applyData()
  } catch (e) { cmx().cmxError?.(`操作失败：${e.message}`) }
}

/**
 * 打开并列门户标签页。opts.single=true 单例复用；默认按 context.crId 多开（不同行多个详情 tab）。
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
  if (!app || typeof app.openNode !== 'function') { console.warn('[cr-todo] 未找到 portal-app.openNode'); return }
  const ctxKey = (context && context.crId) || ''
  const key = opts.single ? 'single' : (ctxKey || Date.now())
  app.openNode({
    id: `${nativePage}-${key}`, name: nativePage, caption, type: 'workspace-node',
    // 域/应用取自当前页 ctx.props（不写死）：F5 重建动态页时据此切换左侧菜单与右上角域
    domain_code: state.domain, application_code: state.application,
    workspace: { content: { caption, views: [{ type: 'native_pages', native_page: nativePage, view: 'content' }] } },
  }, { initialContext: context })
}

async function load() {
  const params = { page: state.page, pageSize: state.pageSize }
  if (state.filter !== 'all') params.docStatus = state.filter
  if (state.docType) params.docType = state.docType
  if (state.keyword) params.keyword = state.keyword
  const d = (await apiGet(`/api/mdm/change-requests?${new URLSearchParams(params)}`, state.dbId)) || {}
  state.list = d.list || []
  state.total = Number(d.total) || 0
}

function bind(root) {
  rootEl = root
  const reload = async () => { await load(); applyData() }
  root.querySelector('#ctStatus')?.addEventListener('change', (e) => { state.filter = e.target.value || 'all'; state.page = 1; reload() })
  // 搜索（单据号/主体名模糊）：cmx-filter-search 回车/按钮触发，reset 清空。
  // 页面局部更新（不整页重绘），输入框文字/焦点/表格滚动天然保留。
  const fb = root.querySelector('#ctFilter')
  if (fb) {
    fb.addEventListener('cmx-filter-search', (e) => {
      state.keyword = ((e.detail || {}).text || '').trim(); state.page = 1; reload()
    })
    fb.addEventListener('cmx-filter-reset', () => {
      state.keyword = ''; state.filter = 'all'; state.page = 1
      const st = root.querySelector('#ctStatus'); if (st) st.value = 'all'
      reload()
    })
  }
  root.querySelector('#ctReload')?.addEventListener('click', async () => { await load(); applyData() })
  // 分页（cmx-pager 独立模式）
  const pager = root.querySelector('#ctPager')
  if (pager) {
    pager.addEventListener('page-change', (e) => {
      const d = e.detail || {}
      if (d.pageSize && d.pageSize !== state.pageSize) { state.pageSize = d.pageSize; state.page = 1 }
      else state.page = d.page || 1
      reload()
    })
  }
  buildListGrid()
  applyData(true)
}
let currentHost = null
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
      const host = ctx && ctx.host; currentHost = host
      const props = (ctx && ctx.props) || {}
      // DAM 优先从 workspace.context 读（框架 openNode 时注入），fallback props
      const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
      const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
      state.dbId = props.dbId || props.db_id || ''
      state.docType = props.docType || props.doc_type || ''
      state.title = props.title || '单据列表'
      state.domain = get('domain') || props.domain || ''
      state.application = get('application') || props.application || ''
      try { await load() } catch (e) { console.error('[cr-todo] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
