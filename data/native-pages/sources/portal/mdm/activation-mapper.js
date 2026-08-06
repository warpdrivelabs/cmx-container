/**
 * MDM 激活映射配置器（native-page · 企业级重设计 + 修复左列表联动）。
 *
 * 布局：页头 → 左右分栏（左「映射列表」面板 / 右「映射配置」面板）。
 *   配置面板：基本信息（form-grid）+ 头映射（cmx-revo-grid 双列 select）+ 明细映射（分组）。
 * 修复：左列表 data-code/active/显示 统一用后端蛇形字段 activation_code（原误用驼峰导致点击不联动）。
 * 提示统一 cmxInfo/cmxWarn/cmxError（禁 alert）。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 * CMX 能力经 globalThis.__cmxDataComp 取用（禁止裸 import）。
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

const state = { crFields: [], cmFields: [], crLineFields: [], list: [], current: null, headerRows: [] }

// 字典坐标四元组（domain/application/module/dbId），全部来自 ctx.props，代码中不写死。
let coord = null
function coordQs(extra = {}) {
  if (!coord) return new URLSearchParams(extra).toString()
  return new URLSearchParams({
    domain: coord.domain, application: coord.application, module: coord.module, ...extra,
  }).toString()
}

// ── 元数据 ───────────────────────────────────────────────────────────────────
async function loadMeta() {
  if (!coord) return
  // doc/meta 文件名约定为 {module}_doc_meta_v1.json（与 domain/application/module 同坐标）
  const docMeta = await apiGet(`/api/doc/meta?${coordQs({ file: `${coord.module}_doc_meta_v1.json` })}`, coord.dbId)
  const layers = (docMeta && docMeta.layers) || []
  state.crFields = (layers.find((l) => l.tableName === 'cv_mdm_apply') || {}).columns || []
  state.crLineFields = (layers.find((l) => l.tableName === 'cv_mdm_apply_line') || {}).columns || []
}
async function loadTargetMeta(dictCode) {
  if (!dictCode || !coord) { state.cmFields = []; return }
  const m = await apiGet(`/api/dct/meta?${coordQs({ dict: dictCode })}`, coord.dbId)
  state.cmFields = (m && m.columns) || []
}
async function loadList() { state.list = (await apiGet('/api/mdm/activations', coord && coord.dbId)) || [] }

// 显示「字段名（字段）」更直观；caption 兼容字符串或 {zh_CN} 对象；无 caption 仅显示字段
const capOf = (f) => {
  const c = f.caption
  if (!c) return ''
  if (typeof c === 'string') return c
  return c.zh_CN || c.zh || c.label || ''
}
const disp = (f) => { const c = capOf(f); return (c && c !== f.name ? `${c}（${f.name}）` : f.name) }
const crOptions = () => state.crFields.map((f) => ({ value: f.name, label: disp(f) }))
const cmOptions = () => state.cmFields.map((f) => ({ value: f.name, label: disp(f) }))

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:16px 20px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:14px; display:flex; justify-content:space-between; align-items:flex-start; gap:12px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .layout { display:flex; gap:14px; align-items:flex-start; }
  .side { width:280px; flex:0 0 280px; }
  .main { flex:1; min-width:0; display:flex; flex-direction:column; gap:14px; }
  .side-list { display:flex; flex-direction:column; }
  .side-item { padding:10px 12px; cursor:pointer; border-bottom:1px solid var(--sapList_BorderColor); }
  .side-item:hover { background:var(--sapList_Hover_Background); }
  .side-item.active { background:color-mix(in srgb, var(--neo-cyan,#00b4d8) 14%, transparent); }
  .side-item .t { font-size:13px; font-weight:600; color:var(--sapTitleColor); }
  .side-item.active .t { color:var(--neo-cyan,#00b4d8); }
  .side-item .s { font-size:11px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(200px,1fr)); gap:14px 18px; padding:6px 2px; }
  .f-item { display:flex; flex-direction:column; gap:6px; min-width:0; }
  .f-item > label { font-size:12px; color:var(--sapContent_LabelColor); }
  .grid-wrap { height:240px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:8px 10px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor); border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:8px 10px; border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl ui5-select { width:100%; display:block; }
  .line-group { border:1px solid var(--sapList_BorderColor); border-radius:6px; padding:12px; margin-bottom:12px; }
  .line-meta { display:grid; grid-template-columns:repeat(auto-fit,minmax(160px,1fr)); gap:10px; }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-panel, cmx-toolbar { display:block; }
  `
}

function headHtml() {
  return `<div class="pg-head"><div>
    <div class="pg-title">激活映射配置</div>
    <div class="pg-sub">配置 CR 单据字段 → 主数据字段的激活映射，供激活器读取执行</div></div>
    <cmx-toolbar>
      <ui5-button design="Emphasized" icon="save" id="amSave">保存映射</ui5-button>
      <ui5-button design="Default" icon="add" id="amNew">新建</ui5-button>
      <ui5-button design="Transparent" icon="refresh" slot="actions" id="amReload">刷新</ui5-button>
    </cmx-toolbar></div>`
}

function sideHtml() {
  const items = state.list.map((it) => {
    const code = it.activation_code || ''
    const active = state.current && state.current.activation_code === code
    return `<div class="side-item ${active ? 'active' : ''}" data-code="${code}">
      <div class="t">${code || '(未命名)'}</div>
      <div class="s">${it.source_doc_type || ''} · ${it.cr_type || ''} → ${it.target_dict || ''}</div></div>`
  }).join('')
  return `<cmx-panel title="映射列表" icon="list"><div class="side-list">${items || '<div class="muted" style="padding:12px">暂无映射</div>'}</div></cmx-panel>`
}

function formHtml() {
  const c = state.current || {}
  return `
  <cmx-panel title="基本信息" icon="detail-view">
    <div class="form-grid">
      <div class="f-item"><label>映射码 activation_code</label><ui5-input id="amCode" value="${c.activation_code || ''}" placeholder="如 supplier_apply"></ui5-input></div>
      <div class="f-item"><label>来源单据类型 source_doc_type</label><ui5-input id="amSdt" value="${c.source_doc_type || ''}" placeholder="如 mdm_supplier_apply"></ui5-input></div>
      <div class="f-item"><label>变更类型 cr_type</label>
        <ui5-select id="amCrt"><ui5-option value="create" ${c.cr_type === 'create' ? 'selected' : ''}>create</ui5-option><ui5-option value="update" ${c.cr_type === 'update' ? 'selected' : ''}>update</ui5-option></ui5-select></div>
      <div class="f-item"><label>目标字典 target_dict</label><ui5-input id="amTd" value="${c.target_dict || ''}" placeholder="如 supplier"></ui5-input></div>
      <div class="f-item"><label>目标表 target_table</label><ui5-input id="amTt" value="${c.target_table || ''}" placeholder="如 cm_supplier"></ui5-input></div>
      <div class="f-item"><label>编码规则 code_rule_code</label><ui5-input id="amCrc" value="${c.code_rule_code || ''}"></ui5-input></div>
    </div>
  </cmx-panel>
  <cmx-panel title="头映射 header_mapping" icon="mapping">
    <cmx-toolbar>
      <ui5-button design="Default" icon="refresh" id="amLoadMeta">加载字段</ui5-button>
      <ui5-button design="Default" icon="add" id="amAddRow">增行</ui5-button>
    </cmx-toolbar>
    <div id="amHeaderTable"></div>
  </cmx-panel>
  <cmx-panel title="明细映射 line_mappings" icon="table-view">
    <cmx-toolbar><ui5-button design="Default" icon="add" id="amAddLine">增明细组</ui5-button></cmx-toolbar>
    <div id="amLineGroups"></div>
  </cmx-panel>`
}

function viewHtml() {
  return `<div class="pg">${headHtml()}<div class="layout"><div class="side">${sideHtml()}</div><div class="main">${state.current ? formHtml() : '<cmx-panel title="映射配置"><div class="muted" style="padding:24px">请从左侧选择或「新建」一份映射</div></cmx-panel>'}</div></div></div>`
}

// ── 头映射（普通可编辑表格，规避 revo-grid 弹层/页内时序不渲染问题）──────────
const mappingToRows = (hm) => Object.entries(hm || {}).map(([sourceField, targetField]) => ({ sourceField, targetField }))
function syncHeaderRowsFromMapping() { state.headerRows = mappingToRows(state.current?.header_mapping) }
function headerRowsToMapping() {
  const m = {}; for (const r of state.headerRows) if (r.sourceField && r.targetField) m[r.sourceField] = r.targetField
  return m
}
const optHtml = (opts, val) => `<ui5-option value=""></ui5-option>` + opts.map((o) => `<ui5-option value="${o.value}" ${o.value === val ? 'selected' : ''}>${o.label}</ui5-option>`).join('')
function renderHeaderTable() {
  const wrap = q('amHeaderTable'); if (!wrap) return
  wrap.innerHTML = `<table class="tbl"><thead><tr><th>CR 单据字段</th><th>主数据字段</th><th style="width:70px"></th></tr></thead><tbody>
    ${state.headerRows.map((r, i) => `<tr data-i="${i}">
      <td><ui5-select class="hm-src" data-i="${i}">${optHtml(crOptions(), r.sourceField)}</ui5-select></td>
      <td><ui5-select class="hm-tgt" data-i="${i}">${optHtml(cmOptions(), r.targetField)}</ui5-select></td>
      <td><ui5-button design="Transparent" icon="delete" data-hdel="${i}">删除</ui5-button></td></tr>`).join('')
      || '<tr><td colspan="3" class="muted">暂无头映射，点击「增行」添加</td></tr>'}
    </tbody></table>`
  wrap.querySelectorAll('ui5-select.hm-src').forEach((s) => s.addEventListener('change', () => { state.headerRows[+s.dataset.i].sourceField = s.value }))
  wrap.querySelectorAll('ui5-select.hm-tgt').forEach((s) => s.addEventListener('change', () => { state.headerRows[+s.dataset.i].targetField = s.value }))
  wrap.querySelectorAll('[data-hdel]').forEach((b) => b.addEventListener('click', () => { state.headerRows.splice(+b.dataset.hdel, 1); renderHeaderTable() }))
}

// ── 明细映射 ─────────────────────────────────────────────────────────────────
function renderLineGroups() {
  const box = q('amLineGroups'); if (!box) return
  const lines = state.current?.line_mappings || []
  box.innerHTML = lines.map((lm, i) => `<div class="line-group" data-idx="${i}"><div class="line-meta">
    <ui5-input data-k="lineType" placeholder="lineType" value="${lm.lineType || lm.line_type || ''}"></ui5-input>
    <ui5-input data-k="targetDict" placeholder="targetDict" value="${lm.targetDict || lm.target_dict || ''}"></ui5-input>
    <ui5-input data-k="targetTable" placeholder="targetTable" value="${lm.targetTable || lm.target_table || ''}"></ui5-input>
    <ui5-input data-k="parentIdField" placeholder="parentIdField" value="${lm.parentIdField || lm.parent_field || ''}"></ui5-input>
    <ui5-input data-k="fields" placeholder='fields JSON' value='${JSON.stringify(lm.fields || {})}'></ui5-input>
    <ui5-button design="Transparent" icon="delete" data-del="${i}">删除</ui5-button>
  </div></div>`).join('') || '<div class="muted">无明细映射</div>'
  box.querySelectorAll('.line-group').forEach((grp) => {
    const idx = +grp.dataset.idx
    grp.querySelectorAll('ui5-input[data-k]').forEach((inp) => inp.addEventListener('change', () => {
      const k = inp.dataset.k
      state.current.line_mappings[idx][k] = k === 'fields' ? safeJson(inp.value, {}) : inp.value
    }))
    grp.querySelector('[data-del]')?.addEventListener('click', () => { state.current.line_mappings.splice(idx, 1); renderLineGroups() })
  })
}
function safeJson(s, fb) { try { return JSON.parse(s) } catch { return fb } }

// ── 收集/保存 ────────────────────────────────────────────────────────────────
// native 页 DOM 在宿主 shadowRoot 内，document.getElementById 查不到 → 用渲染根作用域查询
let rootEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)
const val = (id) => { const el = q(id); return el ? (el.value || '').trim() : '' }
function collectForm() {
  const c = state.current
  c.activation_code = val('amCode'); c.source_doc_type = val('amSdt'); c.cr_type = val('amCrt')
  c.target_dict = val('amTd'); c.target_table = val('amTt'); c.code_rule_code = val('amCrc') || null
  c.header_mapping = headerRowsToMapping()
  return c
}
async function save() {
  const M = cmx()
  try {
    const cfg = collectForm()
    if (!cfg.activation_code || !cfg.source_doc_type || !cfg.target_dict) { M.cmxWarn?.('映射码/来源单据类型/目标字典 不能为空'); return }
    await apiPost('/api/mdm/activations', cfg, coord && coord.dbId)
    M.cmxInfo?.('保存成功'); await loadList(); refresh()
  } catch (e) { M.cmxError?.(`保存失败：${e.message}`) }
}
function newMapping() {
  state.current = { activation_code: '', source_doc_type: '', cr_type: 'create', target_dict: '', target_table: '', header_mapping: {}, line_mappings: [], code_rule_code: null }
  syncHeaderRowsFromMapping(); refresh()
}
function selectByCode(code) {
  state.current = state.list.find((it) => it.activation_code === code) || null
  syncHeaderRowsFromMapping(); refresh()
}

function bind(root) {
  rootEl = root
  root.querySelectorAll('.side-item').forEach((el) => el.addEventListener('click', () => selectByCode(el.dataset.code)))
  root.querySelector('#amNew')?.addEventListener('click', newMapping)
  root.querySelector('#amReload')?.addEventListener('click', async () => { await loadList(); refresh() })
  root.querySelector('#amSave')?.addEventListener('click', save)
  root.querySelector('#amLoadMeta')?.addEventListener('click', async () => { await loadTargetMeta(val('amTd')); renderHeaderTable() })
  root.querySelector('#amAddRow')?.addEventListener('click', () => { state.headerRows.push({ sourceField: '', targetField: '' }); renderHeaderTable() })
  root.querySelector('#amAddLine')?.addEventListener('click', () => {
    if (!state.current.line_mappings) state.current.line_mappings = []
    state.current.line_mappings.push({ lineType: '', targetDict: '', targetTable: '', parentIdField: '', fields: {} })
    renderLineGroups()
  })
  root.querySelector('#amTd')?.addEventListener('change', async (e) => { await loadTargetMeta((e.target.value || '').trim()); renderHeaderTable() })
  if (state.current) { renderHeaderTable(); renderLineGroups() }
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

// 从 ctx.props 读取字典坐标四元组（不写死默认值）；缺 domain/application/module 返回 null。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const c = { domain: p.domain || '', application: p.application || '', module: p.module || '', dbId: p.dbId || p.db_id || '' }
  return (c.domain && c.application && c.module) ? c : null
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      coord = readCoord(ctx)
      try { await loadMeta(); await loadList() } catch (e) { console.error('[activation-mapper] init fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
