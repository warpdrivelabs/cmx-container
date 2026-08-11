/**
 * MDM 变更申请详情页（native-page · 并列标签页）。
 * 由待办台 openNode 打开，经 host.workspace.context 读 { crId }，拉取 /api/mdm/change-requests/detail。
 * 预留「关联流程」区，后续展示该申请的审批/激活流程。
 */
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
const STATUS_META = {
  draft: '草稿', approving: '审批中', approved: '已通过', activated: '已激活', rejected: '已驳回', aborted: '已作废',
}
const state = { dbId: '', crId: null, detail: null }
function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .card { background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; margin-bottom:12px; }
  .card-title { font-size:14px; font-weight:600; color:var(--sapTitleColor); margin-bottom:8px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:8px 10px; font-size:12px; color:var(--sapContent_LabelColor); border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:8px 10px; border-bottom:1px solid var(--sapList_BorderColor); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-desc-list { display:block; }
  `
}
function viewHtml() {
  const d = state.detail || {}; const h = d.head || {}; const lines = d.lines || []
  const kv = (l, v) => `<cmx-desc-item label="${l}">${v ?? '—'}</cmx-desc-item>`
  const lineRows = lines.map((l, i) => {
    const p = (l.line_payload && typeof l.line_payload === 'object') ? l.line_payload : {}
    return `<tr><td>${i + 1}</td><td>${l.line_type || ''}</td><td>${l.line_action || ''}</td><td>${p.account_no || ''}</td><td>${p.bank_name || ''}</td></tr>`
  }).join('') || '<tr><td colspan="5" class="muted">无明细行</td></tr>'
  return `<div class="pg">
    <div class="card"><div class="card-title">CR-${h.id ?? state.crId ?? ''} 基本信息</div>
      <cmx-desc-list columns="3" border>
        ${kv('单据号', h.doc_no)}${kv('状态', STATUS_META[h.doc_status] || h.doc_status)}
        ${kv('单据类型', h.doc_type)}${kv('变更类型', h.cr_type)}
        ${kv('目标字典', h.target_dict_code)}${kv('目标记录ID', h.target_record_id)}
        ${kv('供应商名称', h.subject_name)}${kv('税号', (h.payload || {}).tax_no)}${kv('信用代码', (h.payload || {}).credit_code)}
      </cmx-desc-list></div>
    <div class="card"><div class="card-title">明细行</div>
      <table class="tbl"><thead><tr><th>#</th><th>类型</th><th>操作</th><th>账号</th><th>开户行</th></tr></thead><tbody>${lineRows}</tbody></table></div>
    <div class="card"><div class="card-title">关联流程</div>
      <cmx-empty-state icon="process" title="暂无流程" description="审批记录后续从流程引擎接入"></cmx-empty-state></div></div>`
}
let currentHost = null
export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      state.dbId = (ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || ''
      try { state.crId = host?.workspace?.context?.get?.('crId') || null } catch { state.crId = null }
      if (state.crId) {
        try { state.detail = await apiGet(`/api/mdm/change-requests/detail?crId=${state.crId}`, state.dbId) }
        catch (e) { console.error('[cr-detail] load fail', e) }
      }
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
