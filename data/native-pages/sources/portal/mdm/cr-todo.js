/**
 * MDM 单据列表台（native-page · 企业级重设计）。
 *
 * 布局：页头 → KPI 统计卡（草稿/待审批/已驳回/已处理，点击过滤）→ 列表面板
 * （cmx-filter-bar + 企业表格 + cmx-status-tag + 行内 ui5-button 操作）→ 详情弹层（cmx-desc-list）。
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
// 流程实例状态 → 徽标文案（flow-status 返回的 state 字段映射）
const FLOW_STATE_META = {
  ACTIVE: { name: '审批中', tone: 'warning' },
  COMPLETED: { name: '已审结', tone: 'success' },
  TERMINATED: { name: '已终止', tone: 'neutral' },
}
const state = { dbId: '', filter: 'all', list: [], domain: '', application: '', page: 1, pageSize: 20, total: 0, flowMap: {}, counts: { draft: 0, approving: 0, rejected: 0, done: 0 } }

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
  .kpi-row { display:grid; grid-template-columns:repeat(auto-fit,minmax(150px,1fr)); gap:12px; margin-bottom:14px; }
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

function counts() { return state.counts }

// KPI 轻量计数：pageSize=1 只取 total（不拉数据）；已处理=activated+aborted
async function loadCounts() {
  const one = async (docStatus) => {
    const d = (await apiGet(`/api/mdm/change-requests?${new URLSearchParams({ page: 1, pageSize: 1, docStatus })}`, state.dbId)) || {}
    return Number(d.total) || 0
  }
  try {
    const [draft, approving, rejected, activated, aborted] = await Promise.all([
      one('draft'), one('approving'), one('rejected'), one('activated'), one('aborted'),
    ])
    state.counts = { draft, approving, rejected, done: activated + aborted }
  } catch (e) { /* 计数失败不影响列表 */ }
}

function kpiHtml() {
  const c = counts()
  const card = (label, value, tone, key, clickable = true) =>
    `<cmx-kpi-card variant="card" label="${label}" value="${value}" tone="${tone}" data-k="${key}" ${clickable ? 'clickable' : ''}></cmx-kpi-card>`
  return `<div class="kpi-row">${card('草稿', c.draft, 'neutral', 'draft')}${card('审批中(流程)', c.approving, 'warning', 'approving')}${card('已驳回', c.rejected, 'danger', 'rejected')}${card('已处理', c.done, 'success', 'done', false)}</div>`
}

// 列表已由服务端按 docStatus 过滤，前端直接展示当前页
function filtered() { return state.list }

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
    <div class="pg-head"><div class="pg-title">单据列表</div>
      <div class="pg-sub">提交 / 撤回 / 驳回重提 / 作废；审批在流程待办中心办理（mdm_approver），通过自动激活落字典
        <ui5-button design="Transparent" icon="activity-items" id="ctOpenFlowTodo" style="margin-left:8px">打开流程待办</ui5-button></div></div>
    ${kpiHtml()}
    <div class="list-card">
      <div class="card-title">申请列表（共 ${state.total} 条）</div>
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
  const is = (s) => (m) => m.doc_status === s
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'crList' })
    cm.setMembers([
      new C.CmxColumn({ id: 'id', caption: 'ID', dataType: 'VARCHAR', width: '110px' }),
      new C.CmxColumn({ id: 'doc_no', caption: '单据号', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'remark', caption: '业务事由', dataType: 'VARCHAR', width: '150px' }),
      new C.CmxColumn({ id: 'doc_type', caption: '类型', dataType: 'VARCHAR', width: '120px' }),
      new C.CmxColumn({ id: 'status_name', caption: '状态', dataType: 'VARCHAR', width: '80px' }),
      new C.CmxColumn({ id: 'flow_name', caption: '流程', dataType: 'VARCHAR', width: '90px' }),
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
  const rows = state.list.map((r) => {
    const fs = (state.flowMap || {})[r.id]
    const fm = fs && fs.state ? (FLOW_STATE_META[fs.state] || { name: fs.state, tone: 'neutral' }) : null
    return { ...r, status_name: (STATUS_META[r.doc_status] || {}).name || r.doc_status, flow_name: fm ? fm.name : '' }
  })
  const fill = () => {
    if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows(rows); grid.setDataSet(ds) }
    else grid.setDataSet?.(rows)
    grid.refreshLayout?.()
  }
  requestAnimationFrame(() => requestAnimationFrame(fill))
}

