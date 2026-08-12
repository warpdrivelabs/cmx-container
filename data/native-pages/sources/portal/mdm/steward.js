/**
 * MDM 数据管家工作台（native-page · 双 tab 重设计）。
 *
 * 布局：页头（字典下拉 + tab 切换）→
 *   tab1「查重发现项」：zone-bar（待评审/已合并/已忽略）+ 发现项列表 + 详情弹层（字段对比 + 合并/忽略）
 *   tab2「合并历史」：zone-bar（待审/已合并/已驳回/已还原）+ 队列表格 + 评审弹层（红线 diff + 逐字段裁决）
 *
 * 发现项（md_match_scan）是系统全库扫描出的重复簇，管家评审载体；
 * 合并历史（md_merge_record）是已确认的合并事务记录。两者职责分离。
 * 提示统一 cmxInfo/cmxError/cmxConfirm；禁 alert/confirm。
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

// 合并历史 tab 的 zone（md_merge_record.status）
const ZONES = [
  { code: 'pending', name: '待审', tone: 'warning' },
  { code: 'reviewed', name: '已合并', tone: 'success' },
  { code: 'rejected', name: '已驳回', tone: 'danger' },
  { code: 'unmerged', name: '已还原', tone: 'neutral' },
]
// 发现项 tab 的 zone（md_match_scan.status）
const FINDING_ZONES = [
  { code: 'pending', name: '待评审', tone: 'warning' },
  { code: 'resolved', name: '已合并', tone: 'success' },
  { code: 'ignored', name: '已忽略', tone: 'neutral' },
]
const state = {
  dbId: '',
  dicts: [],            // 有查重规则的字典列表（从 match-configs 动态拉）
  dictConfigMap: {},    // dictCode → match_config（含 survive_fields，供字段对比动态取列）
  dictCode: '',         // 当前选中字典（init 后默认 dicts[0]）
  tab: 'findings',
  // 发现项（md_match_scan）
  findingsZone: 'pending',
  findings: [],
  findingDetail: null,
  // 合并历史（md_merge_record）
  zone: 'pending',
  groups: [],
  detail: null,
  rulings: {},
}

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:14px; display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:12px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .head-tools { display:flex; align-items:center; gap:12px; }
  .dict-sel { padding:5px 8px; font-size:13px; border-radius:4px;
    border:1px solid var(--sapField_BorderColor); background:var(--sapField_Background); color:var(--sapField_TextColor); }
  .tab-bar { display:flex; gap:4px; margin-bottom:14px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tab-btn { padding:8px 16px; font-size:13px; cursor:pointer; border:none; background:transparent;
    color:var(--sapContent_LabelColor); border-bottom:2px solid transparent; }
  .tab-btn.active { color:var(--neo-cyan,#00b4d8); border-bottom-color:var(--neo-cyan,#00b4d8); font-weight:600; }
  .zone-bar { display:flex; gap:8px; margin-bottom:14px; background:var(--sapList_Background);
    border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:8px; }
  .zone-tab { flex:1; display:flex; flex-direction:column; align-items:center; gap:2px; padding:8px 12px;
    border-radius:6px; cursor:pointer; border:1px solid transparent; }
  .zone-tab .z-name { font-size:13px; color:var(--sapTextColor); }
  .zone-tab:hover { background:var(--sapList_Hover_Background); }
  .zone-tab.active { border-color:var(--neo-cyan,#00b4d8); background:color-mix(in srgb, var(--neo-cyan,#00b4d8) 12%, transparent); }
  .zone-tab.active .z-name { color:var(--neo-cyan,#00b4d8); font-weight:600; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:10px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:10px 12px; border-bottom:1px solid var(--sapList_BorderColor); vertical-align:middle; }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-panel { display:block; }
  `
}

// ─── 数据装载 ─────────────────────────────────────────────────────────────
// 拉全部查重规则，建字典列表 + 配置索引（dictCode → match_config）。init 时调一次。
async function loadDicts() {
  const list = (await apiGet('/api/mdm/match-configs', state.dbId)) || []
  state.dictConfigMap = {}
  const seen = []
  for (const c of list) {
    if (c.dict_code && !state.dictConfigMap[c.dict_code]) {
      state.dictConfigMap[c.dict_code] = c
      seen.push(c.dict_code)
    }
  }
  state.dicts = seen
  if (!state.dictCode && state.dicts.length) state.dictCode = state.dicts[0]
}
// 当前字典的字段对比列（从 match_config.survive_fields 动态取；缺失则空数组）
function diffFields() {
  return ((state.dictConfigMap[state.dictCode] || {}).survive_fields) || []
}
async function loadFindings() {
  // 拉全量（不分 status），前端按 findingsZone 过滤展示 + 各 zone 计数
  const d = (await apiGet(`/api/mdm/match-scan?dictCode=${encodeURIComponent(state.dictCode)}&pageSize=500`, state.dbId)) || {}
  state.findings = d.list || []
}
function findingsCount(code) { return state.findings.filter((s) => s.status === code).length }
function filteredFindings() { return state.findings.filter((s) => s.status === state.findingsZone) }
async function loadFindingDetail(scanId) {
  state.findingDetail = await apiGet(`/api/mdm/match-scan/detail?scanId=${scanId}`, state.dbId)
}
async function loadGroups() {
  const d = (await apiGet(`/api/mdm/merge-requests?dictCode=${encodeURIComponent(state.dictCode)}&pageSize=200`, state.dbId)) || {}
  state.groups = d.list || []
}
async function loadDetail(mergeId) {
  state.detail = await apiGet(`/api/mdm/merge-requests/detail?mergeId=${mergeId}`, state.dbId)
  state.rulings = {}
}

// ─── 页头（字典下拉 + tab） ────────────────────────────────────────────────
function headHtml() {
  return `<div class="pg-head">
    <div><div class="pg-title">数据管家工作台</div>
      <div class="pg-sub">查重发现项评审 · 合并/忽略 · 合并历史追溯</div></div>
    <div class="head-tools">
      <select class="dict-sel" data-dict>
        ${state.dicts.length
          ? state.dicts.map((d) => `<option value="${d}" ${state.dictCode === d ? 'selected' : ''}>${d}</option>`).join('')
          : '<option value="">（暂无查重规则）</option>'}
      </select>
    </div>
  </div>
  <div class="tab-bar">
    <button class="tab-btn ${state.tab === 'findings' ? 'active' : ''}" data-tab="findings">查重发现项</button>
    <button class="tab-btn ${state.tab === 'history' ? 'active' : ''}" data-tab="history">合并历史</button>
  </div>`
}

// ─── tab1：查重发现项 ──────────────────────────────────────────────────────
function findingsBarHtml() {
  return `<div class="zone-bar">${FINDING_ZONES.map((z) => `
    <div class="zone-tab ${state.findingsZone === z.code ? 'active' : ''}" data-fz="${z.code}">
      <span class="z-name">${z.name}</span><span class="z-count" style="font-size:16px;font-weight:600;color:var(--sapContent_LabelColor)">${findingsCount(z.code)}</span>
    </div>`).join('')}</div>`
}
function findingsQueueHtml() {
  const list = filteredFindings()
  const rows = list.length ? list.map((s) => {
    const fz = FINDING_ZONES.find((z) => z.code === s.status) || {}
    return `<tr>
    <td class="muted">${s.id}</td>
    <td>${s.cluster_key || ''}</td>
    <td>${s.member_count || 0}</td>
    <td>${s.max_score ?? ''}</td>
    <td><cmx-status-tag tone="${fz.tone || 'neutral'}" variant="subtle" size="sm">${fz.name || s.status}</cmx-status-tag></td>
    <td>${s.status === 'pending' ? `<ui5-button design="Emphasized" icon="inspect" data-freview="${s.id}">评审</ui5-button><ui5-button design="Transparent" icon="decline" data-fignore="${s.id}">忽略</ui5-button>` : ''}</td></tr>`
  }).join('') : null
  return `<cmx-panel title="发现项 · ${state.findingsZone}" icon="search">
    ${rows
      ? `<table class="tbl"><thead><tr><th>id</th><th>簇键</th><th>成员数</th><th>最高分</th><th>状态</th><th>操作</th></tr></thead><tbody>${rows}</tbody></table>`
      : `<cmx-empty-state icon="search" title="该区暂无发现项"></cmx-empty-state>`}
  </cmx-panel>`
}
function findingDiffHtml() {
  const fd = state.findingDetail || {}
  const scan = fd.scan || {}
  const members = fd.members || []
  const heads = members.map((m) => `<th>${m.id}</th>`).join('')
  const rows = diffFields().map((f) => {
    const vals = members.map((m) => `<td>${m[f] ?? ''}</td>`).join('')
    return `<tr><td>${f}</td>${vals}</tr>`
  }).join('')
  return `<h3>发现项对比 · cluster=${scan.cluster_key || ''}（${members.length} 成员）</h3>
    <table class="tbl"><thead><tr><th>字段</th>${heads}</tr></thead><tbody>${rows}</tbody></table>
    <p class="muted" style="margin-top:12px;font-size:12px">合并将默认首个成员为 master、其余为 victims（master 优先存活）；逐字段精细裁决请用「合并历史」tab。</p>
    <div class="dlg-foot">
      <ui5-button design="Transparent" id="fdBack">返回</ui5-button>
      <ui5-button design="Negative" icon="decline" id="fdIgnore">忽略</ui5-button>
      <ui5-button design="Emphasized" icon="combine" id="fdMerge">合并（首个为 master）</ui5-button>
    </div>`
}

// ─── tab2：合并历史（保留现有逻辑） ────────────────────────────────────────
function zoneCount(code) { return state.groups.filter((g) => g.status === code).length }
function filteredGroups() { return state.groups.filter((g) => g.status === state.zone) }
function zoneBarHtml() {
  return `<div class="zone-bar">${ZONES.map((z) => `
    <div class="zone-tab ${state.zone === z.code ? 'active' : ''}" data-z="${z.code}">
      <span class="z-name">${z.name}</span><span class="z-count" style="font-size:16px;font-weight:600;color:var(--sapContent_LabelColor)">${zoneCount(z.code)}</span>
    </div>`).join('')}</div>`
}
function queueHtml() {
  const list = filteredGroups()
  const rows = list.length ? list.map((g) => {
    const gz = ZONES.find((z) => z.code === g.status) || {}
    return `<tr>
    <td class="muted">${g.id}</td><td class="muted">${g.master_id ?? ''}</td><td>${g.score ?? ''}</td>
    <td><cmx-status-tag tone="${gz.tone || 'neutral'}" variant="subtle" size="sm">${gz.name || g.status}</cmx-status-tag></td>
    <td>${g.status === 'pending' ? `<ui5-button design="Emphasized" icon="inspect" data-review="${g.id}">评审</ui5-button><ui5-button design="Transparent" icon="decline" data-rej="${g.id}">驳回</ui5-button>` : ''}</td></tr>`
  }).join('') : null
  return `<cmx-panel title="合并历史 · ${state.zone}" icon="list">
    ${rows
      ? `<table class="tbl"><thead><tr><th>group</th><th>master</th><th>score</th><th>status</th><th>操作</th></tr></thead><tbody>${rows}</tbody></table>`
      : `<cmx-empty-state icon="list" title="该区暂无合并请求"></cmx-empty-state>`}
  </cmx-panel>`
}
function diffHtml() {
  const m = state.detail.master || {}; const v = (state.detail.victims || [])[0] || {}
  const rows = diffFields().map((f) => {
    const mv = m[f] ?? ''; const vv = v[f] ?? ''
    const differ = String(mv) !== String(vv)
    const r = state.rulings[f] || { pick: 'master', text: '' }
    return `<tr class="${differ ? 'diffrow' : ''}" style="${differ ? 'background:color-mix(in srgb, var(--sapCriticalColor,#e76500) 10%, transparent)' : ''}"><td>${f}</td><td>${mv}</td><td>${vv}</td>
      <td class="rule"><select data-f="${f}" data-k="pick" style="padding:5px 8px;font-size:12px;border-radius:4px;border:1px solid var(--sapField_BorderColor);background:var(--sapField_Background);color:var(--sapField_TextColor)">
        <option value="master" ${r.pick === 'master' ? 'selected' : ''}>master</option>
        <option value="victim" ${r.pick === 'victim' ? 'selected' : ''}>victim</option>
        <option value="custom" ${r.pick === 'custom' ? 'selected' : ''}>手填</option>
      </select> <input data-f="${f}" data-k="text" placeholder="手填值" value="${r.text || ''}" style="padding:5px 8px;font-size:12px;border-radius:4px;border:1px solid var(--sapField_BorderColor);background:var(--sapField_Background);color:var(--sapField_TextColor);display:${r.pick === 'custom' ? '' : 'none'}"></td></tr>`
  }).join('')
  return `<h3>红线 diff · group=${state.detail.group.id}</h3>
    <table class="tbl"><thead><tr><th>字段</th><th>master(${m.id || ''})</th><th>victim(${v.id || ''})</th><th>裁决</th></tr></thead><tbody>${rows}</tbody></table>
    <div class="dlg-foot">
      <ui5-button design="Transparent" id="stBack">返回</ui5-button>
      <ui5-button design="Negative" icon="decline" id="stReject">驳回</ui5-button>
      <ui5-button design="Emphasized" icon="combine" id="stMerge">按裁决合并</ui5-button>
    </div>`
}

// ─── 弹层（挂 document.body，fixed 铺满视口） ──────────────────────────────
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
  .mdm-dlg .dlg-foot { margin-top:16px; display:flex; justify-content:flex-end; gap:8px; }
  `
}
function openDiff(html, bindFn) {
  closeDiff()
  const mask = document.createElement('div'); mask.className = 'mdm-mask'
  mask.innerHTML = `<style>${dlgCss()}</style><div class="mdm-dlg">${html}</div>`
  dlgEl = mask
  mask.addEventListener('click', (e) => { if (e.target === mask) closeDiff() })
  if (bindFn) bindFn(mask)
  document.body.appendChild(mask)
}
function closeDiff() { if (dlgEl) { dlgEl.remove(); dlgEl = null } }

function bindFindingDiff(scope) {
  scope.querySelector('#fdBack')?.addEventListener('click', () => { closeDiff(); state.findingDetail = null })
  scope.querySelector('#fdIgnore')?.addEventListener('click', () => doFindingIgnore(state.findingDetail.scan.id))
  scope.querySelector('#fdMerge')?.addEventListener('click', async () => {
    try { await doFindingMerge() } catch (e) { cmx().cmxError?.(`合并失败：${e.message}`) }
  })
}
function bindHistoryDiff(scope) {
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

// ─── 操作 ─────────────────────────────────────────────────────────────────
async function doFindingMerge() {
  const M = cmx()
  const scan = (state.findingDetail || {}).scan || {}
  const members = (state.findingDetail || {}).members || []
  if (members.length < 2) { M.cmxWarn?.('成员不足 2，无法合并'); return }
  const masterId = members[0].id
  const victimIds = members.slice(1).map((m) => m.id)
  // targetTable/surviveFields 不传，后端从 match_config 回填；survivorship 默认 master 优先
  await apiPost('/api/mdm/merge-requests', {
    dictCode: state.dictCode, masterId, victimIds, scanId: scan.id,
  }, state.dbId)
  M.cmxInfo?.('合并成功'); closeDiff(); state.findingDetail = null; await loadFindings(); refresh()
}
async function doFindingIgnore(scanId) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({ title: '忽略发现项', message: `确认忽略发现项 ${scanId}？`, danger: true })
  if (ok === false) return
  await apiPost('/api/mdm/match-scan/ignore', { scanId: Number(scanId) }, state.dbId)
  M.cmxInfo?.('已忽略'); closeDiff(); state.findingDetail = null; await loadFindings(); refresh()
}

function collectRulings() {
  const survivorship = {}; const overrides = {}
  for (const f of diffFields()) {
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
  await apiPost('/api/mdm/merge-requests', {
    dictCode: state.dictCode, masterId, victimIds, mergeId: g.id, survivorship, overrides,
  }, state.dbId)
  M.cmxInfo?.('合并成功'); closeDiff(); state.detail = null; await loadGroups(); refresh()
}
async function doReject(id) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({ title: '驳回', message: `驳回 group=${id}？`, danger: true })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests/reject', { mergeId: Number(id), reason: '管家驳回' }, state.dbId)
  M.cmxInfo?.('已驳回'); closeDiff(); state.detail = null; await loadGroups(); refresh()
}

// ─── 渲染 / 绑定 ──────────────────────────────────────────────────────────
function viewHtml() {
  const body = state.tab === 'findings'
    ? `${findingsBarHtml()}${findingsQueueHtml()}`
    : `${zoneBarHtml()}${queueHtml()}`
  return `<div class="pg">${headHtml()}${body}</div>`
}

async function reloadCurrent() {
  if (state.tab === 'findings') await loadFindings()
  else await loadGroups()
}

function bind(root) {
  // 字典切换
  root.querySelector('[data-dict]')?.addEventListener('change', async (e) => {
    state.dictCode = e.target.value; await reloadCurrent(); refresh()
  })
  // tab 切换
  root.querySelectorAll('[data-tab]').forEach((b) => b.addEventListener('click', async () => {
    state.tab = b.dataset.tab; await reloadCurrent(); refresh()
  }))
  // 发现项 zone 切换
  root.querySelectorAll('[data-fz]').forEach((k) => k.addEventListener('click', async () => {
    state.findingsZone = k.dataset.fz; await loadFindings(); refresh()
  }))
  // 发现项操作
  root.querySelectorAll('[data-freview]').forEach((b) => b.addEventListener('click', async () => {
    await loadFindingDetail(b.dataset.freview); openDiff(findingDiffHtml(), bindFindingDiff)
  }))
  root.querySelectorAll('[data-fignore]').forEach((b) => b.addEventListener('click', () => doFindingIgnore(b.dataset.fignore)))
  // 合并历史 zone 切换
  root.querySelectorAll('[data-z]').forEach((k) => k.addEventListener('click', async () => {
    state.zone = k.dataset.z; state.detail = null; refresh()
  }))
  // 合并历史操作
  root.querySelectorAll('[data-review]').forEach((b) => b.addEventListener('click', async () => {
    await loadDetail(b.dataset.review); openDiff(diffHtml(), bindHistoryDiff)
  }))
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
      try { await loadDicts(); await reloadCurrent() } catch (e) { console.error('[steward] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
