/**
 * MDM 激活映射配置器（native-page · 对齐高保真原型重设计）。
 *
 * 布局：页头 → 左右分栏（左「映射列表」面板 / 右「映射配置」面板）。
 *   配置面板四张编号卡片：
 *     ① 基本信息（CR 路由键 → 目标字典定位）
 *     ② 编码规则与主体识别（code_rule_code ↔ subject_code_field 互斥 + 实时来源指示器）
 *     ③ 头表字段映射（CR 源字段 → cm_* 目标列，扁平 {source:target} 落库）
 *     ④ 行表映射（按 line_type 折叠组，fields 改结构化源→目标子表，告别裸 JSON）
 *
 * 设计约束（已核实后端模型 ActivationConfig）：
 *   - header_mapping 为扁平 Map<String,Value>{源字段:目标列}，激活器 plan_create 按此搬运；
 *     DCT 元数据无 fieldGroup、表内无分组列，故头映射不引入持久化分组（避免落库污染）。
 *   - line_mappings[].fields 同为扁平 {源:目标}，前端以子表编辑、收集时还原为对象。
 *   - is_active 为布尔列，侧栏以状态点 + 编辑头开关呈现。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }。
 * CMX 能力经 globalThis.__cmxDataComp 取用（禁止裸 import）。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

// 轻量 toast（成功/失败反馈，3s 自动消失，免点确定）—— 对齐 registry-center 的提示范式。
// 仅用于「操作已完成」这类轻反馈；校验警告用 cmxWarn、异常用 cmxError（需用户停下查看）。
let _toastTimer = null
function showToast (message, tone = 'ok', duration = 3000) {
  let el = document.getElementById('cmx-native-toast')
  if (!el) {
    el = document.createElement('div')
    el.id = 'cmx-native-toast'
    el.style.cssText = 'position:fixed;top:24px;left:50%;transform:translateX(-50%);z-index:99999;display:flex;align-items:center;gap:8px;padding:10px 18px;border-radius:8px;font:500 14px/1.4 var(--sapFontFamily,Arial,sans-serif);box-shadow:0 4px 16px rgba(0,0,0,.16);pointer-events:none;opacity:0;transition:opacity .18s ease'
    document.body.appendChild(el)
    const icon = document.createElement('span')
    icon.style.cssText = 'display:inline-flex;width:16px;height:16px;flex-shrink:0'
    const text = document.createElement('span')
    el.appendChild(icon); el.appendChild(text)
    el._icon = icon; el._text = text
  }
  if (_toastTimer) { clearTimeout(_toastTimer); _toastTimer = null }
  const isErr = tone === 'err'
  el.style.color = isErr ? 'var(--sapNegativeTextColor,#b00)' : 'var(--sapPositiveTextColor,#107e3e)'
  el.style.background = isErr ? 'color-mix(in srgb,#b00 10%,#fff)' : 'color-mix(in srgb,#107e3e 10%,#fff)'
  el.style.border = `1px solid ${isErr ? 'color-mix(in srgb,#b00 24%,transparent)' : 'color-mix(in srgb,#107e3e 24%,transparent)'}`
  el._icon.innerHTML = isErr
    ? '<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 12.5A5.5 5.5 0 118 2.5a5.5 5.5 0 010 11zM7.25 4h1.5v5h-1.5V4zm0 6h1.5v1.5h-1.5V10z"/></svg>'
    : '<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 1a7 7 0 100 14A7 7 0 008 1zm3.4 5.1L7 10.5 4.6 8.1l1-1L7 8.5l3.4-3.4 1 1z"/></svg>'
  el._text.textContent = String(message ?? '')
  requestAnimationFrame(() => { el.style.opacity = '1' })
  _toastTimer = setTimeout(() => { el.style.opacity = '0'; _toastTimer = null }, duration)
}

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

const state = {
  crFields: [], cmFields: [], crLineFields: [], list: [], current: null,
  headerRows: [], headerGroups: [], dictCatalog: [], codeRules: [], lineDictFields: {}, kw: '', groupBy: 'group',
}
// 行表映射的编辑态：与 state.current.line_mappings 平行的「字段行数组」缓存
const lineRowsCache = [] // [{rows:[{sourceField,targetField}]}]

// 字典坐标四元组（domain/application/module/dbId），全部来自 ctx.props，代码中不写死。
let coord = null
// 复制副本脏标记：复制出的副本虽可能撞已有 activation_code（反向已存在的覆盖场景），但语义是
// 「未保存」——期间不算已持久化（禁删除 / 隐藏复制按钮，避免误删已有或连环复制）；保存/选中/新建时复位。
let cloneDirty = false
function coordQs(extra = {}) {
  if (!coord) return new URLSearchParams(extra).toString()
  return new URLSearchParams({
    domain: coord.domain, application: coord.application, module: coord.module, ...extra,
  }).toString()
}

// ── 元数据 ───────────────────────────────────────────────────────────────────
async function loadMeta() {
  if (!coord) return
  // doc/meta 文件名约定为 {application}_doc_meta_v1.json（与 dct 文件同前缀）
  const docMeta = await apiGet(`/api/doc/meta?${coordQs({ file: `${coord.application}_doc_meta_v1.json` })}`, coord.dbId)
  const layers = (docMeta && docMeta.layers) || []
  state.crFields = (layers.find((l) => l.tableName === 'cv_mdm_apply') || {}).columns || []
  state.crLineFields = (layers.find((l) => l.tableName === 'cv_mdm_apply_line') || {}).columns || []
}
async function loadTargetMeta(dictCode) {
  if (!dictCode || !coord) { state.cmFields = []; return }
  const m = await apiGet(`/api/dct/meta?${coordQs({ dict: dictCode })}`, coord.dbId)
  state.cmFields = (m && m.columns) || []
}
// 加载目标明细字典字段（按 dictCode 缓存，供行表映射明细字段子表的源/目标下拉用）。
// line_payload 镜像明细字典字段（同头 payload 镜像头字典），故源/目标都取明细字典 columns。
async function loadLineDictFields(dictCode) {
  if (!dictCode || !coord) return []
  if (state.lineDictFields[dictCode]) return state.lineDictFields[dictCode]
  const m = await apiGet(`/api/dct/meta?${coordQs({ dict: dictCode })}`, coord.dbId)
  const fields = (m && m.columns) || []
  state.lineDictFields[dictCode] = fields
  return fields
}
async function loadList() { state.list = (await apiGet('/api/mdm/activations', coord && coord.dbId)) || [] }

// 字典目录：从同模块 DCT 定义文件取所有字典（dictCode/dictName/tableName），供目标字典帮助选择。
async function loadDictCatalog() {
  state.dictCatalog = []
  if (!coord) return
  try {
    const listData = await apiGet(`/api/definitions/list?domain=${encodeURIComponent(coord.domain)}`, coord.dbId)
    const dctItem = ((listData && listData.items) || []).find((it) => it.kind === 'DCT' && (!it.module || it.module === coord.module))
    if (!dctItem) return
    const q = new URLSearchParams({ domain: coord.domain, application: coord.application, module: coord.module, file: dctItem.file })
    const cfg = await apiGet(`/api/definitions/config?${q}`, coord.dbId)
    state.dictCatalog = ((cfg && cfg.dictionaryTables) || []).map((t) => {
      const dm = t.dictMeta || {}
      return { dictCode: dm.dictCode, dictName: dm.dictName, tableName: dm.tableName }
    }).filter((d) => d.dictCode)
  } catch (e) { console.error('[activation-mapper] loadDictCatalog fail', e) }
}

// 编码规则目录：GET /api/code/rules（与 cmx_mdm_activation 同库，故沿用 coord.dbId 即业务库）。
// 返回 { rules: [{ ruleCode, ruleName }] }，供卡片② code_rule_code 下拉选择。
async function loadCodeRules() {
  state.codeRules = []
  try {
    const d = await apiGet('/api/code/rules', coord && coord.dbId)
    state.codeRules = ((d && d.rules) || [])
      .map((r) => ({ ruleCode: r.ruleCode, ruleName: r.ruleName }))
      .filter((r) => r.ruleCode)
  } catch (e) { console.error('[activation-mapper] loadCodeRules fail', e) }
}

// 显示「字段名（字段）」更直观；caption 兼容字符串或 {zh_CN} 对象
const capOf = (f) => {
  const c = f.caption
  if (!c) return ''
  if (typeof c === 'string') return c
  return c.zh_CN || c.zh || c.label || ''
}
const disp = (f) => { const c = capOf(f); return (c && c !== f.name ? `${c}（${f.name}）` : f.name) }
// CR 源字段候选：cv_mdm_apply 全部顶层字段（本表 fields 定义 + documentFieldSets 引入的公共/引用列），
// 仅剔除纯审计技术列（id/create_*/update_*/delete_flag）和 payload/field_deltas 容器。
// documentFieldSets 引入的字段（doc_no/doc_date/entity_id/source_doc_no/doc_status 等）属「引用字段」
// （非本表 fields 定义），可选用作 CR 单据展示（目标列留空 → 不写主数据，plan_create 遇 null tgt 自动跳过）。
// + 目标字典字段（payload 段，payload.xxx 显示；plan_create 优先从 payload 取值）。
const AUDIT_COLS = new Set(['id', 'create_by', 'create_time', 'update_by', 'update_time', 'delete_flag'])
const crOptions = () => {
  const header = state.crFields
    .filter((f) => f.name !== 'payload' && f.name !== 'field_deltas' && !AUDIT_COLS.has(f.name))
    .map((f) => ({ value: f.name, label: disp(f) }))
  const payload = state.cmFields.map((f) => ({ value: f.name, label: `payload.${disp(f)}` }))
  return [...header, ...payload]
}
const cmOptions = () => state.cmFields.map((f) => ({ value: f.name, label: disp(f) }))
// 行表映射：目标明细字典候选（dictCatalog）
const dictLineOpts = () => state.dictCatalog.map((d) => ({ value: d.dictCode, label: d.dictName ? `${d.dictName}（${d.dictCode}）` : d.dictCode }))
// 某明细字典的字段选项（供挂头外键列下拉用；和明细字段子表的目标列同源）
const lineFieldOptsFor = (dict) => ((dict && state.lineDictFields[dict]) || []).map((f) => ({ value: f.name, label: disp(f) }))
const CR_TYPES = [
  { value: 'create', label: 'create — 新建' },
  { value: 'update', label: 'update — 变更' },
  { value: 'merge', label: 'merge — 合并' },
  { value: 'block', label: 'block — 冻结' },
  { value: 'flag_delete', label: 'flag_delete — 标记删除' },
]

