/**
 * MDM 供应商新增/变更表单页（native-page · 并列标签页）。
 * 由列表页 openNode 打开，经 host.workspace.context 读 { mode:'create'|'update', supplier }。
 * 技术字段（doc_type/cr_type/target_dict/target_record_id/field_deltas）系统自动填充。
 * 银行账户用 cmx-revo-grid 表格组件（整页内渲染，时序稳定）。
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

const BIZ_FIELDS = ['name', 'tax_no', 'credit_code', 'short_name']
const state = { dbId: '', mode: 'create', supplier: null, bankLines: [] }
let rootEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)
const val = (id) => { const el = q(id); return el ? (el.value || '').trim() : '' }

function styleCss() {
  return `
  .pg { height:100%; display:flex; flex-direction:column; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .card { display:flex; flex-direction:column; flex:1; min-height:0;
    background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; }
  .card-title { font-size:15px; font-weight:600; color:var(--sapTitleColor); margin-bottom:10px; }
  .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:14px 18px; padding:4px 2px 12px; }
  .f-item { display:flex; flex-direction:column; gap:6px; min-width:0; }
  .f-item > label { font-size:12px; color:var(--sapContent_LabelColor); }
  .req::after { content:' *'; color:var(--sapNegativeColor,#e90b0b); }
  .bank-fill { flex:1; min-height:180px; overflow:auto; border:1px solid var(--sapList_BorderColor); border-radius:6px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { position:sticky; top:0; text-align:left; padding:8px 10px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); background:var(--sapList_Background); }
  .tbl td { padding:8px 10px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl input { width:100%; box-sizing:border-box; padding:6px 8px; font-size:13px; border-radius:4px;
    border:1px solid var(--sapField_BorderColor); background:var(--sapField_Background); color:var(--sapField_TextColor); }
  .foot { display:flex; justify-content:flex-end; gap:8px; margin-top:12px; }
  cmx-toolbar { display:block; }
  `
}

function viewHtml() {
  const o = state.supplier || {}
  const isEdit = state.mode === 'update'
  return `<div class="pg"><div class="card">
    <div class="card-title">${isEdit ? '变更供应商' : '新增供应商'}</div>
    <div class="form-grid">
      <div class="f-item"><label><span class="req">供应商名称</span></label><ui5-input id="fName" value="${o.name || ''}"></ui5-input></div>
      <div class="f-item"><label>税号</label><ui5-input id="fTaxNo" value="${o.tax_no || ''}"></ui5-input></div>
      <div class="f-item"><label>统一社会信用代码</label><ui5-input id="fCreditCode" value="${o.credit_code || ''}"></ui5-input></div>
      <div class="f-item"><label>简称</label><ui5-input id="fShortName" value="${o.short_name || ''}"></ui5-input></div>
    </div>
    <div class="card-title" style="font-size:13px">银行账户</div>
    <cmx-toolbar><ui5-button design="Default" icon="add" id="fAddRow">增行</ui5-button><ui5-button design="Transparent" icon="delete" id="fDelRow">删选中</ui5-button></cmx-toolbar>
    <div class="bank-fill" id="fGrid"></div>
    <div class="foot">
      <ui5-button design="Default" icon="save" id="fSave">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit">保存并提交</ui5-button>
    </div></div></div>`
}

// 银行行用组件库 cmx-revo-grid（可编辑）。增行用 CmxDataSet.addRow（触发 _refreshSource）+
// refreshLayout 双保险，保证新行即时可见；容器 .bank-fill 有最低高度。
let bankGrid = null
let lineSeq = 0
const newLine = () => { lineSeq += 1; return { id: `nl_${Date.now()}_${lineSeq}`, account_no: '', bank_name: '' } }
function bindBankGrid() {
  const C = cmx(); const wrap = q('fGrid'); if (!wrap) return
  wrap.innerHTML = ''
  const grid = document.createElement('cmx-revo-grid')
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'bankLines' })
    cm.setMembers([
      new C.CmxColumn({ name: 'account_no', caption: '银行账号', width: '260px', edit: { mode: 'cmx-text-input' } }),
      new C.CmxColumn({ name: 'bank_name', caption: '开户行', width: '260px', edit: { mode: 'cmx-text-input' } }),
    ])
    grid.setColumnModel(cm)
  }
  grid.setOptions?.({ editable: true, fillHeight: true, showRowIndex: true, selectionMode: 'multi' })
  if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows([newLine()]); grid.setDataSet(ds) }
  else grid.setDataSet?.([newLine()])
  wrap.appendChild(grid); bankGrid = grid
  queueMicrotask(() => grid.refreshLayout?.())
}
function collectLines() {
  const ds = bankGrid?.getDataSet?.()
  const rows = ds ? (ds.toPlainRows ? ds.toPlainRows() : (ds.getRows ? ds.getRows() : [])) : []
  return rows.filter((r) => (r.account_no || r.bank_name))
    .map((r) => ({ line_type: 'bank_account', line_action: 'insert', line_payload: { account_no: r.account_no || '', bank_name: r.bank_name || '' } }))
}

function buildHead() {
  const name = val('fName'); const tax = val('fTaxNo'); const cc = val('fCreditCode'); const sn = val('fShortName')
  if (state.mode === 'update') {
    const o = state.supplier || {}
    const deltas = {}
    const cur = { name, tax_no: tax, credit_code: cc, short_name: sn }
    for (const f of BIZ_FIELDS) if ((cur[f] || '') !== (o[f] || '')) deltas[f] = { old: o[f] ?? '', new: cur[f] ?? '' }
    return { doc_type: 'mdm_supplier_change', cr_type: 'update', target_dict_code: 'supplier',
      target_record_id: Number(o.id), name, tax_no: tax, credit_code: cc, short_name: sn, field_deltas: deltas }
  }
  return { doc_type: 'mdm_supplier_apply', cr_type: 'create', target_dict_code: 'supplier', name, tax_no: tax, credit_code: cc, short_name: sn }
}
async function doSave(submit) {
  const M = cmx()
  if (!val('fName')) { M.cmxWarn?.('供应商名称不能为空'); return }
  try {
    const d = await apiPost('/api/mdm/change-requests/create', { head: buildHead(), lines: collectLines() }, state.dbId)
    if (submit) await apiPost('/api/mdm/change-requests/submit', { crId: d.crId }, state.dbId)
    M.cmxInfo?.(submit ? `CR-${d.crId} 已提交审批` : `已创建变更申请 CR-${d.crId}（草稿）`)
  } catch (e) { M.cmxError?.(`保存失败：${e.message}`) }
}

function bind(root) {
  rootEl = root
  bindBankGrid()
  root.querySelector('#fAddRow')?.addEventListener('click', () => {
    const C = cmx()
    const ds = bankGrid?.getDataSet?.()
    if (ds?.addRow) ds.addRow(newLine()); else bankGrid?.addRow?.(newLine())
    queueMicrotask(() => bankGrid?.refreshLayout?.())
  })
  root.querySelector('#fDelRow')?.addEventListener('click', () => {
    const ids = bankGrid?.getSelectedIds?.(); if (ids?.length) { bankGrid.removeRows(ids); queueMicrotask(() => bankGrid?.refreshLayout?.()) }
  })
  root.querySelector('#fSave')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit')?.addEventListener('click', () => doSave(true))
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
      const ctxGet = (k) => { try { return host?.workspace?.context?.get?.(k) } catch { return undefined } }
      state.mode = ctxGet('mode') || 'create'
      state.supplier = ctxGet('supplier') || null
      state.bankLines = [{ account_no: '', bank_name: '' }]
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
