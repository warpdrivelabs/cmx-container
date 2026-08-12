/**
 * MDM 通用变更申请表单（native-page · 并列标签页）。
 *
 * 双层元数据驱动，零业务字段硬编码：
 *   1) activation 配置（按 source_doc_type + cr_type 定位）→ 给出目标字典、头字段映射
 *      （header_mapping：{源字段:目标列}）、头分组（header_groups）、明细映射（line_mappings）、
 *      主体名标识（subject_name_field）。
 *   2) 目标字典 dct/meta → 列模型经组件库标准管线 metaTableFieldsToColumns 派生
 *      （edit.mode / refDict→cmx-dict-select / enumValues→select / required / 系统列只读）。
 *   header_mapping 的 key 即 CR 录入字段名，value（目标列）去 dct/meta 取展示属性——一份配置两处复用。
 *
 * 调用约定（列表台 openTab 经 workspace.context 传入）：
 *   { docType:'gys', crType:'create' | 'update', target?<已有字典记录> }
 *
 * 保存走平台标准单据链路：C.saveDocData → POST /doc/save（坐标 basic/dataplatform/mdm），
 * doc_no 由 cmx-code 按 codeRule 铸号，前端不传 doc_no。
 * 头表骨架（doc_status/line_no/doc_type_id/doc_date/entity_id）前端显式占位；
 * JSONB 列（payload/field_deltas/line_payload）传对象（不序列化）。
 * 流转 submit 调 MDM 专属 /mdm/change-requests/submit。
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

// 轻量 toast（保存成功等轻反馈，3s 自动消失，对齐 activation-mapper / registry-center 范式）。
let _toastTimer = null
function showToast(message, tone = 'ok', duration = 3000) {
  let el = document.getElementById('cmx-native-toast')
  if (!el) {
    el = document.createElement('div'); el.id = 'cmx-native-toast'
    el.style.cssText = 'position:fixed;top:24px;left:50%;transform:translateX(-50%);z-index:99999;display:flex;align-items:center;gap:8px;padding:10px 18px;border-radius:8px;font:500 14px/1.4 var(--sapFontFamily,Arial,sans-serif);box-shadow:0 4px 16px rgba(0,0,0,.16);pointer-events:none;opacity:0;transition:opacity .18s ease'
    document.body.appendChild(el)
    const icon = document.createElement('span'); icon.style.cssText = 'display:inline-flex;width:16px;height:16px;flex-shrink:0'
    const text = document.createElement('span'); el.appendChild(icon); el.appendChild(text)
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

// 头表分组渲染样式（前端配置，不存后端）：card=卡片分区 / bar=色条+下分隔线。改此常量切换。
const HEAD_GROUP_STYLE = 'card'
// step：create 模式初始 1（先查重），update 模式初始 2（改已有记录，跳过查重）
const state = {
  dbId: '', coord: null,
  docType: '', crType: 'create', mode: 'create', target: null,
  activation: null,        // 命中的 activation 配置
  dictMeta: null,          // 头字典 dct/meta
  headMap: [],             // [[srcField, tgtCol]] 按 header_mapping 顺序（数据构造用）
  headCols: [],            // CmxColumn[]（id=srcField，标准派生）—— 渲染用
  nameFieldKey: '',        // 提升到 subject_name 的录入字段 key
  nameCaption: '',         // 主体名字段 caption（查重表单标题/校验提示）
  headerGroups: [],
  lineDefs: [],            // [{lineType, targetDict, targetTable, parentIdField, meta, map:[[src,tgt]], cols:[CmxColumn]}]
  step: 1, keyName: '', savedCrId: null,
  crId: null, crHead: null, crLines: [],
  loading: true, loadErr: '',
}
let rootEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)

// 字典坐标四元组（domain/application/module/dbId），来自 ctx.props / workspace.context；module 回退 mdm。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  return {
    domain: get('domain') || p.domain || '',
    application: get('application') || p.application || '',
    module: get('module') || p.module || 'mdm',
    dbId: p.dbId || p.db_id || get('dbId') || get('db_id') || '',
  }
}
function coordQs(extra = {}) {
  const c = state.coord || {}
  return new URLSearchParams({
    domain: c.domain || '', application: c.application || '', module: c.module || 'mdm', ...extra,
  }).toString()
}

function styleCss() {
  return `
  .pg { height:100%; display:flex; flex-direction:column; gap:6px; box-sizing:border-box; padding:8px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor); overflow:auto;
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .sec { border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0); border-radius:6px; overflow:hidden;
    background:var(--sapList_Background,#fff); }
  .sec-hd { display:flex; align-items:center; justify-content:space-between; gap:8px;
    padding:6px 10px; border-bottom:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0);
    background:var(--sapGroup_TitleBackground,transparent); }
  .sec-hd-l { display:flex; align-items:center; gap:6px; }
  .sec-hd-r { display:flex; gap:4px; align-items:center; }
  .sec-hd ui5-icon { color:var(--sapInformativeTextColor,var(--sapHighlightColor)); font-size:1rem; }
  .sec-t { margin:0; font-weight:700; color:var(--sapTitleColor); font-size:0.95rem; }
  .sec-bd { padding:8px 10px; box-sizing:border-box; }
  .sec-head { flex:0 0 auto; }
  .sec-grid { flex:1 1 0; display:flex; flex-direction:column; min-height:120px; }
  .sec-grid .sec-bd { flex:1; min-height:0; padding:0; display:flex; flex-direction:column; }
  .tbl-wrap { flex:1; min-height:0; display:flex; flex-direction:column; }
  .tbl-wrap cmx-revo-grid { display:flex; width:100%; flex:1 1 0%; min-width:0; min-height:0; flex-direction:column; }
  cmx-toolbar { display:block; }
  /* 头表单分组：card（卡片分区）/ bar（色条+下分隔线），由 group.groupType 控制 */
  .grp { margin-bottom:6px; }
  .grp-title { display:flex; align-items:center; gap:6px; font-weight:700; color:var(--sapTitleColor); font-size:0.92rem; }
  .grp-title ui5-icon { color:var(--sapInformativeTextColor,var(--sapHighlightColor)); font-size:0.95rem; }
  .grp-body { box-sizing:border-box; }
  .grp-card { border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0); border-radius:6px; overflow:hidden;
    background:var(--sapList_Background,#fff); }
  .grp-card .grp-title { padding:6px 10px; background:var(--sapGroup_TitleBackground,#f7f7f7);
    border-bottom:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0); }
  .grp-card .grp-body { padding:8px 10px; }
  .grp-bar .grp-title { padding:2px 0 2px 10px; border-left:3px solid var(--sapButton_Emphasized_Background,#0a6ed1); }
  .grp-bar .grp-body { padding:8px 0 6px; border-bottom:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0); }
  .step-bar { display:flex; align-items:center; gap:6px; padding:6px 10px;
    background:var(--sapList_Background,#fff); border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0);
    border-radius:6px; margin-bottom:6px; font-size:0.85rem; color:var(--sapContent_LabelColor); }
  .step-bar .step { display:flex; align-items:center; gap:4px; }
  .step-bar .step .num { display:inline-flex; align-items:center; justify-content:center;
    width:20px; height:20px; border-radius:50%; font-size:0.75rem; font-weight:700;
    background:var(--sapButton_Emphasized_Background,#0a6ed1); color:#fff; }
  .step-bar .step.done .num { background:var(--sapSuccessBorderColor,#2b7c2b); }
  .step-bar .step.cur .num { background:var(--sapButton_Emphasized_Background,#0a6ed1); }
  .step-bar .step.pending .num { background:var(--sapNeutralBorderColor,#899191); }
  .step-bar .sep { color:var(--sapContent_DisabledTextColor); }
  .step-actions { display:flex; gap:6px; align-items:center; }
  .line-tabs { display:flex; gap:2px; flex-wrap:wrap; }
  .line-tab { padding:4px 12px; font-size:0.82rem; cursor:pointer; border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0);
    border-bottom:none; border-radius:6px 6px 0 0; background:var(--sapGroup_TitleBackground,transparent);
    color:var(--sapContent_LabelColor); }
  .line-tab.active { background:var(--sapList_Background,#fff); color:var(--sapTitleColor); font-weight:600;
    border-bottom:1px solid var(--sapList_Background,#fff); position:relative; top:1px; }
  .loading { padding:40px; text-align:center; color:var(--sapContent_LabelColor); font-size:0.9rem; }
  .load-err { padding:24px; color:var(--sapNegativeTextColor,#b00); font-size:0.9rem; }
  `
}
function esc(s) { return String(s ?? '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])) }

function viewHtml() {
  if (state.loading) return `<div class="pg"><div class="loading">正在加载表单元数据…</div></div>`
  if (state.loadErr) return `<div class="pg"><div class="load-err">⚠ ${esc(state.loadErr)}</div></div>`
  const mode = state.mode
  const isView = mode === 'view'
  const isEdit = mode === 'update'
  const showSteps = mode === 'create' // 仅新增显示步骤条（查重 → 完整信息）
  const step = state.step
  const stepBarHtml = showSteps ? `<div class="step-bar">
      <div class="step ${step >= 1 ? (step > 1 ? 'done' : 'cur') : 'pending'}"><span class="num">1</span><span>关键信息</span></div>
      <span class="sep">→</span>
      <div class="step ${step >= 2 ? 'cur' : 'pending'}"><span class="num">2</span><span>完整信息</span></div>
    </div>` : ''
  const domainLabel = state.dictMeta?.dictName || state.activation?.target_dict || '主数据'
  const modeLabel = isView ? '查看' : (isEdit ? '变更' : '新增')
  // view 模式标题带单据号
  const titleSuffix = (isView && state.crHead?.doc_no) ? `· ${esc(state.crHead.doc_no)}` : ''
  let topActions = ''
  if (showSteps && step === 1) {
    topActions = `<ui5-button design="Emphasized" icon="navigation-right-arrow" id="fNext">下一步</ui5-button>`
  } else if (showSteps && step === 2) {
    topActions = `<ui5-button design="Transparent" icon="navigation-left-arrow" id="fPrev">上一步</ui5-button>
      <ui5-button design="Default" icon="save" id="fSave2">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit2">保存并提交</ui5-button>`
  } else if (isEdit) {
    topActions = `<ui5-button design="Default" icon="save" id="fSave">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit">保存并提交</ui5-button>`
  }
  const keyFormCard = (showSteps && step === 1) ? `<div class="sec sec-head">
      <div class="sec-hd"><div class="sec-hd-l">
        <ui5-icon name="add-document" design="Default" mode="Decorative"></ui5-icon>
        <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">关键信息（查重）</ui5-title>
      </div></div>
      <div class="sec-bd" id="fKeyForm"></div>
    </div>` : ''
  const fullVisible = !showSteps || step === 2
  const headHtml = fullVisible ? `<div id="fHeadForms"></div>` : ''
  // 明细区：view 模式无增删行按钮
  const lineToolbar = isView ? '' : `<div class="sec-hd-r">
          <ui5-button design="Default" icon="add" id="fAddRow">增行</ui5-button>
          <ui5-button design="Transparent" icon="delete" id="fDelRow">删选中</ui5-button>
        </div>`
  const lineHtml = (fullVisible && state.lineDefs.length) ? `<div class="sec sec-grid" style="flex:1 1 auto;">
      <div class="sec-hd">
        <div class="sec-hd-l">
          <ui5-icon name="accounting-document-verification" design="Default" mode="Decorative"></ui5-icon>
          <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">明细</ui5-title>
        </div>
        ${lineToolbar}
      </div>
      <div class="sec-bd">
        <div id="fLineTabs" class="line-tabs"></div>
        <div id="fLinePanels" style="flex:1;min-height:0;display:flex;flex-direction:column;"></div>
      </div>
    </div>` : ''
  // 操作按钮统一放顶部 ui5-bar（各模式集中），底部不再放按钮
  const bottomActions = ''
  return `<div class="pg">
    <ui5-bar design="Header" accessible-role="Toolbar">
      <ui5-label wrapping-type="Normal" style="font-weight:800;font-size:1.05rem;color:var(--sapShellTitleColor,var(--sapTitleColor));">${modeLabel}${esc(domainLabel)} ${titleSuffix}</ui5-label>
      <div slot="endContent" style="display:flex;gap:4px;">${topActions}</div>
    </ui5-bar>
    ${stepBarHtml}
    ${keyFormCard}
    ${headHtml}
    ${lineHtml}
    ${bottomActions}
  </div>`
}

// ── 元数据加载 ──────────────────────────────────────────────────────────────
async function loadActivation() {
  const list = (await apiGet('/api/mdm/activations', state.dbId)) || []
  const exact = list.find((a) => a.source_doc_type === state.docType && a.cr_type === state.crType)
  if (exact) return exact
  // update 渲染回退：若无 update 配置，复用同 docType 的 create 配置（头/明细字段映射一致，
  // 仅激活器搬运走不同分支）。注意：激活器激活 update CR 仍需单独配 cr_type=update 的配置。
  if (state.crType === 'update') {
    return list.find((a) => a.source_doc_type === state.docType && a.cr_type === 'create') || null
  }
  return null
}

const _dictMetaCache = {}
async function loadDictMeta(dictCode) {
  if (!dictCode) return null
  if (_dictMetaCache[dictCode]) return _dictMetaCache[dictCode]
  const m = await apiGet(`/api/dct/meta?${coordQs({ dict: dictCode })}&with_props=true`, state.dbId)
  const data = (m && m.columns) ? m : null
  _dictMetaCache[dictCode] = data
  return data
}

// 字典全量列 → CmxColumn[]（委托组件库标准管线 metaTableFieldsToColumns，含 refDict→dict-select /
// enumValues→select / required / 系统列只读 / editSettings.coord 等完整派生）。
function metaColumns(dictMeta) {
  const C = cmx()
  if (!C.metaTableFieldsToColumns || !dictMeta) return []
  const c = state.coord || {}
  return C.metaTableFieldsToColumns(dictMeta.columns || [], {
    kind: 'DCT',
    pk: dictMeta.pk, codeField: dictMeta.codeField, selfHierarchy: dictMeta.selfHierarchy,
    parentField: dictMeta.parentField, dictCode: dictMeta.dictCode, labelField: dictMeta.labelField,
    domain: c.domain, application: c.application, module: c.module,
  }, {
    respectOrder: true,
    coord: { domain: c.domain, application: c.application, module: c.module, ...(c.dbId ? { dbId: c.dbId } : {}) },
  })
}

// 按 mapping（{srcField: tgtCol}）从全量列里筛 + 把 id 从 tgtCol 改成 srcField，保持 mapping 顺序。
// 直接改实例 id（不 spread）——spread 会丢 CmxColumn 原型链，setMembers 要求 CmxColumn 实例。
function pickAndRename(allCols, mapping) {
  const out = []
  for (const srcField of Object.keys(mapping || {})) {
    const tgtCol = mapping[srcField]
    const found = allCols.find((col) => col.id === tgtCol)
    if (found) { found.id = srcField; out.push(found) }
  }
  return out
}

// 解析 activation + 字典元数据 → headMap/headCols/lineDefs
async function buildFieldModel() {
  const C = cmx()
  const a = state.activation
  if (!a) { state.loadErr = `未找到激活映射配置（source_doc_type=${state.docType}, cr_type=${state.crType}）。请在「激活映射配置器」配置后重试。`; return }
  if (typeof C.metaTableFieldsToColumns !== 'function') { state.loadErr = '组件库版本过低（缺少 metaTableFieldsToColumns），请构建最新 cmx-data-comp。'; return }
  state.dictMeta = await loadDictMeta(a.target_dict)
  if (!state.dictMeta) { state.loadErr = `目标字典元数据加载失败：${a.target_dict}`; return }
  // 头表：全量列派生 → 按 header_mapping 筛 + 改名
  const headAll = metaColumns(state.dictMeta)
  state.headMap = Object.keys(a.header_mapping || {}).map((src) => [src, a.header_mapping[src]])
  state.headCols = pickAndRename(headAll, a.header_mapping || {})
  // 主体名 key：header_mapping 里 value === subject_name_field（目标列名）的那个 key
  const subjField = a.subject_name_field || state.dictMeta.labelField || ''
  state.nameFieldKey = ''
  for (const [src, tgt] of state.headMap) {
    if (tgt === subjField) { state.nameFieldKey = src; break }
  }
  if (!state.nameFieldKey && state.dictMeta.labelField) {
    // 兜底：取目标字典 labelField 对应列
    const fall = state.headMap.find(([src, tgt]) => tgt === state.dictMeta.labelField)
    if (fall) state.nameFieldKey = fall[0]
  }
  const nameCol = state.headCols.find((col) => col.id === state.nameFieldKey)
  state.nameCaption = nameCol ? (nameCol.caption || subjField) : (subjField || '名称')
  state.headerGroups = a.header_groups || []
  // 明细
  state.lineDefs = []
  for (const lmRaw of (a.line_mappings || [])) {
    const lm = normLineMapping(lmRaw)
    const meta = await loadDictMeta(lm.targetDict)
    const all = meta ? metaColumns(meta) : []
    const map = Object.keys(lm.fields || {}).map((src) => [src, lm.fields[src]])
    const cols = pickAndRename(all, lm.fields || {})
    state.lineDefs.push({
      lineType: lm.lineType, targetDict: lm.targetDict,
      targetTable: lm.targetTable, parentIdField: lm.parentIdField,
      meta, map, cols,
    })
  }
}

function normLineMapping(lm) {
  return {
    lineType: lm.lineType || lm.line_type || '',
    targetDict: lm.targetDict || lm.target_dict || '',
    targetTable: lm.targetTable || lm.target_table || '',
    parentIdField: lm.parentIdField || lm.parent_field || lm.parentId_field || '',
    fields: lm.fields || {},
  }
}

// ── 表单构建 ────────────────────────────────────────────────────────────────
let keyForm = null
const headForms = [] // 分组多卡片，每卡片一个 cmx-ui5-form
const lineGrids = [] // 每明细 tab 一个 cmx-revo-grid
let activeLineIdx = 0
let lineSeq = 0

function buildKeyForm() {
  const C = cmx(); const wrap = q('fKeyForm'); if (!wrap || !state.nameFieldKey) return
  wrap.innerHTML = ''
  const form = document.createElement('cmx-ui5-form'); form.classList.add('cmx-form-neo')
  if (C.CmxColumnModel) {
    const cm = new C.CmxColumnModel({ datasetId: 'crKey' })
    const col = state.headCols.find((c) => c.id === state.nameFieldKey)
    cm.setMembers(col ? [col] : [])
    form.setColumnModel(cm)
  }
  form.setLayout?.('S1 M1 L2 XL2')
  form.setDataSet?.({ [state.nameFieldKey]: state.keyName || '' })
  wrap.appendChild(form); keyForm = form
}

// 头表单：单个 cmx-ui5-form，分组用 CmxColumnGroup（列模型语义）+ form.setGroupStyle(HEAD_GROUP_STYLE)。
// cmx-ui5-form 内部按 groupStyle 渲染：每分组独立 ui5-form，标题用 ui5-form::part(header)（CSS 可控 card/bar）。
function buildHeadForms() {
  const C = cmx(); const wrap = q('fHeadForms'); if (!wrap) return
  wrap.innerHTML = ''; headForms.length = 0
  const isEdit = state.mode === 'update'
  const isView = state.mode === 'view'
  // 列只读处理（直接改实例 edit，保持 CmxColumn 类型）：view 全只读；create 步骤2 nameFieldKey 只读回显
  const cols = state.headCols.map((c) => {
    if (isView) c.edit = { ...(c.edit || {}), mode: 'readonly' }
    else if (!isEdit && c.id === state.nameFieldKey) c.edit = { ...(c.edit || {}), mode: 'readonly' }
    return c
  })
  // 按 header_groups 包成 CmxColumnGroup；未归组字段：有分组配置时包「其他」组，无分组配置时散列
  const used = new Set()
  const members = []
  for (const g of state.headerGroups) {
    const items = cols.filter((c) => (g.fields || []).includes(c.id) && !used.has(c.id))
    items.forEach((c) => used.add(c.id))
    if (!items.length) continue
    if (C.CmxColumnGroup) members.push(new C.CmxColumnGroup({ caption: g.groupName || g.groupCode || '分组', members: items }))
    else members.push(...items)
  }
  const ungrouped = cols.filter((c) => !used.has(c.id))
  if (ungrouped.length) {
    if (C.CmxColumnGroup && state.headerGroups.length) members.push(new C.CmxColumnGroup({ caption: '其他', members: ungrouped }))
    else members.push(...ungrouped)
  }
  const form = document.createElement('cmx-ui5-form'); form.classList.add('cmx-form-neo')
  if (C.CmxColumnModel) {
    const cm = new C.CmxColumnModel({ datasetId: 'crHead' })
    cm.setMembers(members)
    form.setColumnModel(cm)
  }
  form.setGroupStyle?.(HEAD_GROUP_STYLE) // card/bar 由前端常量控制
  form.setLayout?.('S1 M2 L3 XL3')
  const ds = {}
  for (const c of cols) ds[c.id] = headInitialValue(c.id)
  form.setDataSet?.(ds)
  wrap.appendChild(form)
  headForms.push(form)
}

// 头字段初始值：
//   view：从 CR 头回填（subject_name 顶层 + payload[srcField] 下沉）
//   update：从 target 字典记录回填（按 tgtCol 取，兼容扁平/payload）
//   create 步骤2：name 从步骤1 缓存的 keyName 回显
function headInitialValue(srcField) {
  const mode = state.mode
  const entry = state.headMap.find(([s]) => s === srcField)
  const tgtCol = entry ? entry[1] : srcField
  if (mode === 'view') {
    const cr = state.crHead || {}
    if (srcField === state.nameFieldKey) return cr.subject_name != null ? String(cr.subject_name) : ''
    const p = cr.payload || {}
    return p[srcField] != null ? String(p[srcField]) : ''
  }
  if (mode === 'update') {
    const t = state.target || {}
    const v = t[tgtCol] != null ? t[tgtCol] : (t.payload && t.payload[tgtCol]) != null ? t.payload[tgtCol] : ''
    return v != null ? String(v) : ''
  }
  if (srcField === state.nameFieldKey) return state.keyName || ''
  return ''
}

// 明细多 tab 渲染
function buildLineGrids() {
  const tabsHost = q('fLineTabs'); const panelsHost = q('fLinePanels')
  if (!tabsHost || !panelsHost || !state.lineDefs.length) return
  tabsHost.innerHTML = ''; panelsHost.innerHTML = ''; lineGrids.length = 0
  const C = cmx()
  const multi = state.lineDefs.length > 1
  state.lineDefs.forEach((lm, idx) => {
    if (multi) {
      const tab = document.createElement('span')
      tab.className = 'line-tab' + (idx === activeLineIdx ? ' active' : '')
      tab.textContent = lm.meta?.dictName || lm.lineType || `明细${idx + 1}`
      tab.dataset.idx = String(idx)
      tab.addEventListener('click', () => { activeLineIdx = idx; refreshLineTabsActive(); showLinePanel(idx) })
      tabsHost.appendChild(tab)
    }
    const panel = document.createElement('div')
    panel.className = 'tbl-wrap'; panel.dataset.idx = String(idx)
    panel.style.display = idx === activeLineIdx ? 'flex' : 'none'
    const grid = document.createElement('cmx-revo-grid')
    grid.setAttribute('data-cmx-fill-height', '')
    grid.setAttribute('data-cmx-options', '{"editable":true,"showTotals":false,"showRequiredMark":false}')
    grid.classList.add('cmx-grid-neo')
    panel.appendChild(grid); panelsHost.appendChild(panel)
    if (C.CmxColumnModel && lm.cols.length) {
      const cm = new C.CmxColumnModel({ datasetId: 'crLine_' + idx })
      cm.setMembers(lm.cols)
      grid.setColumnModel(cm)
    }
    const readonlyGrid = state.mode === 'view'
    grid.setOptions?.({ editable: !readonlyGrid, fillHeight: true, showRowIndex: true, selectionMode: readonlyGrid ? 'none' : 'multi', showTotals: false })
    const fill = () => {
      const rows = lineSeedRows(lm)
      if (!rows.length) { grid.refreshLayout?.(); return }
      if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows(rows); grid.setDataSet(ds) }
      else grid.setDataSet?.(rows)
      grid.refreshLayout?.()
    }
    requestAnimationFrame(() => requestAnimationFrame(fill))
    lineGrids.push(grid)
  })
}
function refreshLineTabsActive() {
  const tabsHost = q('fLineTabs'); if (!tabsHost) return
  tabsHost.querySelectorAll('.line-tab').forEach((t) => { t.classList.toggle('active', +t.dataset.idx === activeLineIdx) })
}
function showLinePanel(idx) {
  const panelsHost = q('fLinePanels'); if (!panelsHost) return
  panelsHost.querySelectorAll('.tbl-wrap').forEach((p) => { p.style.display = +p.dataset.idx === idx ? 'flex' : 'none' })
}
// 新行结构：按 lineDef 的 srcField 生成空值
function newLineRow(lm) {
  lineSeq += 1
  const r = { id: `nl_${Date.now()}_${lineSeq}` }
  for (const [src] of lm.map) r[src] = ''
  return r
}
// 明细 grid 初始行：view 模式从 CR.lines 按 line_type 预填（line_payload → srcField）；
// 其余模式给一行空行待录。
function lineSeedRows(lm) {
  if (state.mode === 'view') {
    const crLines = (state.crLines || []).filter((l) => l.line_type === lm.lineType)
    if (!crLines.length) return []
    return crLines.map((l) => {
      lineSeq += 1
      const row = { id: l.id || `cr_${Date.now()}_${lineSeq}`, _savedId: l.id }
      const p = (l.line_payload && typeof l.line_payload === 'object') ? l.line_payload : {}
      for (const [src] of lm.map) row[src] = p[src] != null ? String(p[src]) : ''
      return row
    })
  }
  return [newLineRow(lm)]
}

// ── 查重 ────────────────────────────────────────────────────────────────────
async function checkKey(name) {
  const a = state.activation
  const subjField = a.subject_name_field || state.dictMeta?.labelField || 'name'
  return apiPost('/api/mdm/check-key', {
    dictCode: a.target_dict, targetTable: a.target_table,
    keyValue: { [subjField]: name },
    specs: [{ field: subjField, weight: 100, kind: 'EditDistance' }],
    clusterKeys: [subjField],
  }, state.dbId)
}
function goStep(n) { state.step = n; refresh() }
async function onNext() {
  const C = cmx()
  const row = (keyForm && keyForm.getData && keyForm.getData()) || {}
  const name = (row[state.nameFieldKey] || '').trim()
  if (!name) { C.cmxWarn?.(`${state.nameCaption}不能为空`); return }
  try {
    const d = await checkKey(name)
    if (d && d.exists) {
      C.cmxError?.(d.message || `已存在相似记录（id=${d.id ?? ''}${d.code ? '，code=' + d.code : ''}），请确认是否继续`)
      return
    }
    state.keyName = name; goStep(2)
  } catch (e) {
    C.cmxError?.(`查重失败：${e.message}`)
  }
}

// ── 收集 / 保存 ─────────────────────────────────────────────────────────────
const DOC_DEF = { domain: 'basic', application: 'dataplatform', module: 'mdm', file: 'dataplatform_doc_meta_v1.json' }
const TABLE_NAMES = ['cv_mdm_apply', 'cv_mdm_apply_line']
const HEAD_TID = 't1'
function todayStr() { const d = new Date(); const z = (n) => String(n).padStart(2, '0'); return `${d.getFullYear()}-${z(d.getMonth() + 1)}-${z(d.getDate())}` }

// 合并所有头表单 getData
function collectHeadData() {
  const merged = {}
  for (const form of headForms) {
    const row = (form && form.getData && form.getData()) || {}
    Object.assign(merged, row)
  }
  return merged
}

// 构造头表 fields。nameFieldKey 值 → subject_name；其余 header_mapping key → payload。
function buildHead() {
  const data = collectHeadData()
  const isEdit = state.mode === 'update'
  const a = state.activation
  const name = (data[state.nameFieldKey] != null ? String(data[state.nameFieldKey]) : '').trim()
  const payload = {}
  for (const [src] of state.headMap) {
    if (src === state.nameFieldKey) continue
    payload[src] = data[src] != null ? data[src] : ''
  }
  const base = { line_no: 1, doc_status: 'draft', doc_type_id: 1, doc_date: todayStr(), entity_id: 1 }
  if (isEdit) {
    const t = state.target || {}
    const deltas = {}
    for (const [src, tgt] of state.headMap) {
      if (src === state.nameFieldKey) continue
      const oldV = (t[tgt] != null ? t[tgt] : (t.payload && t.payload[tgt]) != null ? t.payload[tgt] : '')
      const cur = (data[src] != null ? data[src] : '')
      if (String(cur) !== String(oldV)) deltas[src] = { old: oldV, new: cur }
    }
    const oldName = (t[a.subject_name_field] != null ? t[a.subject_name_field] : (t.name != null ? t.name : ''))
    if (name !== String(oldName).trim()) deltas['subject_name'] = { old: oldName, new: name }
    return { ...base, doc_type: state.docType, cr_type: state.crType, target_dict_code: a.target_dict,
      target_record_id: Number(t.id), subject_name: name, payload, field_deltas: deltas }
  }
  return { ...base, doc_type: state.docType, cr_type: state.crType, target_dict_code: a.target_dict,
    subject_name: name, payload }
}

// 收拢未提交的行内编辑（仿 data-editor）：用户在明细单元格输入后直接点保存时，
// 编辑值仍停留在 editor 组件、未写回行数据。dispatch change + blur 触发 revo-grid flush，等两帧后保存。
function commitGridEdits(cb) {
  try {
    const deepActive = (r) => { const a = r && r.activeElement; if (a && a.shadowRoot && a.shadowRoot.activeElement) return deepActive(a.shadowRoot); return a }
    const ae = deepActive(document)
    if (ae && ae !== document.body) {
      try { ae.dispatchEvent(new Event('change', { bubbles: true })) } catch (_) {}
      if (typeof ae.blur === 'function') { try { ae.blur() } catch (_) {} }
    }
  } catch (_) {}
  requestAnimationFrame(() => requestAnimationFrame(() => { try { cb() } catch (e) { console.error('[cr-form] commitGridEdits cb fail', e) } }))
}
// 取明细 grid 行：优先 getSource（含最新编辑），回退 DataSet.toPlainRows/getRows。
function lineRows(grid) {
  if (grid && typeof grid.getSource === 'function') { const s = grid.getSource(); if (Array.isArray(s)) return s }
  const ds = grid && grid.getDataSet ? grid.getDataSet() : null
  return ds ? (ds.toPlainRows ? ds.toPlainRows() : (ds.getRows ? ds.getRows() : [])) : []
}

// 收集所有明细 tab 行为 changeset
function collectLines() {
  const inserted = []; const updated = []
  state.lineDefs.forEach((lm, idx) => {
    const grid = lineGrids[idx]; if (!grid) return
    const rows = lineRows(grid)
    rows.forEach((r, i) => {
      const hasVal = lm.map.some(([src]) => r[src] != null && String(r[src]).trim() !== '')
      if (!hasVal) return
      const payload = {}
      for (const [src] of lm.map) payload[src] = r[src] != null ? r[src] : ''
      const upperId = state.savedCrId != null ? state.savedCrId : HEAD_TID
      if (r._savedId != null) {
        updated.push({ id: r._savedId, fields: { line_no: i + 1, line_payload: payload } })
      } else {
        inserted.push({ id: r.id, upper_id: upperId, line_no: i + 1, fields: {
          line_type: lm.lineType, line_action: 'insert', line_payload: payload,
        } })
      }
    })
  })
  return { inserted, updated }
}

function doSave(submit) {
  const C = cmx()
  const data0 = collectHeadData()
  const nameVal = (data0[state.nameFieldKey] != null ? String(data0[state.nameFieldKey]) : '').trim()
  if (!nameVal) { C.cmxWarn?.(`${state.nameCaption}不能为空`); return }
  if (typeof C.saveDocData !== 'function') { C.cmxError?.('组件库未加载，无法保存'); return }
  // 先收拢未提交的明细行内编辑（失焦/派发 change 触发 revo-grid flush），再构造 changeset 保存
  commitGridEdits(async () => {
    const changes = {}
    if (state.savedCrId != null) {
      changes.cv_mdm_apply = { updated: [{ id: state.savedCrId, fields: buildHead() }] }
    } else {
      changes.cv_mdm_apply = { inserted: [{ id: HEAD_TID, fields: buildHead() }] }
    }
    const { inserted: lineIns, updated: lineUpd } = collectLines()
    const lineChanges = {}
    if (lineIns.length) lineChanges.inserted = lineIns
    if (lineUpd.length) lineChanges.updated = lineUpd
    if (lineIns.length || lineUpd.length) changes[TABLE_NAMES[1]] = lineChanges
    try {
      const data = await C.saveDocData(null,
        { ...DOC_DEF, dbId: state.dbId },
        { saveMode: 'merge', changes, tableNames: TABLE_NAMES })
      const idMap = (data && data.idMap) || {}
      const isFirstSave = state.savedCrId == null
      if (isFirstSave && idMap[HEAD_TID] != null) state.savedCrId = idMap[HEAD_TID]
      if (lineIns.length) syncSavedLineIds(idMap)
      const crId = state.savedCrId
      if (submit && crId != null) {
        await apiPost('/api/mdm/change-requests/submit', { crId }, state.dbId)
      }
      showToast(submit ? `变更申请 ${crId} 已提交审批` : (isFirstSave ? `已创建变更申请 ${crId}（草稿）` : `变更申请 ${crId} 已更新`))
    } catch (e) {
      if (e && e.violations && typeof C.formatViolations === 'function') {
        C.cmxError?.(`数据校验未通过：\n${C.formatViolations(e.violations, TABLE_NAMES)}`)
      } else {
        C.cmxError?.(`保存失败：${e.message}`)
      }
    }
  })
}

function syncSavedLineIds(idMap) {
  if (!idMap) return
  state.lineDefs.forEach((_, idx) => {
    const grid = lineGrids[idx]; if (!grid) return
    const ds = grid.getDataSet?.(); if (!ds || !ds.rows) return
    ds.rows.forEach((r) => {
      if (r._savedId == null && r.id != null && idMap[r.id] != null) r._savedId = idMap[r.id]
    })
  })
}

// ── 渲染编排 ────────────────────────────────────────────────────────────────
function bind(root) {
  rootEl = root
  if (state.loading || state.loadErr) return
  const showSteps = state.mode === 'create'
  if (showSteps && state.step === 1) {
    try { buildKeyForm() } catch (e) { console.error('[cr-form] buildKeyForm fail', e) }
  } else {
    try { buildHeadForms() } catch (e) { console.error('[cr-form] buildHeadForms fail', e) }
    if (state.lineDefs.length) {
      try { buildLineGrids() } catch (e) { console.error('[cr-form] buildLineGrids fail', e) }
      if (state.mode !== 'view') bindLineToolbar()
    }
  }
  root.querySelector('#fNext')?.addEventListener('click', onNext)
  root.querySelector('#fPrev')?.addEventListener('click', () => goStep(1))
  root.querySelector('#fSave')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit')?.addEventListener('click', () => doSave(true))
  root.querySelector('#fSave2')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit2')?.addEventListener('click', () => doSave(true))
}

function bindLineToolbar() {
  rootEl.querySelector('#fAddRow')?.addEventListener('click', () => {
    const lm = state.lineDefs[activeLineIdx]; const grid = lineGrids[activeLineIdx]; if (!lm || !grid) return
    const seed = newLineRow(lm)
    const ds = grid.getDataSet?.()
    if (ds?.addRow) ds.addRow(seed); else grid.addRow?.(seed)
    queueMicrotask(() => grid?.refreshLayout?.())
  })
  rootEl.querySelector('#fDelRow')?.addEventListener('click', () => {
    const grid = lineGrids[activeLineIdx]; if (!grid) return
    const ids = grid.getSelectedIds?.(); if (ids?.length) { grid.removeRows(ids); queueMicrotask(() => grid?.refreshLayout?.()) }
  })
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

// ── 入口 ────────────────────────────────────────────────────────────────────
async function init() {
  try {
    // view 模式：先加载 CR 详情，从 CR 头取 docType/crType 定位 activation 配置
    if (state.mode === 'view' && state.crId) {
      const detail = await apiGet(`/api/mdm/change-requests/detail?crId=${state.crId}`, state.dbId)
      state.crHead = (detail && detail.head) || {}
      state.crLines = (detail && detail.lines) || []
      state.docType = state.crHead.doc_type || state.docType
      state.crType = state.crHead.cr_type || state.crType
    }
    state.activation = await loadActivation()
    await buildFieldModel()
  } catch (e) {
    state.loadErr = `元数据加载失败：${e.message}`
    console.error('[cr-form] init fail', e)
  }
  state.loading = false
  refresh()
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host; currentHost = host
      const p = (ctx && ctx.props) || {}
      const wctx = host && host.workspace && host.workspace.context
      const ctxGet = (k) => { try { return wctx && wctx.get ? wctx.get(k) : undefined } catch { return undefined } }
      state.coord = readCoord(ctx)
      state.dbId = state.coord.dbId || p.dbId || p.db_id || ''
      state.crId = ctxGet('crId') || p.crId || null
      state.docType = ctxGet('docType') || p.docType || ''
      state.crType = ctxGet('crType') || p.crType || 'create'
      // mode：view（只读详情，由 cr-todo 传 crId）/ update（变更，列表台传 target）/ create（新增）
      state.mode = ctxGet('mode') || p.mode || (state.crId ? 'view' : (state.crType === 'update' ? 'update' : 'create'))
      state.target = ctxGet('target') || p.target || null
      state.step = state.mode === 'create' ? 1 : 2
      state.keyName = ''; state.savedCrId = null
      state.crHead = null; state.crLines = []
      state.loading = true; state.loadErr = ''
      activeLineIdx = 0; lineSeq = 0
      if (host) whenRendered(host, '.pg', () => { init() })
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