const esc = (s) => String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]))

function styleCss() {
  return `
  .pg { height:100%; overflow:hidden; box-sizing:border-box; padding:16px 20px;
    display:flex; flex-direction:column;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { flex:0 0 auto; margin-bottom:14px; display:flex; justify-content:space-between; align-items:flex-start; gap:12px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor); margin-top:2px; }
  .layout { flex:1 1 auto; min-height:0; display:flex; gap:14px; align-items:stretch; }
  .side { width:280px; flex:0 0 280px; min-height:0; display:flex; flex-direction:column; }
  .main { flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; display:flex; flex-direction:column; }
  .main-scroll { flex:1 1 auto; min-height:0; overflow-y:auto;
    display:flex; flex-direction:column; gap:14px; padding-right:6px; }
  .main-scroll > * { flex-shrink:0; } /* 卡片保持自然高度，超出由 .main-scroll 滚动 */

  /* 侧栏（满高卡片：标题/搜索固定，列表区内部滚动）*/
  .side-card { flex:1 1 auto; min-height:0; display:flex; flex-direction:column;
    border:1px solid var(--sapList_BorderColor); border-radius:8px; overflow:hidden;
    background:color-mix(in srgb,var(--sapBackgroundColor) 92%,#000 0%); }
  .side-card-head { flex:0 0 auto; display:flex; align-items:center; gap:8px;
    padding:11px 14px; font-size:13px; font-weight:600; color:var(--sapTitleColor);
    border-bottom:1px solid var(--sapList_BorderColor);
    background:color-mix(in srgb,var(--sapBackgroundColor) 75%,#000 0%); }
  .side-card-head ui5-icon { width:1rem; height:1rem; color:var(--neo-cyan,#00b4d8); flex:0 0 auto; }
  .side-search { flex:0 0 auto; padding:10px 12px; }
  .side-search input { width:100%; box-sizing:border-box; padding:7px 10px;
    border:1px solid var(--sapList_BorderColor); border-radius:5px; font-size:13px;
    background:color-mix(in srgb,var(--sapBackgroundColor) 85%, #000 0%); color:var(--sapTextColor); }
  .side-search input:focus { outline:none; border-color:var(--neo-cyan,#00b4d8); }
  .side-list { flex:1 1 auto; min-height:0; overflow-y:auto; display:flex; flex-direction:column; }
  .side-item { flex-shrink:0; padding:10px 14px; cursor:pointer; border-left:3px solid transparent;
    border-bottom:1px solid var(--sapList_BorderColor); transition:background .12s; }
  .side-item:hover { background:var(--sapList_Hover_Background); }
  .side-item.active { background:color-mix(in srgb, var(--neo-cyan,#00b4d8) 14%, transparent); border-left-color:var(--neo-cyan,#00b4d8); }
  .side-item .row1 { display:flex; align-items:center; gap:8px; }
  .side-item .t { font-size:13px; font-weight:600; color:var(--sapTitleColor); flex:1; min-width:0;
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .side-item.active .t { color:var(--neo-cyan,#00b4d8); }
  .side-item .s { font-size:11px; color:var(--sapContent_LabelColor); margin-top:3px;
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dot { width:8px; height:8px; border-radius:50%; flex-shrink:0; }
  .dot.on { background:var(--neo-green,#2f855a); box-shadow:0 0 0 2px color-mix(in srgb,var(--neo-green,#2f855a) 22%,transparent); }
  .dot.off { background:var(--sapContent_LabelColor); opacity:.5; }
  .pill { font-size:10px; padding:1px 7px; border-radius:8px; font-weight:600; white-space:nowrap; }
  .pill-create { background:color-mix(in srgb,var(--neo-green,#2f855a) 16%,transparent); color:var(--neo-green,#2f855a); }
  .pill-update { background:color-mix(in srgb,var(--neo-orange,#c05621) 16%,transparent); color:var(--neo-orange,#c05621); }
  .pill-merge  { background:color-mix(in srgb,var(--neo-purple,#6b46c1) 16%,transparent); color:var(--neo-purple,#6b46c1); }
  .pill-block  { background:color-mix(in srgb,var(--sapContent_LabelColor) 18%,transparent); color:var(--sapTextColor); }
  .pill-other  { background:color-mix(in srgb,var(--sapContent_LabelColor) 18%,transparent); color:var(--sapContent_LabelColor); }

  /* 编辑头：标题 + 启用开关 */
  .ed-head { display:flex; align-items:center; justify-content:space-between; }
  .ed-actions { display:flex; align-items:center; gap:10px; }
  .ed-head-left { display:flex; align-items:center; gap:14px; }
  .ed-title { font-size:18px; font-weight:600; display:flex; align-items:center; gap:8px; }
  .ed-title .code { font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; font-size:14px;
    color:var(--neo-cyan,#00b4d8); }
  .sw-wrap { display:inline-flex; align-items:center; gap:8px; font-size:12px; color:var(--sapContent_LabelColor); cursor:pointer; user-select:none; }
  .sw { position:relative; width:38px; height:20px; border-radius:10px; background:var(--neo-green,#2f855a); transition:background .2s; flex-shrink:0; }
  .sw::after { content:""; position:absolute; top:2px; left:20px; width:16px; height:16px; background:#fff; border-radius:50%; transition:left .2s; box-shadow:0 1px 2px rgba(0,0,0,.25); }
  .sw.off { background:var(--sapContent_LabelColor); opacity:.55; }
  .sw.off::after { left:2px; }

  /* 提示条 */
  .banner { border-radius:6px; padding:9px 14px; font-size:12px; display:flex; align-items:flex-start; gap:8px; line-height:1.5; }
  .banner.info { background:color-mix(in srgb,var(--neo-cyan,#00b4d8) 9%,transparent); border:1px solid color-mix(in srgb,var(--neo-cyan,#00b4d8) 28%,transparent); color:var(--sapTextColor); }
  .banner.warn { background:color-mix(in srgb,var(--neo-red,#c53030) 8%,transparent); border:1px solid color-mix(in srgb,var(--neo-red,#c53030) 26%,transparent); color:var(--sapTextColor); }
  .banner code { font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; background:color-mix(in srgb,var(--sapTextColor) 8%,transparent); padding:1px 5px; border-radius:3px; font-size:11px; }
  .banner .ic { flex-shrink:0; margin-top:1px; }

  /* 卡片 */
  .card { border:1px solid var(--sapList_BorderColor); border-radius:8px; overflow:hidden;
    background:color-mix(in srgb,var(--sapBackgroundColor) 92%,#000 0%); }
  .card-head { display:flex; align-items:center; justify-content:space-between; padding:11px 16px;
    border-bottom:1px solid var(--sapList_BorderColor); background:color-mix(in srgb,var(--sapBackgroundColor) 75%,#000 0%); }
  .card-head h3 { font-size:13px; font-weight:600; margin:0; display:flex; align-items:center; gap:8px; }
  .card-head .num { width:18px; height:18px; border-radius:50%; background:var(--neo-cyan,#00b4d8); color:#fff;
    font-size:11px; display:inline-flex; align-items:center; justify-content:center; }
  .card-hint { font-size:11px; color:var(--sapContent_LabelColor); }
  .card-body { padding:16px; }

  /* 表单网格 */
  .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(200px,1fr)); gap:14px 18px; }
  .f-item { display:flex; flex-direction:column; gap:5px; min-width:0; }
  .f-item > label { font-size:12px; color:var(--sapContent_LabelColor); display:flex; align-items:center; gap:6px; }
  .f-item > label .req { color:var(--neo-red,#c53030); }
  .f-item .help { font-size:11px; color:var(--sapContent_LabelColor); opacity:.8; }
  .f-item ui5-input, .f-item ui5-select { width:100%; display:block; }
  .f-item cmx-combo-box { width:100%; display:block; }
  .f-item.locked > label { opacity:.6; }
  .f-item.locked > label::after { content:"🔒 已锁定"; font-size:10px; opacity:.7; margin-left:4px; font-weight:400; }
  .f-item.locked ui5-input, .f-item.locked ui5-select { opacity:.55; pointer-events:none; }
  .mono { font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; font-size:12px; }

  /* 来源指示器 */
  .src-ind { margin-top:14px; padding:10px 14px; border:1px dashed var(--sapList_BorderColor); border-radius:6px;
    font-size:12px; display:flex; align-items:center; gap:10px; flex-wrap:wrap; color:var(--sapContent_LabelColor); }
  .src-ind .lab { color:var(--sapTextColor); font-weight:500; }
  .src-badge { padding:3px 10px; border-radius:4px; font-weight:600; font-size:11px; }
  .src-badge.mint { background:color-mix(in srgb,var(--neo-green,#2f855a) 16%,transparent); color:var(--neo-green,#2f855a); }
  .src-badge.manual { background:color-mix(in srgb,var(--neo-orange,#c05621) 16%,transparent); color:var(--neo-orange,#c05621); }
  .src-badge.none { background:color-mix(in srgb,var(--neo-red,#c53030) 14%,transparent); color:var(--neo-red,#c53030); }

  /* 映射表 */
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:8px 10px; font-size:11px; font-weight:600; color:var(--sapContent_LabelColor); border-bottom:1px solid var(--sapList_BorderColor); }
  .tbl td { padding:6px 10px; border-bottom:1px solid var(--sapList_BorderColor); vertical-align:middle; }
  .tbl tr:last-child td { border-bottom:none; }
  .tbl ui5-select { width:100%; display:block; }
  .add-row { padding:8px 10px; text-align:center; }
  .add-btn { color:var(--neo-cyan,#00b4d8); cursor:pointer; font-size:12px; font-weight:500; }
  .add-btn:hover { text-decoration:underline; }
  .icon-btn { display:inline-flex; align-items:center; justify-content:center; width:26px; height:26px; padding:0; border:0; border-radius:6px; background:transparent; color:var(--sapContent_IconColor,#6a6d70); cursor:pointer; transition:background-color .12s,color .12s; vertical-align:middle; }
  .icon-btn ui5-icon { width:16px; height:16px; pointer-events:none; }
  .icon-btn:hover { background:var(--sapButton_Hover_Background,rgba(0,0,0,.06)); color:var(--sapHighlightColor,#0070f2); }
  .icon-btn:active { background:var(--sapButton_Active_Background,rgba(0,0,0,.12)); }
  .icon-btn.danger:hover { background:rgba(187,0,0,.1); color:var(--sapNegativeColor,#bb0000); }
  .icon-btn[disabled] { opacity:.4; cursor:default; background:transparent; color:var(--sapContent_NonInteractiveIconColor,#6a6d70); }

  /* 行表折叠组 */
  .lm { border:1px solid var(--sapList_BorderColor); border-radius:6px; margin-bottom:10px; overflow:hidden; }
  .lm-head { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; cursor:pointer;
    background:color-mix(in srgb,var(--sapBackgroundColor) 75%,#000 0%); }
  .lm-head:hover { background:var(--sapList_Hover_Background); }
  .lm-title { display:flex; align-items:center; gap:8px; font-size:13px; }
  .lm-title .chev { color:var(--sapContent_LabelColor); transition:transform .2s; display:inline-block; font-size:11px; }
  .lm.expanded .lm-title .chev { transform:rotate(90deg); }
  .lm-title .lt-tag { font-size:10px; padding:1px 6px; border-radius:3px; font-weight:600;
    background:color-mix(in srgb,var(--neo-orange,#c05621) 16%,transparent); color:var(--neo-orange,#c05621); }
  .lm-title code { font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; }
  .lm-body { display:none; border-top:1px solid var(--sapList_BorderColor); }
  .lm.expanded .lm-body { display:block; }
  .lm-meta { display:grid; grid-template-columns:repeat(auto-fit,minmax(180px,1fr)); gap:12px; padding:12px 14px;
    background:color-mix(in srgb,var(--sapBackgroundColor) 85%,#000 0%); border-bottom:1px solid var(--sapList_BorderColor); }
  .lm-fields { padding:4px 8px 8px; }

  /* 头表分组（仿 .lm 折叠组） */
  .hg { border:1px solid var(--sapList_BorderColor); border-radius:6px; margin-bottom:10px; overflow:hidden; }
  .hg-head { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; cursor:pointer;
    background:color-mix(in srgb,var(--sapBackgroundColor) 75%,#000 0%); }
  .hg-head:hover { background:var(--sapList_Hover_Background); }
  .hg-title { display:flex; align-items:center; gap:8px; font-size:13px; }
  .hg-title .chev { color:var(--sapContent_LabelColor); transition:transform .2s; display:inline-block; font-size:11px; }
  .hg.expanded .hg-title .chev { transform:rotate(90deg); }
  .hg-title .hg-tag { font-size:10px; padding:1px 6px; border-radius:3px; font-weight:600;
    background:color-mix(in srgb,var(--neo-cyan,#00b4d8) 16%,transparent); color:var(--neo-cyan,#00b4d8); }
  .hg-name { font-weight:600; }
  .hg-name-input { font:inherit; font-weight:600; border:1px solid transparent; background:transparent; padding:1px 4px; border-radius:3px; max-width:180px; }
  .hg-name-input:focus { outline:none; border-color:var(--sapContent_FocusColor,#000); background:var(--sapField_Background,#fff); }
  .hg-count { font-size:11px; color:var(--sapContent_LabelColor); }
  .hg-actions { display:flex; align-items:center; gap:10px; }
  .hg-body { display:none; border-top:1px solid var(--sapList_BorderColor); padding:8px 10px 10px; }
  .hg.expanded .hg-body { display:block; }
  .hg-tool { display:flex; align-items:center; gap:8px; margin-bottom:10px; flex-wrap:wrap; }
  .grp-sel { width:auto; min-width:100px; display:inline-block; }

  .ed-foot { flex:0 0 auto; display:flex; justify-content:flex-end; align-items:center; gap:10px;
    padding:12px 6px 2px 0;
    background:var(--sapBackgroundColor); }
  .muted { color:var(--sapContent_LabelColor); }
  cmx-panel, cmx-toolbar { display:block; }
  `
}

