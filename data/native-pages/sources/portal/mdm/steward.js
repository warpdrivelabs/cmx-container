/**
 * MDM 数据管家工作台（native-page · 企业级重设计）。
 *
 * 布局：页头 → 四区统计卡（待审/已合并/已驳回/已还原，点击切换）→ 队列面板（表格+评审/驳回）
 * → 评审弹层（红线 diff：字段 | master | victim | 逐字段裁决 master/victim/手填）。
 * 提示统一 cmxInfo/cmxError/cmxConfirm。
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

const ZONES = [
  { code: 'pending', name: '待审', tone: 'warning' },
  { code: 'reviewed', name: '已合并', tone: 'success' },
  { code: 'rejected', name: '已驳回', tone: 'danger' },
  { code: 'unmerged', name: '已还原', tone: 'neutral' },
]
const DIFF_FIELDS = ['name', 'tax_no', 'credit_code', 'short_name', 'phone']
const state = { dbId: '', zone: 'pending', groups: [], detail: null, rulings: {} }

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:14px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .zone-bar { display:flex; gap:8px; margin-bottom:14px; background:var(--sapList_Background);
    border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:8px; }
  .zone-tab { flex:1; display:flex; flex-direction:column; align-items:center; gap:2px; padding:8px 12px;
    border-radius:6px; cursor:pointer; border:1px solid transparent; }
  .zone-tab .z-name { font-size:13px; color:var(--sapTextColor); }
  .zone-tab .z-count { font-size:16px; font-weight:600; color:var(--sapContent_LabelColor); }
  .zone-tab:hover { background:var(--sapList_Hover_Background); }
  .zone-tab.active { border-color:var(--neo-cyan,#00b4d8); background:color-mix(in srgb, var(--neo-cyan,#00b4d8) 12%, transparent); }
  .zone-tab.active .z-name, .zone-tab.active .z-count { color:var(--neo-cyan,#00b4d8); }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:10px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:10px 12px; border-bottom:1px solid var(--sapList_BorderColor); vertical-align:middle; }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  tr.diffrow td { background:color-mix(in srgb, var(--sapCriticalColor,#e76500) 10%, transparent); }
  cmx-panel { display:block; }
  .mask { position:fixed; inset:0; background:rgba(0,0,0,.45); display:flex; align-items:center; justify-content:center; z-index:999; }
  .dlg { width:860px; max-height:84vh; overflow:auto; border-radius:10px; padding:20px;
    background:var(--sapList_Background); color:var(--sapTextColor); border:1px solid var(--sapList_BorderColor); }
  .dlg h3 { margin:0 0 14px; font-size:16px; color:var(--sapTitleColor); }
  .rule select, .rule input { padding:5px 8px; font-size:12px; border-radius:4px;
    border:1px solid var(--sapField_BorderColor); background:var(--sapField_Background); color:var(--sapField_TextColor); }
  .dlg-foot { margin-top:16px; display:flex; justify-content:flex-end; gap:8px; }
  `
}

async function loadGroups() {
  const d = (await apiGet(`/api/mdm/merge-requests?dictCode=supplier&pageSize=200`, state.dbId)) || {}
  state.groups = d.list || []
}
function zoneCount(code) { return state.groups.filter((g) => g.status === code).length }
function filteredGroups() { return state.groups.filter((g) => g.status === state.zone) }
async function loadDetail(mergeId) {
  state.detail = await apiGet(`/api/mdm/merge-requests/detail?mergeId=${mergeId}`, state.dbId)
  state.rulings = {}
}

function zoneBarHtml() {
  return `<div class="zone-bar">${ZONES.map((z) => `
    <div class="zone-tab ${state.zone === z.code ? 'active' : ''}" data-z="${z.code}">
      <span class="z-name">${z.name}</span><span class="z-count">${zoneCount(z.code)}</span>
    </div>`).join('')}</div>`
}

function queueHtml() {
  const list = filteredGroups()
  const rows = list.length ? list.map((g) => `<tr>
    <td class="muted">${g.id}</td><td class="muted">${g.master_id ?? ''}</td><td>${g.score ?? ''}</td>
    <td><cmx-status-tag tone="${(ZONES.find((z) => z.code === g.status) || {}).tone || 'neutral'}" variant="subtle" size="sm">${g.status}</cmx-status-tag></td>
    <td>${g.status === 'pending' ? `<ui5-button design="Emphasized" icon="inspect" data-review="${g.id}">评审</ui5-button><ui5-button design="Transparent" icon="decline" data-rej="${g.id}">驳回</ui5-button>` : ''}</td></tr>`).join('') : null
  return `<cmx-panel title="评审队列 · ${state.zone}" icon="list">
    ${rows
      ? `<table class="tbl"><thead><tr><th>group</th><th>master</th><th>score</th><th>status</th><th>操作</th></tr></thead><tbody>${rows}</tbody></table>`
      : `<cmx-empty-state icon="list" title="该区暂无合并请求"></cmx-empty-state>`}
  </cmx-panel>`
}

function diffHtml() {
  const m = state.detail.master || {}; const v = (state.detail.victims || [])[0] || {}
  const rows = DIFF_FIELDS.map((f) => {
    const mv = m[f] ?? ''; const vv = v[f] ?? ''
    const differ = String(mv) !== String(vv)
    const r = state.rulings[f] || { pick: 'master', text: '' }
    return `<tr class="${differ ? 'diffrow' : ''}"><td>${f}</td><td>${mv}</td><td>${vv}</td>
      <td class="rule"><select data-f="${f}" data-k="pick">
        <option value="master" ${r.pick === 'master' ? 'selected' : ''}>master</option>
        <option value="victim" ${r.pick === 'victim' ? 'selected' : ''}>victim</option>
        <option value="custom" ${r.pick === 'custom' ? 'selected' : ''}>手填</option>
      </select> <input data-f="${f}" data-k="text" placeholder="手填值" value="${r.text || ''}" style="${r.pick === 'custom' ? '' : 'display:none'}"></td></tr>`
  }).join('')
  return `<h3>红线 diff · group=${state.detail.group.id}</h3>
    <table class="tbl"><thead><tr><th>字段</th><th>master(${m.id || ''})</th><th>victim(${v.id || ''})</th><th>裁决</th></tr></thead><tbody>${rows}</tbody></table>
    <div class="dlg-foot">
      <ui5-button design="Transparent" id="stBack">返回</ui5-button>
      <ui5-button design="Negative" icon="decline" id="stReject">驳回</ui5-button>
      <ui5-button design="Emphasized" icon="combine" id="stMerge">按裁决合并</ui5-button>
    </div>`
}

// 弹层挂 document.body（无 transform 祖先，fixed 铺满视口，无左侧分界线）并自带内联样式
let dlgEl = null
function dlgCss() {
  return `
  .mdm-mask { position:fixed; inset:0; background:rgba(0,0,0,.45); display:flex; align-items:center; justify-content:center; z-index:999; }
  .mdm-dlg { width:860px; max-height:84vh; overflow:auto; border-radius:10px; padding:20px;
    background:var(--sapList_Background,#1a2332); color:var(--sapTextColor,#eef); border:1px solid var(--sapList_BorderColor,#334); }
  .mdm-dlg h3 { margin:0 0 14px; font-size:16px; color:var(--sapTitleColor,#fff); }
  .mdm-dlg .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .mdm-dlg .tbl th { text-align:left; padding:8px 10px; font-size:12px; color:var(--sapContent_LabelColor,#9ab); border-bottom:1px solid var(--sapList_BorderColor,#334); }
  .mdm-dlg .tbl td { padding:8px 10px; border-bottom:1px solid var(--sapList_BorderColor,#334); }
  .mdm-dlg tr.diffrow td { background:color-mix(in srgb, var(--sapCriticalColor,#e76500) 12%, transparent); }
  .mdm-dlg .rule select, .mdm-dlg .rule input { padding:5px 8px; font-size:12px; border-radius:4px;
    border:1px solid var(--sapField_BorderColor,#456); background:var(--sapField_Background,#223); color:var(--sapField_TextColor,#eef); }
  .mdm-dlg .dlg-foot { margin-top:16px; display:flex; justify-content:flex-end; gap:8px; }
  `
}
function openDiff() {
  closeDiff()
  const mask = document.createElement('div'); mask.className = 'mdm-mask'
  mask.innerHTML = `<style>${dlgCss()}</style><div class="mdm-dlg">${diffHtml()}</div>`
  dlgEl = mask
  mask.addEventListener('click', (e) => { if (e.target === mask) closeDiff() })
  bindDiff(mask)
  document.body.appendChild(mask)
}
function closeDiff() { if (dlgEl) { dlgEl.remove(); dlgEl = null } }
function bindDiff(scope) {
  scope.querySelectorAll('[data-f]').forEach((el) => el.addEventListener('change', () => {
    const f = el.dataset.f; const k = el.dataset.k
    state.rulings[f] = state.rulings[f] || { pick: 'master', text: '' }
    state.rulings[f][k] = el.value
    if (k === 'pick') {
      const inp = scope.querySelector(`input[data-f="${f}"][data-k="text"]`)
      if (inp) inp.style.display = el.value === 'custom' ? '' : 'none'
    }
  }))
  scope.querySelector('#stMerge')?.addEventListener('click', async () => { try { await doMerge() } catch (e) { cmx().cmxError?.(`合并失败：${e.message}`) } })
  scope.querySelector('#stReject')?.addEventListener('click', () => doReject(state.detail.group.id))
  scope.querySelector('#stBack')?.addEventListener('click', () => { closeDiff(); state.detail = null })
}

function viewHtml() {
  return `<div class="pg"><div class="pg-head"><div class="pg-title">数据管家工作台</div>
    <div class="pg-sub">匹配评审 · 字段级存活裁决 · 合并/驳回/还原</div></div>
    ${zoneBarHtml()}${queueHtml()}</div>`
}

function collectRulings() {
  const survivorship = {}; const overrides = {}
  for (const f of DIFF_FIELDS) {
    const r = state.rulings[f]; if (!r) continue
    if (r.pick === 'master') survivorship[f] = 'master'
    else if (r.pick === 'victim') overrides[f] = ((state.detail.victims || [])[0] || {})[f] ?? null
    else overrides[f] = r.text
  }
  return { survivorship, overrides }
}

async function doMerge() {
  const M = cmx(); const g = state.detail.group
  const { survivorship, overrides } = collectRulings()
  const masterId = g.master_id
  const victimIds = (g.member_ids || []).filter((id) => id !== masterId)
  await apiPost('/api/mdm/merge-requests', { dictCode: 'supplier', masterId, victimIds, mergeId: g.id, survivorship, overrides }, state.dbId)
  M.cmxInfo?.('合并成功'); closeDiff(); state.detail = null; await loadGroups(); refresh()
}
async function doReject(id) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({ title: '驳回', message: `驳回 group=${id}？`, danger: true })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests/reject', { mergeId: Number(id), reason: '管家驳回' }, state.dbId)
  M.cmxInfo?.('已驳回'); closeDiff(); state.detail = null; await loadGroups(); refresh()
}

function bind(root) {
  root.querySelectorAll('.zone-tab').forEach((k) => k.addEventListener('click', () => {
    state.zone = k.dataset.z; state.detail = null; refresh()
  }))
  root.querySelectorAll('[data-review]').forEach((b) => b.addEventListener('click', async () => { await loadDetail(b.dataset.review); openDiff() }))
  root.querySelectorAll('[data-rej]').forEach((b) => b.addEventListener('click', () => doReject(b.dataset.rej)))
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
      try { await loadGroups() } catch (e) { console.error('[steward] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