// ── 操作 ─────────────────────────────────────────────────────────────────────
async function doAction(act, id) {
  const crId = Number(id); const M = cmx()
  try {
    if (act === 'submit') {
      const ok = await M.cmxConfirm?.({ title: '提交审批', message: `确认提交 CR-${crId}？提交后进入流程审批（mdm_approver 审批）。`, danger: false })
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
    await load(); refresh()
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
  const d = (await apiGet(`/api/mdm/change-requests?${new URLSearchParams(params)}`, state.dbId)) || {}
  state.list = d.list || []
  state.total = Number(d.total) || 0
  await loadFlowState()
}

// 流程徽标 + 懒同步自愈：approving/activating 单批量查实例状态；服务端顺带核对
// 「实例已终态而 CR 未回写」（webhook 丢失兜底）与「无实例超时」（submit 崩溃残留回退 draft）。
async function loadFlowState() {
  const ids = state.list.filter((r) => r.doc_status === 'approving' || r.doc_status === 'activating').map((r) => r.id)
  if (!ids.length) { state.flowMap = {}; return }
  try {
    const d = (await apiGet(`/api/mdm/change-requests/flow-status?crIds=${ids.join(',')}`, state.dbId)) || {}
    const map = {}
    for (const it of d.items || []) map[it.crId] = it
    state.flowMap = map
    // 懒同步可能已回写状态（activated/rejected/draft）——刷新列表数据以反映最新 doc_status。
    const changed = state.list.some((r) => {
      const st = (map[r.id] || {}).state
      return (r.doc_status === 'approving' || r.doc_status === 'activating')
        && (st === 'COMPLETED' || st === 'TERMINATED')
    })
    if (changed) {
      const params = { page: state.page, pageSize: state.pageSize }
      if (state.filter !== 'all') params.docStatus = state.filter
      const d2 = (await apiGet(`/api/mdm/change-requests?${new URLSearchParams(params)}`, state.dbId)) || {}
      state.list = d2.list || []; state.total = Number(d2.total) || 0
    }
  } catch (e) { state.flowMap = state.flowMap || {}; /* 徽标缺失不阻断列表 */ }
}

function bind(root) {
  rootEl = root
  const reload = async () => { await load(); refresh() }
  // 打开流程待办中心（portal.flow.todo-center 经门户反代取页，审批人在该页办理 MDM 任务）
  root.querySelector('#ctOpenFlowTodo')?.addEventListener('click', () => {
    openTab(currentHost, '流程待办中心', 'portal.flow.todo-center', { domain: state.domain, application: state.application }, { single: true })
  })
  root.querySelectorAll('cmx-kpi-card[clickable]').forEach((k) => k.addEventListener('cmx-kpi-click', () => {
    state.filter = k.dataset.k || 'all'; state.page = 1; reload()
  }))
  root.querySelector('#ctStatus')?.addEventListener('change', (e) => { state.filter = e.target.value || 'all'; state.page = 1; reload() })
  root.querySelector('#ctReload')?.addEventListener('click', async () => { await Promise.all([load(), loadCounts()]); refresh() })
  // 分页（cmx-pager 独立模式）
  const pager = root.querySelector('#ctPager')
  if (pager) {
    pager.total = state.total; pager.page = state.page; pager.pageSize = state.pageSize
    pager.addEventListener('page-change', (e) => {
      const d = e.detail || {}
      if (d.pageSize && d.pageSize !== state.pageSize) { state.pageSize = d.pageSize; state.page = 1 }
      else state.page = d.page || 1
      reload()
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
      state.domain = get('domain') || props.domain || ''
      state.application = get('application') || props.application || ''
      try { await Promise.all([load(), loadCounts()]) } catch (e) { console.error('[cr-todo] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