function headHtml() {
  return `<div class="pg-head"><div>
    <div class="pg-title">激活映射配置</div>
    <div class="pg-sub">配置 CR 单据字段 → 主数据字段的激活映射（cmx_mdm_activation），供激活器读取执行</div></div>
    <cmx-toolbar>
      <ui5-button design="Emphasized" icon="add" id="amNew">新增映射</ui5-button>
      <ui5-button design="Transparent" icon="refresh" slot="actions" id="amReload">刷新</ui5-button>
    </cmx-toolbar></div>`
}

function sideItemHtml(it) {
  const code = it.activation_code || ''
  const active = state.current && state.current.activation_code === code
  const cr = it.cr_type || ''
  const pillCls = { create: 'pill-create', update: 'pill-update', merge: 'pill-merge', block: 'pill-block' }[cr] || 'pill-other'
  return `<div class="side-item ${active ? 'active' : ''}" data-code="${esc(code)}">
    <div class="row1">
      <span class="dot ${it.is_active ? 'on' : 'off'}" title="${it.is_active ? '已启用' : '已停用'}"></span>
      <span class="t">${esc(code) || '(未命名)'}</span>
      <span class="pill ${pillCls}">${esc(cr)}</span>
    </div>
    <div class="s">${esc(it.source_doc_type || '')} → ${esc(it.target_dict || '')} · ${esc(it.target_table || '')}</div>
  </div>`
}
function sideListHtml() {
  const kw = (state.kw || '').trim().toLowerCase()
  const items = state.list
    .filter((it) => { if (!kw) return true; const c = (it.activation_code || '').toLowerCase(); return c.includes(kw) || (it.target_dict || '').toLowerCase().includes(kw) })
    .map(sideItemHtml).join('')
  return items || '<div class="muted" style="padding:12px">暂无映射，点击「新增映射」</div>'
}
function sideHtml() {
  return `<div class="side-card">
    <div class="side-card-head"><ui5-icon name="list"></ui5-icon><span>映射列表（${state.list.length}）</span></div>
    <div class="side-search"><input type="text" id="amKw" placeholder="搜索激活编码 / 目标字典…" value="${esc(state.kw)}"></div>
    <div class="side-list" id="amSideList">${sideListHtml()}</div>
  </div>`
}

// ── 卡片1：基本信息 ──────────────────────────────────────────────────────────
function cardBasic() {
  const c = state.current || {}
  // 已持久化（list 内能查到）→ 锁定 source_doc_type / cr_type（改这俩 = 换 activation_code = 另一条配置）
  const isPersisted = !!c.activation_code && state.list.some((it) => it.activation_code === c.activation_code)
  // activation_code 统一由 sdt+crt 派生（确定性 → upsert 幂等）
  const derived = c.source_doc_type && c.cr_type ? `${c.source_doc_type}__${c.cr_type}` : ''
  return `<div class="card">
    <div class="card-head"><h3><span class="num">1</span> 基本信息</h3>
      <span class="card-hint">CR 路由键 → 目标字典定位</span></div>
    <div class="card-body"><div class="form-grid">
      <div class="f-item readonly-field" style="display:none"><label>激活编码 activation_code · 自动生成</label>
        <ui5-input id="amCode" class="mono" value="${esc(derived)}" placeholder="填入来源单据类型 + 变更类型后自动生成" readonly></ui5-input>
        <span class="help">由「来源单据类型 + 变更类型」拼接，作配置主键；已保存记录不可改</span></div>
      <div class="f-item ${isPersisted ? 'readonly-field' : ''}"><label>来源单据类型 source_doc_type <span class="req">*</span>${isPersisted ? ' · 已锁定' : ''}</label>
        <ui5-input id="amSdt" class="mono" value="${esc(c.source_doc_type || '')}" placeholder="如 mdm_supplier_apply" ${isPersisted ? 'readonly' : ''}></ui5-input></div>
      <div class="f-item ${isPersisted ? 'readonly-field' : ''}"><label>变更类型 cr_type <span class="req">*</span>${isPersisted ? ' · 已锁定' : ''}</label>
        <ui5-select id="amCrt" ${isPersisted ? 'disabled' : ''}>${CR_TYPES.map((t) => `<ui5-option value="${t.value}" ${c.cr_type === t.value ? 'selected' : ''}>${esc(t.label)}</ui5-option>`).join('')}</ui5-select></div>
      <div class="f-item"><label>目标字典 target_dict <span class="req">*</span></label>
        <cmx-combo-box id="amTdCombo" data-cmx-mode="list" data-cmx-clearable="false"></cmx-combo-box>
        <span class="help">从同模块 DCT 字典目录选择</span></div>
      <div class="f-item readonly-field"><label>目标表 target_table <span class="req">*</span> · 自动带出</label>
        <ui5-input id="amTt" class="mono" value="${esc(c.target_table || '')}" placeholder="选字典后自动填入" readonly></ui5-input>
        <span class="help">由所选字典 DCT tableName 自动填入，不可编辑</span></div>
    </div></div>
  </div>`
}

