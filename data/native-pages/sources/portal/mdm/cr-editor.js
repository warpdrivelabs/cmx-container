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

function unwrap(res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) { const e = new Error(body.msg || `业务错误 ${body.code}`); e.body = body; throw e }
    return body.data
  }
  if (!res.ok) { const e = new Error((body && body.error) || `HTTP ${res.status}`); e.status = res.status; throw e }
  return body
}

const state = { dbId: '', suppliers: [], kw: '' }
let rootEl = null

function styleCss() {
  return `
  .pg { height:100%; display:flex; flex-direction:column; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:10px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .card { display:flex; flex-direction:column; flex:1; min-height:0;
    background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; }
  .card-hd { display:flex; justify-content:space-between; align-items:center; gap:8px; margin-bottom:10px; }
  .card-title { font-size:15px; font-weight:600; color:var(--sapTitleColor); }
  .tbl-wrap { flex:1; min-height:0; overflow:auto; }
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
  const ctxKey = (context && (context.crId || (context.supplier && context.supplier.id))) || ''
  const key = opts.single ? 'single' : (ctxKey || Date.now())
  app.openNode({
    id: `${nativePage}-${key}`, name: nativePage, caption, type: 'workspace-node',
    workspace: { content: { caption, views: [{ type: 'native_pages', native_page: nativePage, view: 'content' }] } },
  }, { initialContext: context })
}

async function loadSuppliers() {
  const h = { Accept: 'application/json' }; if (state.dbId) h.db_id = state.dbId
  const res = await fetch('/api/dct/export?domain=basic&application=dataplatform&module=mdm&dict=supplier', { headers: h, credentials: 'same-origin' })
  const text = await res.text()
  state.suppliers = text.split('\n').filter((l) => l.trim()).map((l) => { try { return JSON.parse(l) } catch { return null } }).filter(Boolean)
}
function filtered() {
  const kw = state.kw.trim().toLowerCase()
  if (!kw) return state.suppliers
  return state.suppliers.filter((s) => [s.name, s.code, s.credit_code, s.tax_no].some((v) => String(v || '').toLowerCase().includes(kw)))
}

function viewHtml() {
  const rows = filtered()
  const body = rows.length ? rows.map((s) => `<tr>
    <td class="muted">${s.code || ''}</td><td>${s.name || ''}</td><td>${s.tax_no || ''}</td><td>${s.credit_code || ''}</td><td>${s.short_name || ''}</td><td class="muted">v${s.published_version ?? 1}</td>
    <td><ui5-button design="Transparent" icon="show" data-view="${s.id}">查看详情</ui5-button><ui5-button design="Transparent" icon="edit" data-edit="${s.id}">变更</ui5-button></td></tr>`).join('')
    : '<tr><td colspan="7" class="muted">暂无供应商，点击「新增供应商」创建</td></tr>'
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">供应商主数据</div>
      <div class="pg-sub">浏览已发布供应商；新增/变更/详情以并列标签页打开</div></div>
    <div class="card">
      <div class="card-hd"><div class="card-title">供应商列表</div>
        <cmx-toolbar><ui5-button design="Emphasized" icon="add" id="ceAdd">新增供应商</ui5-button><ui5-button design="Transparent" icon="refresh" slot="actions" id="ceReload">刷新</ui5-button></cmx-toolbar></div>
      <cmx-filter-bar id="ceFilter" search-placeholder="名称/编码/信用代码"></cmx-filter-bar>
      <div class="tbl-wrap"><table class="tbl"><thead><tr><th>编码</th><th>名称</th><th>税号</th><th>信用代码</th><th>简称</th><th>版本</th><th>操作</th></tr></thead><tbody>${body}</tbody></table></div>
    </div></div>`
}

// 事件委托挂在 document（模块在主 realm 执行；composed 事件必冒泡到 document），
// 在 content() 里立即挂载，不依赖 DOM 挂载时机，规避 shadow 内逐个绑定/whenRendered 时机失效。
// 逐元素绑定（与 activation-mapper 相同的成熟模式）
function bind(root) {
  rootEl = root
  const host = currentHost
  // 新增=单例（只开一个）；详情/变更=按行 id 多开
  root.querySelector('#ceAdd')?.addEventListener('click', () => openTab(host, '新增供应商', 'portal.mdm.cr-form', { mode: 'create' }, { single: true }))
  root.querySelector('#ceReload')?.addEventListener('click', () => { loadSuppliers().then(refresh) })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-search', (e) => { state.kw = e.detail?.text || ''; refresh() })
  root.querySelector('#ceFilter')?.addEventListener('cmx-filter-reset', () => { state.kw = ''; refresh() })
  root.querySelectorAll('[data-view]').forEach((b) => b.addEventListener('click', () => {
    const s = state.suppliers.find((x) => String(x.id) === String(b.dataset.view))
    openTab(host, `供应商·${s?.name || ''}`, 'portal.mdm.supplier-detail', { supplier: s })
  }))
  root.querySelectorAll('[data-edit]').forEach((b) => b.addEventListener('click', () => {
    const s = state.suppliers.find((x) => String(x.id) === String(b.dataset.edit))
    openTab(host, `变更·${s?.name || ''}`, 'portal.mdm.cr-form', { mode: 'update', supplier: s })
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
