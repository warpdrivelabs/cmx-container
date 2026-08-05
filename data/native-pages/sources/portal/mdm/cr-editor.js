/**
 * MDM 供应商主数据维护台（native-page · 以主数据列表为中心 · 业务友好）。
 *
 * 页面先展示已发布供应商列表；「新增供应商」常可用；选中后「变更选中」可用。
 * 技术字段（doc_type/cr_type/target_dict/target_record_id）系统自动填充，不暴露。
 * 零后端改动：复用 /api/dct/export 与 /api/mdm/change-requests/create|submit。
 *
 * 弹层挂 document.body（无 transform 祖先，fixed 铺满视口，无左侧分界线）并自带内联样式。
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

const BIZ_FIELDS = ['name', 'tax_no', 'credit_code', 'short_name']
const state = { dbId: '', suppliers: [], selected: null, kw: '', mode: null, original: null }

let rootEl = null
let dlgEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)
const dval = (id) => { const el = dlgEl && dlgEl.querySelector('#' + id); return el ? (el.value || '').trim() : '' }

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:14px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .pg-body { display:flex; flex-direction:column; gap:14px; max-width:1200px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:10px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor); border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:10px 12px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl tbody tr { cursor:pointer; }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .tbl tbody tr.sel td { background:color-mix(in srgb, var(--neo-cyan,#00b4d8) 16%, transparent); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-panel, cmx-toolbar, cmx-filter-bar { display:block; }
  `
}
function dlgCss() {
  return `
  .mdm-mask { position:fixed; inset:0; background:rgba(0,0,0,.45); display:flex; align-items:center; justify-content:center; z-index:999; }
  .mdm-dlg { width:760px; max-height:84vh; overflow:auto; border-radius:10px; padding:20px;
    background:var(--sapList_Background,#1a2332); color:var(--sapTextColor,#eef); border:1px solid var(--sapList_BorderColor,#334); }
  .mdm-dlg h3 { margin:0 0 14px; font-size:16px; color:var(--sapTitleColor,#fff); }
  .mdm-dlg .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:14px 18px; padding:6px 2px; }
  .mdm-dlg .f-item { display:flex; flex-direction:column; gap:6px; min-width:0; }
  .mdm-dlg .f-item > label { font-size:12px; color:var(--sapContent_LabelColor,#9ab); }
  .mdm-dlg .req::after { content:' *'; color:var(--sapNegativeColor,#e90b0b); }
  .mdm-dlg .sec { margin:14px 0 6px; font-size:13px; font-weight:600; }
  .mdm-dlg .mdm-bank-in { width:100%; box-sizing:border-box; padding:6px 8px; font-size:13px; border-radius:4px;
    border:1px solid var(--sapField_BorderColor,#456); background:var(--sapField_Background,#223); color:var(--sapField_TextColor,#eef); }
  .mdm-dlg .dlg-foot { margin-top:16px; display:flex; justify-content:flex-end; gap:8px; }
  `
}

async function loadSuppliers() {
  const h = { Accept: 'application/json' }; if (state.dbId) h.db_id = state.dbId
  const res = await fetch('/api/dct/export?domain=basic&application=dataplatform&module=mdm&dict=supplier', { headers: h, credentials: 'same-origin' })
  const text = await res.text()
  state.suppliers = text.split('\n').filter((l) => l.trim()).map((l) => { try { return JSON.parse(l) } catch { return null } }).filter(Boolean)
}
function filteredSuppliers() {
  const kw = state.kw.trim().toLowerCase()
  if (!kw) return state.suppliers
  return state.suppliers.filter((s) => [s.name, s.code, s.credit_code, s.tax_no].some((v) => String(v || '').toLowerCase().includes(kw)))
}

function headHtml() {
  return `<div class="pg-head"><div class="pg-title">供应商主数据</div>
    <div class="pg-sub">浏览已发布供应商；新增或选中后发起变更申请，审批通过自动激活</div></div>`
}
function toolbarHtml() {
  const dis = state.selected ? '' : 'disabled'
  return `<cmx-toolbar divider>
    <ui5-button design="Emphasized" icon="add" id="ceAdd">新增供应商</ui5-button>
    <ui5-button design="Default" icon="edit" id="ceEdit" ${dis}>变更选中</ui5-button>
    <ui5-button design="Transparent" icon="refresh" slot="actions" id="ceReload">刷新</ui5-button>
  </cmx-toolbar>`
}
function listHtml() {
  const rows = filteredSuppliers()
  if (!rows.length) return `<cmx-empty-state icon="company" title="暂无供应商" description="点击「新增供应商」创建第一条主数据"></cmx-empty-state>`
  const trs = rows.map((s) => `<tr data-id="${s.id}" class="${String(state.selected) === String(s.id) ? 'sel' : ''}">
    <td class="muted">${s.code || ''}</td><td>${s.name || ''}</td><td>${s.tax_no || ''}</td><td>${s.credit_code || ''}</td><td>${s.short_name || ''}</td><td class="muted">v${s.published_version ?? 1}</td></tr>`).join('')
  return `<table class="tbl"><thead><tr><th>编码</th><th>名称</th><th>税号</th><th>信用代码</th><th>简称</th><th>版本</th></tr></thead><tbody>${trs}</tbody></table>`
}
function viewHtml() {
  return `<div class="pg">${headHtml()}<div class="pg-body">
    <cmx-panel title="供应商列表" icon="company">${toolbarHtml()}
      <cmx-filter-bar id="ceFilter" search-placeholder="名称/编码/信用代码"></cmx-filter-bar>
      ${listHtml()}
    </cmx-panel></div></div>`
}

// ── 表单弹层（挂 body，自带样式）─────────────────────────────────────────────
// 银行行用普通可编辑表格（revo-grid 在 body 弹层内初始化时序不稳定，普通表格可靠增删/可见）
function bankHtml() {
  const rows = (state.bankLines || []).map((l, i) => `<tr data-i="${i}">
    <td><input class="mdm-bank-in" data-i="${i}" data-k="account_no" placeholder="银行账号" value="${l.account_no || ''}"></td>
    <td><input class="mdm-bank-in" data-i="${i}" data-k="bank_name" placeholder="开户行" value="${l.bank_name || ''}"></td>
    <td style="width:70px"><ui5-button design="Transparent" icon="delete" data-del="${i}">删除</ui5-button></td></tr>`).join('')
  return `<table class="tbl"><thead><tr><th>银行账号</th><th>开户行</th><th></th></tr></thead><tbody>${rows || '<tr><td colspan="3" class="muted">暂无银行账户，点击「增行」添加</td></tr>'}</tbody></table>`
}
function renderBank() {
  const wrap = dlgEl && dlgEl.querySelector('#fBank'); if (!wrap) return
  wrap.innerHTML = bankHtml()
  wrap.querySelectorAll('.mdm-bank-in').forEach((inp) => inp.addEventListener('input', () => {
    const i = +inp.dataset.i; const k = inp.dataset.k
    state.bankLines[i] = state.bankLines[i] || {}
    state.bankLines[i][k] = inp.value
  }))
  wrap.querySelectorAll('[data-del]').forEach((b) => b.addEventListener('click', () => {
    state.bankLines.splice(+b.dataset.del, 1); renderBank()
  }))
}
function collectLines() {
  return (state.bankLines || []).filter((r) => (r.account_no || r.bank_name))
    .map((r) => ({ line_type: 'bank_account', line_action: 'insert', line_payload: { account_no: r.account_no || '', bank_name: r.bank_name || '' } }))
}

function openForm(mode) {
  state.mode = mode
  state.original = mode === 'update' ? (state.suppliers.find((s) => String(s.id) === String(state.selected)) || {}) : {}
  state.bankLines = [{ account_no: '', bank_name: '' }]
  const o = state.original
  const isEdit = mode === 'update'
  const mask = document.createElement('div'); mask.className = 'mdm-mask'
  mask.innerHTML = `<style>${dlgCss()}</style><div class="mdm-dlg"><h3>${isEdit ? '变更供应商' : '新增供应商'}</h3>
    <div class="form-grid">
      <div class="f-item"><label><span class="req">供应商名称</span></label><ui5-input id="fName" value="${o.name || ''}"></ui5-input></div>
      <div class="f-item"><label>税号</label><ui5-input id="fTaxNo" value="${o.tax_no || ''}"></ui5-input></div>
      <div class="f-item"><label>统一社会信用代码</label><ui5-input id="fCreditCode" value="${o.credit_code || ''}"></ui5-input></div>
      <div class="f-item"><label>简称</label><ui5-input id="fShortName" value="${o.short_name || ''}"></ui5-input></div>
    </div>
    <div class="sec">银行账户</div>
    <cmx-toolbar><ui5-button design="Default" icon="add" id="fAddRow">增行</ui5-button></cmx-toolbar>
    <div id="fBank"></div>
    <div class="dlg-foot">
      <ui5-button design="Transparent" id="fCancel">取消</ui5-button>
      <ui5-button design="Default" icon="save" id="fSave">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit">保存并提交</ui5-button>
    </div></div>`
  dlgEl = mask
  mask.addEventListener('click', (e) => { if (e.target === mask) closeForm() })
  mask.querySelector('#fCancel').addEventListener('click', closeForm)
  mask.querySelector('#fAddRow').addEventListener('click', () => { state.bankLines.push({ account_no: '', bank_name: '' }); renderBank() })
  mask.querySelector('#fSave').addEventListener('click', () => doSave(false))
  mask.querySelector('#fSubmit').addEventListener('click', () => doSave(true))
  document.body.appendChild(mask)
  renderBank()
}
function closeForm() { if (dlgEl) { dlgEl.remove(); dlgEl = null } state.mode = null }

function buildHead() {
  const name = dval('fName'); const tax = dval('fTaxNo'); const cc = dval('fCreditCode'); const sn = dval('fShortName')
  if (state.mode === 'update') {
    const o = state.original || {}
    const deltas = {}
    const cur = { name, tax_no: tax, credit_code: cc, short_name: sn }
    for (const f of BIZ_FIELDS) if ((cur[f] || '') !== (o[f] || '')) deltas[f] = { old: o[f] ?? '', new: cur[f] ?? '' }
    return { doc_type: 'mdm_supplier_change', cr_type: 'update', target_dict_code: 'supplier',
      target_record_id: Number(state.selected), name, tax_no: tax, credit_code: cc, short_name: sn, field_deltas: deltas }
  }
  return { doc_type: 'mdm_supplier_apply', cr_type: 'create', target_dict_code: 'supplier',
    name, tax_no: tax, credit_code: cc, short_name: sn }
}
async function doSave(submit) {
  const M = cmx()
  if (!dval('fName')) { M.cmxWarn?.('供应商名称不能为空'); return }
  try {
    const head = buildHead()
    const d = await apiPost('/api/mdm/change-requests/create', { head, lines: collectLines() }, state.dbId)
    if (submit) await apiPost('/api/mdm/change-requests/submit', { crId: d.crId }, state.dbId)
    M.cmxInfo?.(submit ? `CR-${d.crId} 已提交审批` : `已创建变更申请 CR-${d.crId}（草稿），请到待办台提交/审批`)
    closeForm(); state.selected = null
    await loadSuppliers(); refresh()
  } catch (e) { M.cmxError?.(`保存失败：${e.message}`) }
}

function bind(root) {
  rootEl = root
  root.querySelector('#ceAdd')?.addEventListener('click', () => openForm('create'))
  root.querySelector('#ceEdit')?.addEventListener('click', () => { if (state.selected) openForm('update') })
  root.querySelector('#ceReload')?.addEventListener('click', async () => { await loadSuppliers(); refresh() })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-search', (e) => { state.kw = e.detail?.text || ''; refresh() })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-reset', () => { state.kw = ''; refresh() })
  root.querySelectorAll('.tbl tbody tr[data-id]').forEach((tr) => tr.addEventListener('click', () => {
    state.selected = state.selected === tr.dataset.id ? null : tr.dataset.id; refresh()
  }))
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
      state.dbId = (ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || ''
      try { await loadSuppliers() } catch (e) { console.error('[cr-editor] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