// ── 卡片2：编码规则与主体识别（互斥 + 指示器）─────────────────────────────────
function cardCodeSource() {
  const c = state.current || {}
  const hasRule = !!(c.code_rule_code && c.code_rule_code.trim())
  const hasSubCode = !!(c.subject_code_field && c.subject_code_field.trim())
  const lockRule = hasSubCode && !hasRule
  const lockSub = hasRule && !hasSubCode
  return `<div class="card">
    <div class="card-head"><h3><span class="num">2</span> 编码规则与主体识别</h3>
      <span class="card-hint">cm_*.code 来源（二选一）+ 列表搜索列</span></div>
    <div class="card-body">
      <div class="banner warn"><span class="ic">⚠️</span><span><b>互斥配置</b>：<code>编码规则</code> 与 <code>主体编码字段</code> 二选一。
        选铸号规则 → code 系统生成；选主体编码字段 → code 从 payload 取。<b>不可同时生效</b>，否则激活器会静默吞掉用户手填值。</span></div>
      <div class="form-grid" style="margin-top:14px">
        <div class="f-item ${lockRule ? 'locked' : ''}" id="fldRule">
          <label>编码规则 code_rule_code</label>
          <ui5-select id="amCrc">
            <ui5-option value="" ${!hasRule ? 'selected' : ''}>（无——改用主体编码字段）</ui5-option>
            ${state.codeRules.map((r) => `<ui5-option value="${esc(r.ruleCode)}" ${r.ruleCode === c.code_rule_code ? 'selected' : ''}>${esc(r.ruleName)}（${esc(r.ruleCode)}）</ui5-option>`).join('')}
          </ui5-select>
          <span class="help">非空时激活调 cmx-code 铸号写入 cm_*.code</span></div>
        <div class="f-item">
          <label>主体名字段 subject_name_field</label>
          <ui5-select id="amSnf">${optHtml(cmOptions(), c.subject_name_field || '')}</ui5-select>
          <span class="help">前端步骤条据此从 payload 取值填 subject_name</span></div>
        <div class="f-item ${lockSub ? 'locked' : ''}" id="fldSubCode">
          <label>主体编码字段 subject_code_field</label>
          <ui5-select id="amScf">${optHtml(cmOptions(), c.subject_code_field || '')}</ui5-select>
          <span class="help">非空时从 payload 取 code，codeRule 自动禁用</span></div>
      </div>
      <div class="src-ind" id="srcInd">${srcIndHtml()}</div>
    </div>
  </div>`
}
function srcIndHtml() {
  const c = state.current || {}
  const rule = (c.code_rule_code || '').trim()
  const sub = (c.subject_code_field || '').trim()
  if (rule) {
    return `<span class="lab">📌 当前 cm_*.code 来源：</span>
      <span class="src-badge mint">系统铸号（${esc(rule)}）</span>
      <span>激活时调 cmx-code 引擎按 ${esc(rule)} 生成编码写入 cm_*.code</span>`
  }
  if (sub) {
    return `<span class="lab">📌 当前 cm_*.code 来源：</span>
      <span class="src-badge manual">用户手填（payload.${esc(sub)}）</span>
      <span>激活器从 payload.${esc(sub)} 取值写入 cm_*.code，不调铸号引擎</span>`
  }
  return `<span class="lab">📌 当前 cm_*.code 来源：</span>
    <span class="src-badge none">⚠️ 未配置</span>
    <span>激活时 cm_*.code 无来源，将触发 NOT NULL 约束错误</span>`
}

// ── 卡片3：头表字段映射 ──────────────────────────────────────────────────────
function cardHeader() {
  return `<div class="card">
    <div class="card-head"><h3><span class="num">3</span> 头表字段映射 header_mapping</h3>
      <span class="card-hint">CR 源字段 → cm_* 目标列 · 分组仅 UI 展示，落库仍扁平 {源:目标}</span></div>
    <div class="card-body">
      <div id="amHeaderTable"></div>
      <div style="text-align:center; padding:6px">
        <span class="add-btn" id="amAddGroup">+ 添加分组</span>
      </div>
    </div>
  </div>`
}

// ── 卡片4：行表映射 ──────────────────────────────────────────────────────────
function cardLines() {
  return `<div class="card">
    <div class="card-head"><h3><span class="num">4</span> 行表映射 line_mappings</h3>
      <span class="card-hint">按明细类型 line_type 路由到目标明细字典</span></div>
    <div class="card-body" style="padding:14px">
      <div id="amLineGroups"></div>
      <div style="text-align:center; padding:6px">
        <span class="add-btn" id="amAddLine">+ 添加行表映射组</span>
      </div>
    </div>
  </div>`
}

// 复制按钮：仅已持久化时显示（基于已保存配置派生）。点击弹出选择目标 cr_type 的对话框。
function cloneBtnHtml(isPersisted) {
  if (!isPersisted) return ''
  return `<ui5-button design="Transparent" icon="duplicate" id="amClone">复制</ui5-button>`
}

// 复制目标 cr_type 选择对话框（ui5-dialog）：下拉列出全部 cr_type（排除当前），选 update 默认提示
// 会清空编码规则；目标 activation_code 已存在时红字提示「将覆盖」。确认后调 doClone 执行。
function cloneDlgHtml() {
  const cur = state.current ? state.current.cr_type : ''
  const def = cur === 'create' ? 'update' : 'create'
  const opts = CR_TYPES
    .filter((t) => t.value !== cur)
    .map((t) => `<ui5-option value="${t.value}"${t.value === def ? ' selected' : ''}>${esc(t.label)}</ui5-option>`)
    .join('')
  return `
  <ui5-dialog id="amCloneDlg">
    <ui5-bar slot="header" design="Header"><ui5-title slot="startContent" level="H5">复制映射</ui5-title></ui5-bar>
    <div style="min-width:360px;max-width:520px;padding:12px 18px;box-sizing:border-box;">
      <div style="font-size:13px;margin-bottom:10px;color:var(--sapContent_LabelColor)">
        将当前映射「<b>${esc(state.current?.activation_code || '')}</b>」复制为新的变更类型：
      </div>
      <ui5-select id="amCloneCrt" style="width:100%">${opts}</ui5-select>
      <div id="amCloneHint" style="font-size:12px;margin-top:8px;min-height:16px;color:var(--sapContent_LabelColor)"></div>
    </div>
    <ui5-bar slot="footer" design="Footer">
      <ui5-button id="amCloneCancel" slot="endContent">取消</ui5-button>
      <ui5-button id="amCloneOk" slot="endContent" design="Emphasized">复制</ui5-button>
    </ui5-bar>
  </ui5-dialog>`
}

function formHtml() {
  const c = state.current || {}
  // 已持久化 = list 里能按 activation_code 查到 且 非复制副本脏态（选中已有 / 保存后均在 list；新建未保存不在 → 禁删）
  const isPersisted = !!c.activation_code && !cloneDirty && state.list.some((it) => it.activation_code === c.activation_code)
  return `
  <div class="main-scroll">
  <div class="banner info"><span class="ic">ℹ️</span><span><b>数据来源提示</b>：目标字典字段、源字段下拉均来自 <b>DCT 字典元数据</b>。先在「目标字典」选择字典，自动加载字段候选。</span></div>
  <div class="ed-head">
    <div class="ed-head-left">
      <div class="ed-title">激活映射 <span class="code">${esc(c.activation_code || '(新建)')}</span></div>
      <div class="sw-wrap" id="amActiveWrap"><span>${c.is_active ? '已启用' : '已停用'}</span><span class="sw ${c.is_active ? '' : 'off'}" id="amActiveSw"></span></div>
    </div>
    <div class="ed-actions">
      <ui5-button design="Negative" icon="delete" id="amDelete" ${isPersisted ? '' : 'disabled'}>删除</ui5-button>
      ${cloneBtnHtml(isPersisted)}
      <ui5-button design="Emphasized" icon="save" id="amSave">保存配置</ui5-button>
    </div>
  </div>
  ${cardBasic()}
  ${cardCodeSource()}
  ${cardHeader()}
  ${cardLines()}
  </div>`
}

function viewHtml() {
  return `<div class="pg">${headHtml()}<div class="layout"><div class="side">${sideHtml()}</div>
    <div class="main">${state.current ? formHtml() : '<cmx-panel title="映射配置"><div class="muted" style="padding:24px">请从左侧选择或「新增映射」一份映射</div></cmx-panel>'}</div>
  </div>${cloneDlgHtml()}</div>`
}

