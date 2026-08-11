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
// CR 源字段候选：CR 头公共列（subject_name/subject_code）+ 目标字典全部字段（payload 段，
// 裸名 value、payload.xxx 显示）。源/目标同取 cmFields 全集，不做过滤——所有引用字段都展示，
// 由用户自行决定映射哪些。
const crOptions = () => {
  const common = state.crFields
    .filter((f) => ['subject_name', 'subject_code'].includes(f.name))
    .map((f) => ({ value: f.name, label: disp(f) }))
  const payload = state.cmFields.map((f) => ({ value: f.name, label: `payload.${disp(f)}` }))
  return [...common, ...payload]
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
  .del-btn { color:var(--sapContent_LabelColor); cursor:pointer; font-size:12px; }
  .del-btn:hover { color:var(--neo-red,#c53030); }
  .row-move { color:var(--sapContent_LabelColor); cursor:pointer; font-size:13px; padding:0 2px; line-height:1; }
  .row-move:hover { color:var(--neo-cyan,#00b4d8); }

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
  return `<div class="card">
    <div class="card-head"><h3><span class="num">1</span> 基本信息</h3>
      <span class="card-hint">CR 路由键 → 目标字典定位</span></div>
    <div class="card-body"><div class="form-grid">
      <div class="f-item"><label>激活编码 activation_code <span class="req">*</span></label>
        <ui5-input id="amCode" class="mono" value="${esc(c.activation_code || '')}" placeholder="如 supplier_apply"></ui5-input></div>
      <div class="f-item"><label>来源单据类型 source_doc_type <span class="req">*</span></label>
        <ui5-input id="amSdt" class="mono" value="${esc(c.source_doc_type || '')}" placeholder="如 mdm_supplier_apply"></ui5-input></div>
      <div class="f-item"><label>变更类型 cr_type <span class="req">*</span></label>
        <ui5-select id="amCrt">${CR_TYPES.map((t) => `<ui5-option value="${t.value}" ${c.cr_type === t.value ? 'selected' : ''}>${esc(t.label)}</ui5-option>`).join('')}</ui5-select></div>
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
      <div class="hg-tool">
        <ui5-button design="Default" icon="add" id="amAddRow">增行</ui5-button>
        <ui5-button design="Transparent" icon="group" id="amAddGroup">+ 添加分组</ui5-button>
        <span style="flex:1"></span>
        <span class="muted" style="font-size:11px">展示：</span>
        <ui5-select id="amGroupBy" class="grp-sel">
          <ui5-option value="group" ${state.groupBy === 'group' ? 'selected' : ''}>按分组</ui5-option>
          <ui5-option value="flat" ${state.groupBy === 'flat' ? 'selected' : ''}>扁平</ui5-option>
        </ui5-select>
      </div>
      <div id="amHeaderTable"></div>
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

function formHtml() {
  const c = state.current || {}
  // 已持久化 = list 里能按 activation_code 查到（选中已有 / 保存后均在 list；新建未保存不在 → 禁删）
  const isPersisted = !!c.activation_code && state.list.some((it) => it.activation_code === c.activation_code)
  return `
  <div class="main-scroll">
  <div class="banner info"><span class="ic">ℹ️</span><span><b>数据来源提示</b>：目标字典字段、源字段下拉均来自 <b>DCT 字典元数据</b>。先在「目标字典」选择字典，自动加载字段候选。</span></div>
  <div class="ed-head">
    <div class="ed-title">激活映射 <span class="code">${esc(c.activation_code || '(新建)')}</span></div>
    <div class="sw-wrap" id="amActiveWrap"><span>${c.is_active ? '已启用' : '已停用'}</span><span class="sw ${c.is_active ? '' : 'off'}" id="amActiveSw"></span></div>
  </div>
  ${cardBasic()}
  ${cardCodeSource()}
  ${cardHeader()}
  ${cardLines()}
  </div>
  <div class="ed-foot">
    <ui5-button design="Negative" icon="delete" id="amDelete" ${isPersisted ? '' : 'disabled'}>删除</ui5-button>
    <ui5-button design="Emphasized" icon="save" id="amSave">保存配置</ui5-button>
  </div>`
}

function viewHtml() {
  return `<div class="pg">${headHtml()}<div class="layout"><div class="side">${sideHtml()}</div>
    <div class="main">${state.current ? formHtml() : '<cmx-panel title="映射配置"><div class="muted" style="padding:24px">请从左侧选择或「新增映射」一份映射</div></cmx-panel>'}</div>
  </div></div>`
}

// ── 头映射表（普通可编辑表格，规避 revo-grid 弹层/页内时序不渲染问题）──────────
const mappingToRows = (hm) => Object.entries(hm || {}).map(([sourceField, targetField]) => ({ sourceField, targetField }))
function syncHeaderRowsFromMapping() { state.headerRows = mappingToRows(state.current?.header_mapping) }
function headerRowsToMapping() {
  const m = {}; for (const r of state.headerRows) if (r.sourceField && r.targetField) m[r.sourceField] = r.targetField
  return m
}
const optHtml = (opts, val) => `<ui5-option value=""></ui5-option>` + opts.map((o) => `<ui5-option value="${esc(o.value)}" ${o.value === val ? 'selected' : ''}>${esc(o.label)}</ui5-option>`).join('')
// 从 current.header_groups 同步到 state.headerGroups（list 返 snake；兼容历史 camel 数据）
function syncHeaderGroups() {
  // list 返回 DB row（snake: header_groups）；save 后 current 可能带 header_groups。统一读 snake。
  const hg = state.current?.header_groups ?? state.current?.headerGroups
  state.headerGroups = Array.isArray(hg) ? hg.map((g) => ({
    groupCode: g.groupCode || g.group_code || '',
    groupName: g.groupName || g.group_name || g.groupCode || g.group_code || '',
    fields: Array.isArray(g.fields) ? [...g.fields] : [],
  })) : []
}
// 查 sourceField 所属组下标（-1 = 未分组）。空 sourceField 视为未分组。
function groupIndexOf(sourceField) {
  if (!sourceField) return -1
  return state.headerGroups.findIndex((g) => Array.isArray(g.fields) && g.fields.includes(sourceField))
}
// 把 sourceField 从所有组移除，再加入 toGi（-1 = 仅移除/归未分组）
function moveRowGroup(sourceField, toGi) {
  if (!sourceField) return
  state.headerGroups.forEach((g) => { const k = g.fields.indexOf(sourceField); if (k >= 0) g.fields.splice(k, 1) })
  if (toGi >= 0 && state.headerGroups[toGi]) state.headerGroups[toGi].fields.push(sourceField)
}
// 字段排序：调整 headerRows 数组顺序。dir=-1 上移 / +1 下移。
// 扁平模式：纯相邻交换；分组模式：只在同一分组（含「未分组」）内找相邻同组行交换，
// 避免上移越过别组行导致组内位置不变。顺序靠 header_mapping（preserve_order）持久化。
function moveHeaderRow(i, dir) {
  const rows = state.headerRows; if (i < 0 || i >= rows.length) return
  const grouped = state.groupBy === 'group' && state.headerGroups.length
  let j = -1
  if (!grouped) {
    j = dir < 0 ? i - 1 : i + 1
  } else {
    const gi = groupIndexOf(rows[i].sourceField)
    if (dir < 0) { for (let k = i - 1; k >= 0; k--) { if (groupIndexOf(rows[k].sourceField) === gi) { j = k; break } } }
    else { for (let k = i + 1; k < rows.length; k++) { if (groupIndexOf(rows[k].sourceField) === gi) { j = k; break } } }
  }
  if (j < 0 || j >= rows.length) return
  const tmp = rows[i]; rows[i] = rows[j]; rows[j] = tmp
}
// 渲染单行（4 列：源 | 目标 | 分组 | 操作）。分组列按归组状态显示标签/移出/加入组下拉。
function headerRowHtml(r, i) {
  const gi = groupIndexOf(r.sourceField)
  let grpCell
  if (!r.sourceField) {
    grpCell = '<span class="muted" style="font-size:11px">填源字段后可归组</span>'
  } else if (gi >= 0) {
    grpCell = `<span class="hg-tag">${esc(state.headerGroups[gi].groupName)}</span> <span class="add-btn" data-mvout="${i}" title="移到未分组">移出</span>`
  } else if (state.headerGroups.length) {
    grpCell = `<ui5-select class="hm-grp" data-i="${i}"><ui5-option value="">加入组…</ui5-option>${state.headerGroups.map((g, x) => `<ui5-option value="${x}">${esc(g.groupName)}</ui5-option>`).join('')}</ui5-select>`
  } else {
    grpCell = '<span class="muted" style="font-size:11px">未分组</span>'
  }
  return `<tr data-i="${i}">
    <td><ui5-select class="hm-src" data-i="${i}">${optHtml(crOptions(), r.sourceField)}</ui5-select></td>
    <td><ui5-select class="hm-tgt" data-i="${i}">${optHtml(cmOptions(), r.targetField)}</ui5-select></td>
    <td style="white-space:nowrap">${grpCell}</td>
    <td style="white-space:nowrap"><span class="row-move" data-up="${i}" title="上移">↑</span><span class="row-move" data-down="${i}" title="下移">↓</span><span class="del-btn" data-hdel="${i}" title="删除">✕</span></td></tr>`
}
function renderHeaderTable() {
  const wrap = q('amHeaderTable'); if (!wrap) return
  const rows = state.headerRows
  if (!rows.length) {
    const empty = !state.cmFields.length
    wrap.innerHTML = `<div class="muted" style="padding:8px">${empty ? '目标字典为空，先选目标字典加载字段' : '暂无头映射，点击「增行」添加'}</div>`; return
  }
  // 扁平模式（或无分组定义）：单表格，每行带分组操作列
  if (state.groupBy === 'flat' || !state.headerGroups.length) {
    wrap.innerHTML = `<table class="tbl"><thead><tr>
        <th style="width:36%">源字段（CR 侧）</th>
        <th style="width:34%">目标列（${esc(state.current?.target_table || 'cm_*')}）</th>
        <th style="width:20%">分组</th>
        <th style="width:80px"></th></tr></thead><tbody>
      ${rows.map((r, i) => headerRowHtml(r, i)).join('')}</tbody></table>`
    bindHeaderEvents(wrap); return
  }
  // 分组模式：各分组折叠卡片 + 未分组区
  const groupsHtml = state.headerGroups.map((g, gi) => {
    const grpRows = rows.map((r, i) => ({ r, i })).filter(({ r }) => groupIndexOf(r.sourceField) === gi)
    return `<div class="hg expanded" data-gi="${gi}">
      <div class="hg-head">
        <div class="hg-title">
          <span class="chev">▸</span>
          <span class="hg-tag">分组</span>
          <input class="hg-name-input" data-gi="${gi}" value="${esc(g.groupName)}">
          <span class="hg-count">${grpRows.length} 字段</span>
        </div>
        <div class="hg-actions"><span class="del-btn" data-gdel="${gi}" title="删除整组（字段回到未分组）">✕</span></div>
      </div>
      <div class="hg-body">${grpRows.length
        ? `<table class="tbl"><tbody>${grpRows.map(({ r, i }) => headerRowHtml(r, i)).join('')}</tbody></table>`
        : '<div class="muted" style="padding:6px;font-size:11px">空组：把未分组字段「加入组」即可</div>'}</div>
    </div>`
  }).join('')
  const unassigned = rows.map((r, i) => ({ r, i })).filter(({ r }) => groupIndexOf(r.sourceField) < 0)
  const ungrpHtml = unassigned.length ? `<div class="hg expanded" data-gi="-1">
    <div class="hg-head"><div class="hg-title">
      <span class="chev">▸</span>
      <span class="hg-tag" style="background:color-mix(in srgb,var(--sapContent_LabelColor) 16%,transparent);color:var(--sapContent_LabelColor)">未分组</span>
      <span class="hg-count">${unassigned.length} 字段</span>
    </div></div>
    <div class="hg-body"><table class="tbl"><tbody>${unassigned.map(({ r, i }) => headerRowHtml(r, i)).join('')}</tbody></table></div>
  </div>` : ''
  wrap.innerHTML = groupsHtml + ungrpHtml
  bindHeaderEvents(wrap)
  wrap.querySelectorAll('.hg-head').forEach((h) => h.addEventListener('click', (e) => {
    if (e.target.closest('[data-gdel]') || e.target.closest('.hg-name-input')) return
    h.parentElement.classList.toggle('expanded')
  }))
  wrap.querySelectorAll('.hg-name-input').forEach((inp) => {
    inp.addEventListener('click', (e) => e.stopPropagation())
    inp.addEventListener('change', () => { const gi = +inp.dataset.gi; if (state.headerGroups[gi]) { state.headerGroups[gi].groupName = inp.value.trim() || state.headerGroups[gi].groupName; renderHeaderTable() } })
  })
  wrap.querySelectorAll('[data-gdel]').forEach((el) => el.addEventListener('click', () => { state.headerGroups.splice(+el.dataset.gdel, 1); renderHeaderTable() }))
}
// 行级事件绑定（源/目标/归组/删行/移出）——扁平与分组模式共用
function bindHeaderEvents(wrap) {
  wrap.querySelectorAll('ui5-select.hm-src').forEach((s) => s.addEventListener('change', () => {
    const i = +s.dataset.i; const old = state.headerRows[i].sourceField; const nxt = s.value
    state.headerRows[i].sourceField = nxt
    // 源字段变更：同步它在 headerGroups.fields 里的引用，保持归组
    if (old && old !== nxt) { const gi = groupIndexOf(old); if (gi >= 0) { state.headerGroups[gi].fields = state.headerGroups[gi].fields.filter((f) => f !== old); if (nxt) state.headerGroups[gi].fields.push(nxt) } }
    renderHeaderTable()
  }))
  wrap.querySelectorAll('ui5-select.hm-tgt').forEach((s) => s.addEventListener('change', () => { state.headerRows[+s.dataset.i].targetField = s.value }))
  wrap.querySelectorAll('ui5-select.hm-grp').forEach((s) => s.addEventListener('change', () => { if (s.value !== '') { moveRowGroup(state.headerRows[+s.dataset.i].sourceField, +s.value); renderHeaderTable() } }))
  wrap.querySelectorAll('[data-hdel]').forEach((el) => el.addEventListener('click', () => {
    const i = +el.dataset.hdel; const sf = state.headerRows[i].sourceField
    state.headerRows.splice(i, 1)
    if (sf) state.headerGroups.forEach((g) => { const k = g.fields.indexOf(sf); if (k >= 0) g.fields.splice(k, 1) })
    renderHeaderTable()
  }))
  wrap.querySelectorAll('[data-mvout]').forEach((el) => el.addEventListener('click', () => { moveRowGroup(state.headerRows[+el.dataset.mvout].sourceField, -1); renderHeaderTable() }))
  wrap.querySelectorAll('[data-up]').forEach((el) => el.addEventListener('click', () => { moveHeaderRow(+el.dataset.up, -1); renderHeaderTable() }))
  wrap.querySelectorAll('[data-down]').forEach((el) => el.addEventListener('click', () => { moveHeaderRow(+el.dataset.down, 1); renderHeaderTable() }))
}

// ── 行表映射（折叠组 + 结构化 fields 子表）────────────────────────────────────
function syncLineRowsFromMapping() {
  const lines = state.current?.line_mappings || []
  lineRowsCache.length = 0
  lines.forEach((lm) => { lineRowsCache.push({ rows: Object.entries(lm.fields || {}).map(([sourceField, targetField]) => ({ sourceField, targetField })) }) })
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
        <span class="del-btn" data-lmdel="${i}" title="删除整组">✕</span>
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
      <td style="white-space:nowrap"><span class="row-move" data-lfup="${idx}" data-ri="${ri}" title="上移">↑</span><span class="row-move" data-lfdown="${idx}" data-ri="${ri}" title="下移">↓</span><span class="del-btn" data-lfdel="${idx}" data-ri="${ri}" title="删除">✕</span></td></tr>`).join('')
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
  c.activation_code = val('amCode'); c.source_doc_type = val('amSdt'); c.cr_type = val('amCrt')
  // target_dict 来自字典帮助选择（combo），target_table 由其自动带出 → 两者均已同步进 state.current
  const combo = q('amTdCombo')
  if (combo && typeof combo.getValue === 'function') c.target_dict = combo.getValue() || ''
  c.code_rule_code = val('amCrc') || null
  c.subject_name_field = val('amSnf') || null; c.subject_code_field = val('amScf') || null
  c.header_mapping = headerRowsToMapping()
  c.header_groups = state.headerGroups.map((g) => ({ groupCode: g.groupCode, groupName: g.groupName, fields: g.fields.filter(Boolean) }))
  // 行表：把缓存行还原为 fields 扁平对象
  c.line_mappings = (c.line_mappings || []).map((lm, i) => ({
    lineType: lm.lineType || lm.line_type || '',
    targetDict: lm.targetDict || lm.target_dict || '',
    targetTable: lm.targetTable || lm.target_table || '',
    parentIdField: lm.parentIdField || lm.parent_field || lm.parentField || '',
    fields: lineRowsToFields(i),
  }))
  return c
}
async function save() {
  const M = cmx()
  try {
    const cfg = collectForm()
    if (!cfg.activation_code || !cfg.source_doc_type || !cfg.target_dict) { M.cmxWarn?.('映射码 / 来源单据类型 / 目标字典 不能为空'); return }
    await apiPost('/api/mdm/activations', cfg, coord && coord.dbId)
    showToast('保存成功', 'ok'); await loadList(); refresh()
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
function newMapping() {
  state.current = { activation_code: '', source_doc_type: '', cr_type: 'create', target_dict: '', target_table: '', is_active: true, header_mapping: {}, line_mappings: [], code_rule_code: null, subject_name_field: null, subject_code_field: null, header_groups: [] }
  state.cmFields = []
  syncHeaderRowsFromMapping(); syncHeaderGroups(); syncLineRowsFromMapping(); refresh()
}
function selectByCode(code) {
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
  // 头映射
  root.querySelector('#amAddRow')?.addEventListener('click', () => { state.headerRows.push({ sourceField: '', targetField: '' }); renderHeaderTable() })
  // 添加分组：建一个默认名「新分组」，用户随后点组名输入框改名（免 prompt）。切到分组模式以便看到。
  root.querySelector('#amAddGroup')?.addEventListener('click', () => {
    const n = state.headerGroups.length + 1
    state.headerGroups.push({ groupCode: 'group_' + Date.now(), groupName: `分组${n}`, fields: [] })
    state.groupBy = 'group'; renderHeaderTable()
  })
  // 切换 分组/扁平 展示
  root.querySelector('#amGroupBy')?.addEventListener('change', (e) => { state.groupBy = e.target.value || 'group'; renderHeaderTable() })
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
