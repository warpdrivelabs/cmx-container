/**
 * MDM 供应商详情页（native-page · 并列标签页）。
 * 由列表页 openNode 打开，经 host.workspace.context 读 { supplier }。
 * 预留「关联流程」区，后续展示该供应商的变更/审批流程。
 */
function unwrap(res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) { const e = new Error(body.msg || `业务错误 ${body.code}`); e.body = body; throw e }
    return body.data
  }
  if (!res.ok) { const e = new Error((body && body.error) || `HTTP ${res.status}`); e.status = res.status; throw e }
  return body
}
const state = { dbId: '', supplier: null }
function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .card { background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; margin-bottom:12px; }
  .card-title { font-size:14px; font-weight:600; color:var(--sapTitleColor); margin-bottom:8px; }
  cmx-desc-list { display:block; }
  `
}
function viewHtml() {
  const s = state.supplier || {}
  const kv = (l, v) => `<cmx-desc-item label="${l}">${v ?? '—'}</cmx-desc-item>`
  return `<div class="pg">
    <div class="card"><div class="card-title">供应商·${s.name || ''}</div>
      <cmx-desc-list columns="3" border>
        ${kv('编码', s.code)}${kv('名称', s.name)}${kv('简称', s.short_name)}
        ${kv('税号', s.tax_no)}${kv('信用代码', s.credit_code)}${kv('版本', 'v' + (s.published_version ?? 1))}
      </cmx-desc-list></div>
    <div class="card"><div class="card-title">关联流程（预留）</div>
      <cmx-empty-state icon="process" title="暂无流程" description="后续在此展示该供应商的变更/审批流程"></cmx-empty-state></div></div>`
}
let currentHost = null
export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      state.dbId = (ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || ''
      try { state.supplier = host?.workspace?.context?.get?.('supplier') || null } catch { state.supplier = null }
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