// ── 头映射表（普通可编辑表格，规避 revo-grid 弹层/页内时序不渲染问题）──────────
// 字段展示顺序的持久化载体是 header_groups[].fields 数组（jsonb 数组保序），而非 header_mapping 的 key 序——
// header_mapping 经 serde Map（BTreeMap 字母序）+ PG jsonb（key 无序）落库后 key 序必丢。
// 未分组字段（gi=-1）保存时收进 groupCode='__order__' 的影子组（仅作顺序载体，加载时剥离不进 UI）。
const ORDER_GROUP_CODE = '__order__'
// 行的分组归属由行自带 gi 承载（-1 = 未分组，>=0 = 分组下标），不再靠 headerGroups.fields 反查 sourceField。
// 这样空 sourceField 的新行也能稳定归属某分组，支持「在分组内直接增行」；headerGroups 运行时只存组定义，
// 其 fields 在 collectForm 时由各行 gi 推导落库（扁平 header_mapping 形态不变）。
function syncHeaderRowsFromMapping() {
  const hm = state.current?.header_mapping || {}
  const hg = state.current?.header_groups ?? state.current?.headerGroups
  const groups = Array.isArray(hg) ? hg : []
  const codeOf = (g) => g.groupCode || g.group_code || ''
  const uiGroups = groups.filter((g) => codeOf(g) !== ORDER_GROUP_CODE)
  const rows = []
  const seen = new Set()
  // 按各组（含影子组）fields 数组序展开行——恢复用户保存时排的顺序
  for (const g of groups) {
    const isShadow = codeOf(g) === ORDER_GROUP_CODE
    const gi = isShadow ? -1 : uiGroups.indexOf(g)
    for (const f of (Array.isArray(g.fields) ? g.fields : [])) {
      if (!f || seen.has(f) || !Object.prototype.hasOwnProperty.call(hm, f)) continue
      seen.add(f)
      rows.push({ sourceField: f, targetField: hm[f] || '', gi })
    }
  }
  // header_mapping 里未被任何 fields 覆盖的 key（旧数据无影子组、字段未归组）追加为未分组
  for (const [k, v] of Object.entries(hm)) {
    if (seen.has(k)) continue
    rows.push({ sourceField: k, targetField: v || '', gi: -1 })
  }
  state.headerRows = rows
}
function headerRowsToMapping() {
  // 只要求源字段有值；目标列留空时存 null（激活器 plan_create/plan_update 遇 null tgt 自动跳过，不搬运不报错）
  const m = {}; for (const r of state.headerRows) if (r.sourceField) m[r.sourceField] = r.targetField || null
  return m
}
const optHtml = (opts, val) => `<ui5-option value=""></ui5-option>` + opts.map((o) => `<ui5-option value="${esc(o.value)}" ${o.value === val ? 'selected' : ''}>${esc(o.label)}</ui5-option>`).join('')
// 从 current.header_groups 同步分组定义（仅组元信息；行归属在 syncHeaderRowsFromMapping 里标 gi）
function syncHeaderGroups() {
  const hg = state.current?.header_groups ?? state.current?.headerGroups
  // 影子组（__order__）仅是未分组字段顺序的持久化载体，不进 UI 分组定义
  state.headerGroups = Array.isArray(hg) ? hg.filter((g) => (g.groupCode || g.group_code || '') !== ORDER_GROUP_CODE).map((g) => ({
    groupCode: g.groupCode || g.group_code || '',
    groupName: g.groupName || g.group_name || g.groupCode || g.group_code || '',
  })) : []
}
// 行的分组下标（直接读 r.gi；-1 = 未分组）
const groupIndexOfRow = (r) => (r && r.gi != null && r.gi >= 0 ? r.gi : -1)
// 设置行分组（gi = -1 归未分组）
function setRowGroup(i, gi) { if (state.headerRows[i]) state.headerRows[i].gi = gi < 0 ? -1 : gi }
// 删除分组：该组行归未分组，后续分组下标前移（保证其余行 gi 仍指向正确组）
function removeHeaderGroup(gi) {
  if (gi < 0 || gi >= state.headerGroups.length) return
  state.headerGroups.splice(gi, 1)
  state.headerRows.forEach((r) => {
    if (r.gi === gi) r.gi = -1
    else if (r.gi != null && r.gi > gi) r.gi -= 1
  })
}
// 分组排序：交换相邻分组定义 + 同步行归属 gi（gi↔j 互换，行跟随组移动）。
// 顺序由 header_groups 数组序持久化（collectForm 按当前数组序导出）。
function moveHeaderGroup(gi, dir) {
  const j = gi + dir
  if (gi < 0 || gi >= state.headerGroups.length || j < 0 || j >= state.headerGroups.length) return
  const tmp = state.headerGroups[gi]; state.headerGroups[gi] = state.headerGroups[j]; state.headerGroups[j] = tmp
  // 行归属下标同步互换（同一 r.gi 只命中 gi 或 j 之一，if/elif 互斥，无 double-swap）
  state.headerRows.forEach((r) => {
    if (r.gi === gi) r.gi = j
    else if (r.gi === j) r.gi = gi
  })
}
// 字段排序：调整 headerRows 数组顺序。dir=-1 上移 / +1 下移。
// 扁平模式：纯相邻交换；分组模式：只在同一分组（含「未分组」）内找相邻同组行交换，
// 避免上移越过别组行导致组内位置不变。顺序靠 header_groups[].fields 数组（jsonb 保序）持久化。
function moveHeaderRow(i, dir) {
  const rows = state.headerRows; if (i < 0 || i >= rows.length) return
  const grouped = state.groupBy === 'group' && state.headerGroups.length
  let j = -1
  if (!grouped) {
    j = dir < 0 ? i - 1 : i + 1
  } else {
    const gi = groupIndexOfRow(rows[i])
    if (dir < 0) { for (let k = i - 1; k >= 0; k--) { if (groupIndexOfRow(rows[k]) === gi) { j = k; break } } }
    else { for (let k = i + 1; k < rows.length; k++) { if (groupIndexOfRow(rows[k]) === gi) { j = k; break } } }
  }
  if (j < 0 || j >= rows.length) return
  const tmp = rows[i]; rows[i] = rows[j]; rows[j] = tmp
}
// 渲染单行（源 | 目标 | [分组] | 操作）。分组列统一为下拉，分组内行与未分组行同风格——
// 选「未分组」即移出，选某组即归组，不再有「移出」按钮与「加入组」两套交互。
function headerRowHtml(r, i) {
  const hasGroups = state.headerGroups.length > 0
  const gi = groupIndexOfRow(r)
  const grpCell = hasGroups
    ? `<ui5-select class="hm-grp" data-i="${i}">
         <ui5-option value="-1" ${gi < 0 ? 'selected' : ''}>未分组</ui5-option>
         ${state.headerGroups.map((g, x) => `<ui5-option value="${x}" ${x === gi ? 'selected' : ''}>${esc(g.groupName)}</ui5-option>`).join('')}
       </ui5-select>`
    : ''
  return `<tr data-i="${i}">
    <td><ui5-select class="hm-src" data-i="${i}">${optHtml(crOptions(), r.sourceField)}</ui5-select></td>
    <td><ui5-select class="hm-tgt" data-i="${i}">${optHtml(cmOptions(), r.targetField)}</ui5-select></td>
    ${hasGroups ? `<td style="white-space:nowrap">${grpCell}</td>` : ''}
    <td style="white-space:nowrap"><button class="icon-btn" data-up="${i}" title="上移"><ui5-icon name="slim-arrow-up"></ui5-icon></button><button class="icon-btn" data-down="${i}" title="下移"><ui5-icon name="slim-arrow-down"></ui5-icon></button><button class="icon-btn danger" data-hdel="${i}" title="删除"><ui5-icon name="delete"></ui5-icon></button></td></tr>`
}
// 通用表格渲染：扁平模式 / 各分组卡片内的表格共用。底部「增行」按 gi 决定新行归属。
function headerTableHtml(rowList, gi) {
  const hasGroups = state.headerGroups.length > 0
  const addLabel = hasGroups
    ? (gi >= 0 ? `+ 在「${esc(state.headerGroups[gi].groupName)}」内增行` : '+ 增行（未分组）')
    : '+ 增行'
  return `<table class="tbl"><thead><tr>
      <th style="width:36%">源字段（CR 侧）</th>
      <th style="width:34%">目标列（${esc(state.current?.target_table || 'cm_*')}）</th>
      ${hasGroups ? '<th style="width:20%">分组</th>' : ''}
      <th style="width:80px"></th></tr></thead><tbody>
    ${rowList.map(({ r, i }) => headerRowHtml(r, i)).join('')
      || `<tr><td colspan="${hasGroups ? 4 : 3}" class="muted" style="padding:8px">暂无字段，点击下方增行</td></tr>`}
    </tbody></table>
    <div class="add-row"><span class="add-btn" data-grpadd="${gi}">${addLabel}</span></div>`
}
function renderHeaderTable() {
  const wrap = q('amHeaderTable'); if (!wrap) return
  if (!state.cmFields.length) {
    wrap.innerHTML = `<div class="muted" style="padding:8px">目标字典为空，先选目标字典加载字段</div>`; return
  }
  const rows = state.headerRows
  // 无分组定义或扁平展示：单表格（扁平，新行归未分组）
  if (state.groupBy === 'flat' || !state.headerGroups.length) {
    wrap.innerHTML = headerTableHtml(rows.map((r, i) => ({ r, i })), -1)
    bindHeaderEvents(wrap); return
  }
  // 分组模式：各分组折叠卡片 + 未分组区，每区独立增行（归属该区 gi）
  const groupsHtml = state.headerGroups.map((g, gi) => {
    const grpRows = rows.map((r, i) => ({ r, i })).filter(({ r }) => groupIndexOfRow(r) === gi)
    return `<div class="hg expanded" data-gi="${gi}">
      <div class="hg-head">
        <div class="hg-title">
          <span class="chev">▸</span>
          <span class="hg-tag">分组</span>
          <input class="hg-name-input" data-gi="${gi}" value="${esc(g.groupName)}">
          <span class="hg-count">${grpRows.length} 字段</span>
        </div>
        <div class="hg-actions">
          <button class="icon-btn" data-gup="${gi}" ${gi === 0 ? 'disabled' : ''} title="上移分组"><ui5-icon name="slim-arrow-up"></ui5-icon></button>
          <button class="icon-btn" data-gdown="${gi}" ${gi === state.headerGroups.length - 1 ? 'disabled' : ''} title="下移分组"><ui5-icon name="slim-arrow-down"></ui5-icon></button>
          <button class="icon-btn danger" data-gdel="${gi}" title="删除整组（字段回到未分组）"><ui5-icon name="delete"></ui5-icon></button>
        </div>
      </div>
      <div class="hg-body">${headerTableHtml(grpRows, gi)}</div>
    </div>`
  }).join('')
  const unassigned = rows.map((r, i) => ({ r, i })).filter(({ r }) => groupIndexOfRow(r) < 0)
  const ungrpHtml = `<div class="hg expanded" data-gi="-1">
    <div class="hg-head"><div class="hg-title">
      <span class="chev">▸</span>
      <span class="hg-tag" style="background:color-mix(in srgb,var(--sapContent_LabelColor) 16%,transparent);color:var(--sapContent_LabelColor)">未分组</span>
      <span class="hg-count">${unassigned.length} 字段</span>
    </div></div>
    <div class="hg-body">${headerTableHtml(unassigned, -1)}</div>
  </div>`
  wrap.innerHTML = groupsHtml + ungrpHtml
  bindHeaderEvents(wrap)
}
// 头映射事件（扁平 / 分组模式共用）：源/目标/归组下拉、删行、区内增行、上下移、
// 折叠、组名编辑、删组。归属完全由行 gi 承载，下拉改值即迁移。
function bindHeaderEvents(wrap) {
  wrap.querySelectorAll('ui5-select.hm-src').forEach((s) => s.addEventListener('change', () => {
    state.headerRows[+s.dataset.i].sourceField = s.value; renderHeaderTable()
  }))
  wrap.querySelectorAll('ui5-select.hm-tgt').forEach((s) => s.addEventListener('change', () => { state.headerRows[+s.dataset.i].targetField = s.value }))
  wrap.querySelectorAll('ui5-select.hm-grp').forEach((s) => s.addEventListener('change', () => {
    setRowGroup(+s.dataset.i, parseInt(s.value, 10)); renderHeaderTable()
  }))
  wrap.querySelectorAll('[data-hdel]').forEach((el) => el.addEventListener('click', () => {
    state.headerRows.splice(+el.dataset.hdel, 1); renderHeaderTable()
  }))
  // 区内增行：data-grpadd = 新行归属组（-1 = 未分组 / 扁平）
  wrap.querySelectorAll('[data-grpadd]').forEach((el) => el.addEventListener('click', () => {
    const gi = parseInt(el.dataset.grpadd, 10)
    state.headerRows.push({ sourceField: '', targetField: '', gi: Number.isNaN(gi) ? -1 : gi })
    renderHeaderTable()
  }))
  wrap.querySelectorAll('[data-up]').forEach((el) => el.addEventListener('click', () => { moveHeaderRow(+el.dataset.up, -1); renderHeaderTable() }))
  wrap.querySelectorAll('[data-down]').forEach((el) => el.addEventListener('click', () => { moveHeaderRow(+el.dataset.down, 1); renderHeaderTable() }))
  wrap.querySelectorAll('[data-gdel]').forEach((el) => el.addEventListener('click', () => { removeHeaderGroup(+el.dataset.gdel); renderHeaderTable() }))
  wrap.querySelectorAll('[data-gup]').forEach((el) => el.addEventListener('click', () => { moveHeaderGroup(+el.dataset.gup, -1); renderHeaderTable() }))
  wrap.querySelectorAll('[data-gdown]').forEach((el) => el.addEventListener('click', () => { moveHeaderGroup(+el.dataset.gdown, 1); renderHeaderTable() }))
  // 折叠头（跳过组名输入框 / 删组 / 增行 / 组排序，避免点这些触发折叠）
  wrap.querySelectorAll('.hg-head').forEach((h) => h.addEventListener('click', (e) => {
    if (e.target.closest('[data-gdel]') || e.target.closest('[data-gup]') || e.target.closest('[data-gdown]') || e.target.closest('.hg-name-input') || e.target.closest('[data-grpadd]')) return
    h.parentElement.classList.toggle('expanded')
  }))
  wrap.querySelectorAll('.hg-name-input').forEach((inp) => {
    inp.addEventListener('click', (e) => e.stopPropagation())
    inp.addEventListener('change', () => {
      const gi = +inp.dataset.gi; if (state.headerGroups[gi]) { state.headerGroups[gi].groupName = inp.value.trim() || state.headerGroups[gi].groupName; renderHeaderTable() }
    })
  })
}

