/**
 * MDM 供应商字典详情页（native-page）。
 *
 * 由列表页 cr-editor 打开，经 host.workspace.context 读 { supplierId }。
 * 详情页自行调接口加载（不复用列表传过来的对象），保证刷新后数据自洽：
 *   1) 主信息：POST /api/dct/data/search?dict=supplier  + { filters:{id}, pageSize:1 } → rows[0]
 *   2) 银行账户：POST /api/dct/data/search?dict=supplier_bank + { filters:{supplier_id} } → rows[]
 * 银行账户是独立字典（cm_bank_account，非父子分级），字典详情接口不返回它，故单独请求。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 */

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

// 字典坐标四元组（domain/application/module/dbId），来自 ctx.props / workspace.context；module 回退 mdm。
// domain/application 兼容 camelCase（openNode 注入的 domainCode/applicationCode）。
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

const state = {
  coord: null, dbId: '', supplierId: null,
  supplier: null, banks: [],
  loading: true, loadErr: '',
}
let currentHost = null

function coordQs(extra = {}) {
  const c = state.coord || {}
  return new URLSearchParams({
    domain: c.domain || '', application: c.application || '', module: c.module || 'mdm', ...extra,
  }).toString()
}

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:12px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .card { background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px;
    padding:12px 14px; margin-bottom:12px; }
  .card-title { font-size:14px; font-weight:600; color:var(--sapTitleColor); margin-bottom:10px;
    display:flex; align-items:center; gap:6px; }
  .card-title ui5-icon { color:var(--neo-cyan,var(--sapInformativeTextColor,#00b4d8)); font-size:15px; }
  cmx-desc-list { display:block; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:9px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); background:var(--sapList_Background); }
  .tbl td { padding:9px 12px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  .loading { padding:40px; text-align:center; color:var(--sapContent_LabelColor); font-size:13px; }
  .load-err { padding:24px; color:var(--sapNegativeTextColor,#b00); font-size:13px; }
  `
}

function esc(s) { return String(s ?? '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])) }

function viewHtml() {
  if (state.loading) return `<div class="pg"><div class="loading">正在加载供应商详情…</div></div>`
  if (state.loadErr) return `<div class="pg"><div class="load-err">⚠ ${esc(state.loadErr)}</div></div>`
  const s = state.supplier || {}
  const kv = (l, v) => `<cmx-desc-item label="${l}">${v == null || v === '' ? '—' : esc(v)}</cmx-desc-item>`
  const banks = state.banks || []
  const bankRows = banks.length
    ? banks.map((b) => `<tr>
        <td>${esc(b.name)}</td><td>${esc(b.account_no)}</td>
        <td>${esc(b.bank_name)}</td><td class="muted">${esc(b.status)}</td></tr>`).join('')
    : '<tr><td colspan="4" class="muted">暂无银行账户</td></tr>'
  return `<div class="pg">
    <div class="card"><div class="card-title"><ui5-icon name="supplier" mode="Decorative"></ui5-icon>供应商·${esc(s.name || '')}</div>
      <cmx-desc-list columns="3" border>
        ${kv('编码', s.code)}${kv('名称', s.name)}${kv('简称', s.short_name)}
        ${kv('税号', s.tax_no)}${kv('信用代码', s.credit_code)}${kv('版本', s.published_version != null ? 'v' + s.published_version : null)}
      </cmx-desc-list></div>
    <div class="card"><div class="card-title"><ui5-icon name="accounting-document-verification" mode="Decorative"></ui5-icon>银行账户（${banks.length}）</div>
      <table class="tbl"><thead><tr><th>账户名</th><th>账号</th><th>开户行</th><th>状态</th></tr></thead><tbody>${bankRows}</tbody></table></div>
  </div>`
}

// ── 数据加载 ──────────────────────────────────────────────────────────────
async function loadDetail() {
  const id = state.supplierId
  if (id == null || id === '') { state.loadErr = '缺少供应商 ID'; return }
  // 主信息：按 id 取一条（DCT 通用 search + filters，无专用详情 GET 接口）
  const main = (await apiPost(`/api/dct/data/search?${coordQs({ dict: 'supplier' })}`, {
    filters: { id }, pageSize: 1,
  }, state.dbId)) || {}
  state.supplier = (main.rows && main.rows[0]) || null
  if (!state.supplier) { state.loadErr = `供应商 ${id} 不存在`; return }
  // 银行账户：按 supplier_id 过滤（supplier_bank 是独立字典，用 filters.supplier_id，不是 parentId）
  const bank = (await apiPost(`/api/dct/data/search?${coordQs({ dict: 'supplier_bank' })}`, {
    filters: { supplier_id: id }, pageSize: 100,
  }, state.dbId)) || {}
  state.banks = bank.rows || []
}

// ── 渲染编排 ────────────────────────────────────────────────────────────
function refresh() {
  const host = currentHost; if (!host) return
  const root = host.renderRoot || host.shadowRoot; if (!root) return
  root.innerHTML = `<style>${styleCss()}</style>${viewHtml()}`
}

async function init() {
  try {
    await loadDetail()
  } catch (e) {
    state.loadErr = `加载失败：${e.message}`
    console.error('[supplier-detail] load fail', e)
  }
  state.loading = false
  refresh()
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      state.coord = readCoord(ctx)
      state.dbId = state.coord.dbId || ''
      const wctx = host && host.workspace && host.workspace.context
      const get = (k) => { try { return wctx && wctx.get ? wctx.get(k) : undefined } catch { return undefined } }
      const p = (ctx && ctx.props) || {}
      // supplierId 优先；兼容旧调用可能仍传整个 supplier 对象
      const ctxSupplier = get('supplier')
      state.supplierId = get('supplierId') || p.supplierId || (ctxSupplier && ctxSupplier.id) || null
      state.supplier = null; state.banks = []
      state.loading = true; state.loadErr = ''
      // 异步加载，content 立即返回 loading 占位，加载完 refresh
      init()
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
