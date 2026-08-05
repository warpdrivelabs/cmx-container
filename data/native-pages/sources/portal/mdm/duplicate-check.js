/**
 * MDM 查重候选台（native-page · 企业级重设计）。
 *
 * 布局：页头 → 工具面板（记录选择 + 查重按钮）→ 候选面板（score + 裁决 status-tag + 合并）
 * → 合并请求面板（列表 + 还原）。提示统一 cmxInfo/cmxError/cmxConfirm。
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

const DECISION_META = {
  AutoMerge: { name: '自动合并', tone: 'success' },
  Review: { name: '人工评审', tone: 'warning' },
  NoMatch: { name: '不匹配', tone: 'neutral' },
}
const state = { dbId: '', dictCode: 'supplier', records: [], currentId: null, result: null, groups: [] }

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:14px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .pg-body { display:flex; flex-direction:column; gap:14px; }
  .bar { display:flex; gap:10px; align-items:flex-end; flex-wrap:wrap; padding:6px 2px; }
  .bar .f-item { display:flex; flex-direction:column; gap:6px; min-width:280px; }
  .bar label { font-size:12px; color:var(--sapContent_LabelColor); }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:10px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor);
    border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:10px 12px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background); }
  .muted { color:var(--sapContent_LabelColor); }
  .score { font-weight:600; }
  cmx-panel, cmx-toolbar { display:block; }
  `
}

async function loadRecords() {
  const h = { Accept: 'application/json' }; if (state.dbId) h.db_id = state.dbId
  const res = await fetch(`/api/dct/export?domain=basic&application=dataplatform&module=mdm&dict=${encodeURIComponent(state.dictCode)}`,
    { headers: h, credentials: 'same-origin' })
  const text = await res.text()
  state.records = text.split('\n').filter((l) => l.trim()).map((l) => { try { return JSON.parse(l) } catch { return null } }).filter(Boolean)
  if (state.records.length && !state.currentId) state.currentId = state.records[0].id
}
async function loadGroups() {
  state.groups = (await apiGet(`/api/mdm/merge-requests?dictCode=${encodeURIComponent(state.dictCode)}`, state.dbId)) || []
}

function headHtml() {
  return `<div class="pg-head"><div class="pg-title">主数据查重</div>
    <div class="pg-sub">识别一物多码：选择记录查重，按裁决合并或提交管家评审</div></div>`
}

function barHtml() {
  const opts = state.records.map((r) => `<ui5-option value="${r.id}" ${String(r.id) === String(state.currentId) ? 'selected' : ''}>${r.id} · ${r.name || ''} · ${r.credit_code || ''}</ui5-option>`).join('')
  return `<cmx-panel title="查重条件" icon="search">
    <div class="bar">
      <div class="f-item"><label>目标记录（supplier）</label><ui5-select id="dcRecord">${opts || '<ui5-option value="">（无 published 记录）</ui5-option>'}</ui5-select></div>
      <ui5-button design="Emphasized" icon="search" id="dcFind">查重</ui5-button>
      <ui5-button design="Transparent" icon="refresh" id="dcReload">刷新</ui5-button>
    </div>
  </cmx-panel>`
}

function candHtml() {
  const cands = (state.result && state.result.candidates) || []
  const body = cands.length ? cands.map((c) => {
    const rec = state.records.find((r) => String(r.id) === String(c.recordId)) || {}
    const m = DECISION_META[c.decision] || { name: c.decision, tone: 'neutral' }
    return `<tr><td class="muted">${c.recordId}</td><td>${rec.name || ''}</td><td>${rec.credit_code || ''}</td>
      <td class="score">${c.score}</td><td><cmx-status-tag tone="${m.tone}" variant="subtle" dot size="sm">${m.name}</cmx-status-tag></td>
      <td><ui5-button design="Default" icon="combine" data-merge="${c.recordId}">合并（当前为 master）</ui5-button></td></tr>`
  }).join('') : null
  return `<cmx-panel title="查重候选" icon="duplicate">
    ${body
      ? `<table class="tbl"><thead><tr><th>候选ID</th><th>名称</th><th>信用代码</th><th>score</th><th>裁决</th><th>操作</th></tr></thead><tbody>${body}</tbody></table>`
      : `<cmx-empty-state icon="search" title="暂无候选" description="选择记录后点击「查重」开始匹配"></cmx-empty-state>`}
  </cmx-panel>`
}

function groupHtml() {
  const rows = state.groups.slice(0, 10)
  const body = rows.length ? rows.map((g) => `<tr><td class="muted">${g.id}</td><td>${g.status || ''}</td><td>${g.score ?? ''}</td><td class="muted">${g.master_id ?? ''}</td>
    <td>${(g.status === 'reviewed' || g.status === 'auto_merged') ? `<ui5-button design="Transparent" icon="reset" data-undo="${g.id}">还原</ui5-button>` : ''}</td></tr>`).join('') : null
  return `<cmx-panel title="合并请求（最近 10 条）" icon="history">
    ${body
      ? `<table class="tbl"><thead><tr><th>group</th><th>status</th><th>score</th><th>master</th><th>操作</th></tr></thead><tbody>${body}</tbody></table>`
      : `<cmx-empty-state icon="history" title="暂无合并请求"></cmx-empty-state>`}
  </cmx-panel>`
}

function viewHtml() {
  return `<div class="pg">${headHtml()}<div class="pg-body">${barHtml()}${candHtml()}${groupHtml()}</div></div>`
}

async function runFind() {
  const M = cmx()
  if (!state.currentId) { M.cmxWarn?.('请先选择记录'); return }
  state.result = await apiPost('/api/mdm/records/find-duplicates', { dictCode: state.dictCode, recordId: Number(state.currentId) }, state.dbId)
}
async function doMerge(victimId) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({ title: '合并', message: `确认合并？master=${state.currentId}，victim=${victimId}（victim 将置 merged）`, danger: true })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests', { dictCode: state.dictCode, masterId: Number(state.currentId), victimIds: [Number(victimId)] }, state.dbId)
  M.cmxInfo?.('合并成功'); await refreshData()
}
async function doUndo(mergeId) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({ title: '还原', message: `确认还原 mergeId=${mergeId}？` })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests/undo', { mergeId: Number(mergeId) }, state.dbId)
  M.cmxInfo?.('已还原'); await refreshData()
}
async function refreshData() { await loadRecords(); await loadGroups(); refresh() }

function bind(root) {
  root.querySelector('#dcRecord')?.addEventListener('change', (e) => { state.currentId = e.target.value })
  root.querySelector('#dcFind')?.addEventListener('click', async () => { try { await runFind(); await loadGroups(); refresh() } catch (e) { cmx().cmxError?.(`查重失败：${e.message}`) } })
  root.querySelector('#dcReload')?.addEventListener('click', refreshData)
  root.querySelectorAll('[data-merge]').forEach((b) => b.addEventListener('click', async () => { try { await doMerge(b.dataset.merge) } catch (e) { cmx().cmxError?.(`合并失败：${e.message}`) } }))
  root.querySelectorAll('[data-undo]').forEach((b) => b.addEventListener('click', async () => { try { await doUndo(b.dataset.undo) } catch (e) { cmx().cmxError?.(`还原失败：${e.message}`) } }))
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
      try { await loadRecords(); await loadGroups() } catch (e) { console.error('[duplicate-check] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