// ── 行表映射（折叠组 + 结构化 fields 子表）────────────────────────────────────
function syncLineRowsFromMapping() {
  const lines = state.current?.line_mappings || []
  lineRowsCache.length = 0
  lines.forEach((lm) => {
    // fields 的 key 序落库时已丢（BTreeMap + jsonb，同头表），按 fieldOrder 保序数组恢复用户排的顺序；
    // 旧数据无 fieldOrder → 保持 entries 原序（sort 稳定，未命中字段排尾部不交错）。
    const order = Array.isArray(lm.fieldOrder) ? lm.fieldOrder : []
    const entries = Object.entries(lm.fields || {})
    if (order.length) {
      entries.sort((a, b) => {
        const ia = order.indexOf(a[0]); const ib = order.indexOf(b[0])
        return (ia < 0 ? order.length : ia) - (ib < 0 ? order.length : ib)
      })
    }
    lineRowsCache.push({ rows: entries.map(([sourceField, targetField]) => ({ sourceField, targetField })) })
  })
}
function lineRowsToFields(idx) {
  const m = {}; const rows = (lineRowsCache[idx] && lineRowsCache[idx].rows) || []
  for (const r of rows) if (r.sourceField && r.targetField) m[r.sourceField] = r.targetField
  return m
}
function renderLineGroups() {
  const box = q('amLineGroups'); if (!box) return
  const lines = state.current?.line_mappings || []
  if (!lines.length) { box.innerHTML = '<div class="muted" style="padding:8px">无明细映射，点击下方添加</div>'; return }
  box.innerHTML = lines.map((lm, i) => {
    const lt = lm.lineType || lm.line_type || ''
    const td = lm.targetDict || lm.target_dict || ''
    const tt = lm.targetTable || lm.target_table || ''
    const pf = lm.parentIdField || lm.parent_field || ''
    return `<div class="lm ${i === 0 ? 'expanded' : ''}" data-idx="${i}">
      <div class="lm-head">
        <div class="lm-title">
          <span class="chev">▸</span>
          <span class="lt-tag">明细类型</span>
          <b>${esc(lt || '(未填)')}</b>
          <span class="muted">→</span>
          <code>${esc(tt || '(目标表)')}</code>
        </div>
        <button class="icon-btn danger" data-lmdel="${i}" title="删除整组"><ui5-icon name="delete"></ui5-icon></button>
      </div>
      <div class="lm-body">
        <div class="lm-meta">
          <div class="f-item"><label>明细类型 line_type <span class="req">*</span></label>
            <ui5-input class="mono lm-k" data-k="lineType" data-i="${i}" value="${esc(lt)}" placeholder="如 bank_account"></ui5-input>
            <span class="help">分录的业务类型标识（需与 CR 单据明细行 line_type 一致，激活器据此匹配）</span></div>
          <div class="f-item"><label>目标明细字典 <span class="req">*</span></label>
            <ui5-select class="lm-k" data-k="targetDict" data-i="${i}">${optHtml(dictLineOpts(), td)}</ui5-select></div>
          <div class="f-item"><label>目标明细表 <span class="req">*</span></label>
            <ui5-input class="mono" data-tt-i="${i}" value="${esc(tt)}" placeholder="选目标明细字典后自动带出" readonly></ui5-input></div>
          <div class="f-item"><label>挂头表外键列 parentField</label>
            <ui5-select class="lm-k lm-kpf" data-k="parentField" data-i="${i}">${optHtml(lineFieldOptsFor(td), pf)}</ui5-select>
            <span class="help">明细表里指向头表主键的外键列（来自目标明细字典字段）</span></div>
        </div>
        <div class="lm-fields" data-lfields="${i}"></div>
      </div>
    </div>`
  }).join('')
  // 折叠头
  box.querySelectorAll('.lm-head').forEach((h) => h.addEventListener('click', (e) => { if (e.target.closest('[data-lmdel]')) return; h.parentElement.classList.toggle('expanded') }))
  // 删除整组
  box.querySelectorAll('[data-lmdel]').forEach((el) => el.addEventListener('click', () => {
    const idx = +el.dataset.lmdel
    state.current.line_mappings.splice(idx, 1); lineRowsCache.splice(idx, 1); renderLineGroups()
  }))
  // meta 输入/选择：来源分录、目标明细字典（带出 targetTable）、挂头外键列
  box.querySelectorAll('.lm-k').forEach((el) => el.addEventListener('change', () => {
    const idx = +el.dataset.i; const k = el.dataset.k
    if (!state.current.line_mappings[idx]) return
    const v = (el.value || '').trim()
    state.current.line_mappings[idx][k] = v
    const grp = el.closest('.lm'); const titleEl = grp.querySelector('.lm-title')
    if (k === 'lineType') {
      titleEl.querySelector('b').textContent = v || '(未填)'
    } else if (k === 'targetDict') {
      // 选目标明细字典 → 从 dictCatalog 带出 targetTable（写 state + 只读输入框 + 折叠头 code）
      const dict = state.dictCatalog.find((d) => d.dictCode === v)
      const table = dict ? dict.tableName : ''
      state.current.line_mappings[idx].targetTable = table
      const ttInp = grp.querySelector('[data-tt-i="' + idx + '"]'); if (ttInp) ttInp.setAttribute('value', table)
      titleEl.querySelector('code').textContent = table || '(目标表)'
      // 加载新明细字典字段（缓存）+ 重渲染该组明细字段子表 + 刷新挂头外键下拉
      if (v) loadLineDictFields(v).then(() => {
        renderLineFields(idx)
        const pfSel = grp.querySelector('.lm-kpf')
        if (pfSel) { const cur = state.current.line_mappings[idx].parentField || ''; pfSel.innerHTML = optHtml(lineFieldOptsFor(v), cur) }
      })
    }
  }))
  // 各组字段子表
  lines.forEach((_, i) => renderLineFields(i))
}
function renderLineFields(idx) {
  const box = q('amLineGroups'); if (!box) return
  const host = box.querySelector(`[data-lfields="${idx}"]`); if (!host) return
  const cache = lineRowsCache[idx] || (lineRowsCache[idx] = { rows: [] })
  const rows = cache.rows
  // 明细字段的源/目标都取自「目标明细字典」的字段（line_payload 镜像明细字典，同头 payload 镜像头字典）
  const lm = state.current?.line_mappings?.[idx]
  const dict = lm?.targetDict || lm?.target_dict || ''
  const fields = (dict && state.lineDictFields[dict]) || []
  const srcOpts = fields.map((f) => ({ value: f.name, label: `payload.${disp(f)}` }))
  const tgtOpts = fields.map((f) => ({ value: f.name, label: disp(f) }))
  host.innerHTML = `<table class="tbl"><thead><tr>
      <th style="width:42%">源字段（line_payload）</th>
      <th style="width:42%">目标列（明细表）</th>
      <th style="width:80px"></th></tr></thead><tbody>
    ${rows.map((r, ri) => `<tr data-ri="${ri}">
      <td><ui5-select class="lf-src" data-i="${idx}" data-ri="${ri}">${optHtml(srcOpts, r.sourceField)}</ui5-select></td>
      <td><ui5-select class="lf-tgt" data-i="${idx}" data-ri="${ri}">${optHtml(tgtOpts, r.targetField)}</ui5-select></td>
      <td style="white-space:nowrap"><button class="icon-btn" data-lfup="${idx}" data-ri="${ri}" title="上移"><ui5-icon name="slim-arrow-up"></ui5-icon></button><button class="icon-btn" data-lfdown="${idx}" data-ri="${ri}" title="下移"><ui5-icon name="slim-arrow-down"></ui5-icon></button><button class="icon-btn danger" data-lfdel="${idx}" data-ri="${ri}" title="删除"><ui5-icon name="delete"></ui5-icon></button></td></tr>`).join('')
      || `<tr><td colspan="3" class="muted">暂无明细字段，点击「+ 添加明细字段」</td></tr>`}
    </tbody></table>
    <div class="add-row"><span class="add-btn" data-lfadd="${idx}">+ 添加明细字段</span></div>`
  host.querySelectorAll('.lf-src').forEach((s) => s.addEventListener('change', () => { cache.rows[+s.dataset.ri].sourceField = s.value }))
  host.querySelectorAll('.lf-tgt').forEach((s) => s.addEventListener('change', () => { cache.rows[+s.dataset.ri].targetField = s.value }))
  host.querySelectorAll('[data-lfdel]').forEach((el) => el.addEventListener('click', () => { cache.rows.splice(+el.dataset.ri, 1); renderLineFields(idx) }))
  host.querySelectorAll('[data-lfup]').forEach((el) => el.addEventListener('click', () => { const ri = +el.dataset.ri; if (ri > 0) { const tmp = cache.rows[ri]; cache.rows[ri] = cache.rows[ri - 1]; cache.rows[ri - 1] = tmp; renderLineFields(idx) } }))
  host.querySelectorAll('[data-lfdown]').forEach((el) => el.addEventListener('click', () => { const ri = +el.dataset.ri; if (ri < cache.rows.length - 1) { const tmp = cache.rows[ri]; cache.rows[ri] = cache.rows[ri + 1]; cache.rows[ri + 1] = tmp; renderLineFields(idx) } }))
  host.querySelector('[data-lfadd]')?.addEventListener('click', () => { cache.rows.push({ sourceField: '', targetField: '' }); renderLineFields(idx) })
}
function safeJson(s, fb) { try { return JSON.parse(s) } catch { return fb } }

// ── 收集/保存 ────────────────────────────────────────────────────────────────
let rootEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)
const val = (id) => { const el = q(id); return el ? (el.value || '').trim() : '' }
function collectForm() {
  const c = state.current
  c.source_doc_type = val('amSdt'); c.cr_type = val('amCrt')
  // activation_code 统一由 sdt+crt 派生（确定性 → upsert 幂等）
  c.activation_code = c.source_doc_type && c.cr_type ? `${c.source_doc_type}__${c.cr_type}` : ''
  // target_dict 来自字典帮助选择（combo），target_table 由其自动带出 → 两者均已同步进 state.current
  const combo = q('amTdCombo')
  if (combo && typeof combo.getValue === 'function') c.target_dict = combo.getValue() || ''
  c.code_rule_code = val('amCrc') || null
  c.subject_name_field = val('amSnf') || null; c.subject_code_field = val('amScf') || null
  c.header_mapping = headerRowsToMapping()
  c.header_groups = state.headerGroups.map((g, gi) => ({
    groupCode: g.groupCode, groupName: g.groupName,
    fields: state.headerRows.filter((r) => groupIndexOfRow(r) === gi && r.sourceField).map((r) => r.sourceField),
  }))
  // 未分组字段顺序收进 __order__ 影子组（header_mapping 的 key 序经 BTreeMap + jsonb 落库必丢，
  // fields 数组是唯一保序通道）；加载时 syncHeaderGroups / syncHeaderRowsFromMapping 剥离。
  const looseFields = state.headerRows.filter((r) => groupIndexOfRow(r) < 0 && r.sourceField).map((r) => r.sourceField)
  if (looseFields.length) c.header_groups.push({ groupCode: ORDER_GROUP_CODE, groupName: ORDER_GROUP_CODE, fields: looseFields })
  // 行表：把缓存行还原为 fields 扁平对象；字段顺序另存 fieldOrder 保序数组（同头表原理）
  c.line_mappings = (c.line_mappings || []).map((lm, i) => ({
    lineType: lm.lineType || lm.line_type || '',
    targetDict: lm.targetDict || lm.target_dict || '',
    targetTable: lm.targetTable || lm.target_table || '',
    parentIdField: lm.parentIdField || lm.parent_field || lm.parentField || '',
    fields: lineRowsToFields(i),
    fieldOrder: ((lineRowsCache[i] && lineRowsCache[i].rows) || []).filter((r) => r.sourceField && r.targetField).map((r) => r.sourceField),
  }))
  return c
}
async function save() {
  const M = cmx()
  try {
    const cfg = collectForm()
    if (!cfg.source_doc_type || !cfg.target_dict) { M.cmxWarn?.('来源单据类型 / 目标字典 不能为空'); return }
    await apiPost('/api/mdm/activations', cfg, coord && coord.dbId)
    cloneDirty = false; showToast('保存成功', 'ok'); await loadList(); refresh()
  } catch (e) { M.cmxError?.(`保存失败：${e.message}`) }
}
// 删除当前映射（硬删除，二次确认）。删除后返回空态并刷新侧栏列表。
async function delMapping() {
  const M = cmx()
  const c = state.current
  if (!c || !c.activation_code) { M.cmxWarn?.('请先选择要删除的映射'); return }
  // 仅已持久化的映射可删（新建未保存的不在 list，按钮本应禁用——此处兜底防绕过）
  if (!state.list.some((it) => it.activation_code === c.activation_code)) { M.cmxWarn?.('该映射尚未保存，无需删除'); return }
  const ok = await M.cmxConfirm?.({ title: '删除映射', message: `确认删除激活映射「${c.activation_code}」？此操作不可恢复。`, danger: true })
  if (!ok) return
  try {
    await apiPost('/api/mdm/activations/delete', { activationCode: c.activation_code }, coord && coord.dbId)
    showToast(`已删除「${c.activation_code}」`, 'ok'); state.current = null; await loadList(); refresh()
  } catch (e) { M.cmxError?.(`删除失败：${e.message}`) }
}
// 打开复制对话框：校验当前配置已持久化 → 绑定下拉/确认/取消事件 → 显示。下拉切换实时提示重复覆盖风险。
function openCloneDlg() {
  const M = cmx()
  const src = state.current
  if (!src || !src.activation_code) { M.cmxWarn?.('请先选择要复制的映射'); return }
  const dlg = q('amCloneDlg'); if (!dlg) return
  // 下拉切换时实时提示：目标 activation_code 已存在 → 红字「将覆盖」；update → 提示会清空编码规则
  const updateHint = () => {
    const target = (q('amCloneCrt')?.value || '').trim()
    const hint = q('amCloneHint'); if (!hint) return
    if (!target) { hint.textContent = ''; return }
    const dupCode = `${src.source_doc_type}__${target}`
    if (state.list.some((it) => it.activation_code === dupCode)) {
      hint.textContent = `⚠「${dupCode}」已存在，保存将覆盖原配置`
      hint.style.color = 'var(--sapNegativeElementColor,#bb0000)'
    } else if (target === 'update') {
      hint.textContent = 'update 不铸号，将自动清空编码规则 code_rule_code'
      hint.style.color = 'var(--sapContent_LabelColor)'
    } else {
      hint.textContent = ''
    }
  }
  q('amCloneCrt')?.addEventListener('change', updateHint)
  q('amCloneOk').onclick = () => {
    const target = (q('amCloneCrt')?.value || '').trim()
    if (!target) { M.cmxWarn?.('请选择目标变更类型'); return }
    dlg.open = false; doClone(target)
  }
  q('amCloneCancel').onclick = () => { dlg.open = false }
  updateHint()
  dlg.open = true
}
// 执行复制：深拷贝当前配置 → 改 cr_type → activation_code 由 sdt__crt 派生（与原配置不冲突）→
// update 清空 code_rule_code（update 分支不铸号）→ 进入未保存编辑态（cloneDirty）。
function doClone(target) {
  const src = state.current
  const dup = JSON.parse(JSON.stringify(src)) // 配置均为 JSON 可序列化数据，深拷贝安全
  dup.cr_type = target
  dup.activation_code = `${src.source_doc_type}__${target}`
  if (target === 'update') dup.code_rule_code = null // update 分支不铸号，清空规则
  cloneDirty = true
  state.current = dup
  state.cmFields = []
  syncHeaderRowsFromMapping(); syncHeaderGroups(); syncLineRowsFromMapping()
  // 目标字典不变，重新加载字段候选（头/明细/主体字段下拉才有项）
  const td = state.current.target_dict
  if (td) { loadTargetMeta(td).then(onTargetMetaLoaded).catch(() => {}) }
  refresh()
  showToast(`已复制为「${target}」配置，检查后请点「保存配置」`, 'ok')
}
function newMapping() {
  cloneDirty = false
  state.current = { activation_code: '', source_doc_type: '', cr_type: 'create', target_dict: '', target_table: '', is_active: true, header_mapping: {}, line_mappings: [], code_rule_code: null, subject_name_field: null, subject_code_field: null, header_groups: [] }
  state.cmFields = []
  syncHeaderRowsFromMapping(); syncHeaderGroups(); syncLineRowsFromMapping(); refresh()
}
function selectByCode(code) {
  cloneDirty = false
  state.current = state.list.find((it) => it.activation_code === code) || null
  state.cmFields = []
  syncHeaderRowsFromMapping(); syncHeaderGroups(); syncLineRowsFromMapping()
  // 选中后异步拉目标字典字段，让头/明细下拉有候选
  const td = state.current && state.current.target_dict
  if (td) { loadTargetMeta(td).then(onTargetMetaLoaded).catch(() => {}) }
  refresh()
}

// 互斥联动：rule 非空 → 锁 subCode；subCode 非空 → 锁 rule。同步指示器。
function applyMutex() {
  const fldRule = q('fldRule'); const fldSub = q('fldSubCode'); if (!fldRule || !fldSub) return
  const rule = (val('amCrc')); const sub = val('amScf')
  fldRule.classList.toggle('locked', !!(sub && !rule))
  fldSub.classList.toggle('locked', !!(rule && !sub))
  const ind = q('srcInd'); if (ind) ind.innerHTML = srcIndHtml()
}

// 目标字典帮助选择：选中后自动带出 target_table（只读）+ 加载该字典字段候选
function onDictChange(e) {
  const code = e.detail.id
  const dict = state.dictCatalog.find((d) => d.dictCode === code)
  if (state.current) {
    state.current.target_dict = code || ''
    state.current.target_table = dict ? dict.tableName : ''
  }
  const tt = q('amTt'); if (tt) tt.value = dict ? dict.tableName : ''
  if (code) loadTargetMeta(code).then(onTargetMetaLoaded).catch(() => {})
}
// 目标字典字段加载完成后，统一刷新所有依赖 cmFields 的下拉/表格（头映射、行明细、主体名/编码字段）
async function onTargetMetaLoaded() {
  fillSubjectSelects()
  renderHeaderTable()
  // 先加载各 line_mapping 的目标明细字典字段（缓存），再渲染明细字段子表
  const lms = state.current?.line_mappings || []
  await Promise.all(lms.map(async (lm) => {
    const d = lm?.targetDict || lm?.target_dict || ''
    if (d) await loadLineDictFields(d)
  }))
  // 字段已加载，重渲染行表组（挂头外键下拉 + 明细字段子表都有选项）
  renderLineGroups()
}
// 卡片② 主体名字段/主体编码字段下拉（选项来自目标字典 DCT columns）。这两个 select 在 formHtml
// 静态渲染时 cmFields 可能尚未加载 → 选项为空；故在 loadTargetMeta 完成后补填。
function fillSubjectSelects() {
  const c = state.current || {}
  const opts = cmOptions()
  ;['amSnf', 'amScf'].forEach((id) => {
    const sel = q(id); if (!sel) return
    const cur = id === 'amSnf' ? (c.subject_name_field || '') : (c.subject_code_field || '')
    sel.innerHTML = '<ui5-option value=""></ui5-option>' + opts.map((o) => `<ui5-option value="${esc(o.value)}" ${o.value === cur ? 'selected' : ''}>${esc(o.label)}</ui5-option>`).join('')
  })
}
// 初始化 cmx-combo-box（本地字典目录 + list 模式 + 可搜索）；元素未升级时等 whenDefined
function initCombo() {
  const el = q('amTdCombo'); if (!el) return
  const M = cmx()
  const { CmxDataSet, CmxColumnModel, CmxColumn } = M
  if (!CmxDataSet || !CmxColumnModel || !CmxColumn) return
  const fill = () => {
    if (typeof el.setDataSet !== 'function') return false
    const ds = new CmxDataSet({ datasetId: 'act-dict-catalog' })
    state.dictCatalog.forEach((d) => ds.addRow({ id: d.dictCode, dictCode: d.dictCode, dictName: d.dictName, tableName: d.tableName }))
    el.setDataSet(ds)
    el.setColumnModel(new CmxColumnModel({ members: [
      new CmxColumn({ id: 'dictCode', caption: '编码', type: 'text' }),
      new CmxColumn({ id: 'dictName', caption: '名称', type: 'text' }),
    ] }))
    el.setMode('list')
    el.setPlaceholder('选择目标字典')
    el.setValue(state.current?.target_dict || null, { silent: true })
    el.addEventListener('cmx-combo-value-change', onDictChange)
    return true
  }
  if (!fill()) customElements.whenDefined('cmx-combo-box').then(fill).catch(() => {})
}

function bind(root) {
  rootEl = root
  root.querySelector('#amKw')?.addEventListener('input', (e) => {
    state.kw = e.target.value
    const list = root.querySelector('#amSideList'); if (!list) return
    list.innerHTML = sideListHtml() // 只重渲列表项，搜索框保持焦点不动
    bindSide(root)
  })
  bindSide(root)
  root.querySelector('#amNew')?.addEventListener('click', newMapping)
  root.querySelector('#amReload')?.addEventListener('click', async () => { await loadList(); refresh() })
  root.querySelector('#amSave')?.addEventListener('click', save)
  root.querySelector('#amClone')?.addEventListener('click', () => openCloneDlg())
  root.querySelector('#amDelete')?.addEventListener('click', () => delMapping())
  // 启用开关
  root.querySelector('#amActiveWrap')?.addEventListener('click', () => {
    if (!state.current) return
    state.current.is_active = !state.current.is_active
    const sw = q('amActiveSw'); if (sw) sw.classList.toggle('off', !state.current.is_active)
    const wrap = q('amActiveWrap'); if (wrap) wrap.querySelector('span').textContent = state.current.is_active ? '已启用' : '已停用'
    // 同步侧栏状态点
    const code = state.current.activation_code
    const dot = root.querySelector(`.side-item[data-code="${cssEsc(code)}"] .dot`)
    if (dot) dot.classList.toggle('on', state.current.is_active)
  })
  // 卡片1 目标字典：cmx-combo-box 帮助选择（选中自动带出 target_table + 加载字段）
  initCombo()
  // 卡片1 来源单据类型/变更类型 → 实时派生激活编码（只读展示框）。已保存记录两键锁定，不会触发。
  const refreshDerivedCode = () => {
    const sdt = val('amSdt'); const crt = val('amCrt')
    if (!state.current) return
    state.current.source_doc_type = sdt; state.current.cr_type = crt
    const code = sdt && crt ? `${sdt}__${crt}` : ''
    state.current.activation_code = code
    const codeEl = q('amCode'); if (codeEl) codeEl.value = code
  }
  root.querySelector('#amSdt')?.addEventListener('input', refreshDerivedCode)
  root.querySelector('#amCrt')?.addEventListener('change', refreshDerivedCode)
  // 头映射：增行入口已下沉到每个分区底部（data-grpadd）；「+ 添加分组」在头表区下方（add-btn 样式）
  // 添加分组：默认名「分组N」，点组名输入框可改名（免 prompt）。有分组即自动按分组展示。
  root.querySelector('#amAddGroup')?.addEventListener('click', () => {
    const n = state.headerGroups.length + 1
    state.headerGroups.push({ groupCode: 'group_' + Date.now(), groupName: `分组${n}` })
    renderHeaderTable()
  })
  // 卡片2 互斥：rule 非空 → 清并锁 subCode；subCode 非空 → 清并锁 rule。始终同步 state 与指示器。
  root.querySelector('#amCrc')?.addEventListener('change', () => {
    const v = val('amCrc'); if (state.current) state.current.code_rule_code = v || null
    if (v) { // 选了规则 → 清空主体编码字段并锁定
      if (state.current) state.current.subject_code_field = null
      const sc = q('amScf'); if (sc) sc.value = ''
    }
    applyMutex()
  })
  root.querySelector('#amScf')?.addEventListener('change', () => {
    const v = val('amScf'); if (state.current) state.current.subject_code_field = v || null
    if (v) { // 选了主体编码字段 → 清空规则并锁定
      if (state.current) state.current.code_rule_code = null
      const cr = q('amCrc'); if (cr) cr.value = ''
    }
    applyMutex()
  })
  // 行表映射
  root.querySelector('#amAddLine')?.addEventListener('click', () => {
    if (!state.current.line_mappings) state.current.line_mappings = []
    state.current.line_mappings.push({ lineType: '', targetDict: '', targetTable: '', parentField: '', fields: {} })
    syncLineRowsFromMapping(); renderLineGroups()
  })
  if (state.current) { renderHeaderTable(); renderLineGroups() }
}
// 侧栏项点击绑定（独立出来，供搜索重渲复用）
function bindSide(root) {
  root.querySelectorAll('.side-item').forEach((el) => el.addEventListener('click', () => selectByCode(el.dataset.code)))
}
// CSS 选择器转义（data-code 可能含特殊字符）
function cssEsc(s) { return String(s == null ? '' : s).replace(/["\\]/g, '\\$&') }

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

// 从 workspace.context（框架 openNode 注入）或 ctx.props 读取字典坐标四元组，不写死默认值。
// domain/application/module 缺任一返回 null。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  const c = {
    domain: get('domain') || p.domain || '',
    application: get('application') || p.application || '',
    module: get('module') || p.module || '',
    dbId: p.dbId || p.db_id || '',
  }
  return (c.domain && c.application && c.module) ? c : null
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      coord = readCoord(ctx)
      // 各加载器独立 try/catch：doc/dct 元数据缺失不能阻断激活列表加载
      try { await loadMeta() } catch (e) { console.error('[activation-mapper] loadMeta fail', e) }
      try { await loadDictCatalog() } catch (e) { console.error('[activation-mapper] loadDictCatalog fail', e) }
      try { await loadCodeRules() } catch (e) { console.error('[activation-mapper] loadCodeRules fail', e) }
      try { await loadList() } catch (e) { console.error('[activation-mapper] loadList fail', e) }
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
