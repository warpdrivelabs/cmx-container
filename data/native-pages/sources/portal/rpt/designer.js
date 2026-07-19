/**
 * 报表设计器 —— native_pages 多实例三区页面。
 *
 * props: { reportCode, reportName, version }
 * explorer：数据元素 / 多维模型。
 * content ：电子表格设计区，使用 cmx-spreadjs-sheet 组件。
 * property：报表/Sheet/区域/版本属性，单元格/元素/公式属性。
 */

const instances = new Map()

const MODEL_PLACEHOLDERS = [
  { code: 'fico_cube', name: 'FICO 财务立方', desc: '总账、组织、期间、币种的统一分析模型' },
  { code: 'consol_cube', name: '合并报表模型', desc: '合并范围、抵消、调整分录的多维模型占位' },
  { code: 'budget_cube', name: '预算执行模型', desc: '预算、实际、预测的差异分析模型占位' },
]

const CATEGORY_COLORS = ['#0a6ed1', '#00a6c8', '#10a760', '#d98200', '#7c3aed', '#c0398a', '#607d8b']
const NUMBER_FORMATS = {
  general: '',
  number: '#,##0.00',
  currency: '¥#,##0.00',
  percent: '0.00%',
  date: 'yyyy-mm-dd',
  integer: '#,##0',
}

const FORMAT_SHORT = {
  general: '常规',
  number: '数值',
  currency: '货币',
  percent: '百分比',
  date: '日期',
  integer: '整数',
}

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;')

async function apiJson (url, options = {}) {
  const res = await fetch(url, {
    ...options,
    headers: { Accept: 'application/json', ...(options.headers || {}) },
    credentials: 'same-origin',
  })
  let j = null
  try { j = await res.json() } catch {}
  if (!res.ok || (j && typeof j.code === 'number' && j.code !== 0)) {
    throw new Error((j && (j.msg || j.error)) || `HTTP ${res.status}`)
  }
  return j && typeof j === 'object' && 'data' in j ? j.data : j
}

function propsOf (ctx) {
  const p = ctx?.props || ctx?.host?.__props || {}
  return {
    reportCode: String(p.reportCode || p.code || '').trim(),
    reportName: String(p.reportName || p.name || '').trim(),
    version: String(p.version || '').trim(),
  }
}

function instanceKey (props) {
  return `${props.reportCode || 'UNKNOWN'}@@${props.version || ''}`
}

function slug (s) {
  return String(s || 'default').trim().replace(/[^A-Za-z0-9_-]+/g, '_') || 'default'
}

/** 0基列号 → 字母（0→A, 26→AA） */
function indexToCol (idx) {
  let n = Number(idx) + 1
  let s = ''
  while (n > 0) { const r = (n - 1) % 26; s = String.fromCharCode(65 + r) + s; n = Math.floor((n - 1) / 26) }
  return s || 'A'
}

/** A1 范围串 → {r1,c1,r2,c2}（0基，含端点）；接受单格 A1（视为 A1:A1）；非法返回 null */
function expandRange (range) {
  const raw = String(range || '').toUpperCase().trim()
  const single = /^([A-Z]+)(\d+)$/.exec(raw)
  const m = single ? [raw, single[1], single[2], single[1], single[2]] : /^([A-Z]+)(\d+):([A-Z]+)(\d+)$/.exec(raw)
  if (!m) return null
  const col = (letters) => { let n = 0; for (let i = 0; i < letters.length; i++) n = n * 26 + (letters.charCodeAt(i) - 64); return n - 1 }
  const r1 = Math.min(Number(m[2]), Number(m[4])) - 1
  const r2 = Math.max(Number(m[2]), Number(m[4])) - 1
  const c1 = Math.min(col(m[1]), col(m[3]))
  const c2 = Math.max(col(m[1]), col(m[3]))
  return { r1, c1, r2, c2 }
}

function designerSid (st) {
  return `${slug(st.props.reportCode)}-${slug(st.props.version || 'default')}`
}

/** 深度穿透 shadow DOM 全局找 PORTAL-CONTENT-AREA（parent-walk 失败时兜底）。 */
function deepFindContentArea (root = document) {
  const stack = [root]
  for (let guard = 0; guard < 5000 && stack.length; guard++) {
    const node = stack.pop()
    if (!node) continue
    if (node.nodeType === 1) {
      const tag = node.tagName || ''
      if (tag === 'PORTAL-CONTENT-AREA' || (node._tabs && typeof node.getActiveTabId === 'function')) return node
      if (node.shadowRoot) stack.push(node.shadowRoot)
    }
    const kids = node.children
    if (kids) for (let i = 0; i < kids.length; i++) stack.push(kids[i])
  }
  return null
}

/** 向上穿 shadow host 链找 PORTAL-CONTENT-AREA 组件（失败则全局兜底）。 */
function findContentArea (host) {
  let node = host
  for (let i = 0; i < 40 && node; i++) {
    const tag = node.tagName || ''
    if (tag === 'PORTAL-CONTENT-AREA' || (node._tabs && typeof node.getActiveTabId === 'function')) return node
    node = node.parentElement || (node.parentNode instanceof ShadowRoot ? node.parentNode.host : node.getRootNode?.()?.host) || null
  }
  return deepFindContentArea()
}

/** 本宿主所在 tab id：向上找 dataset.cmxWorkspaceId="tab:<id>"，剥前缀。 */
function ownTabId (host) {
  let node = host
  for (let i = 0; i < 40 && node; i++) {
    const wsId = node.dataset?.cmxWorkspaceId || node.getAttribute?.('data-cmx-workspace-id')
    if (wsId && String(wsId).startsWith('tab:')) return String(wsId).slice(4)
    node = node.parentElement || (node.parentNode instanceof ShadowRoot ? node.parentNode.host : node.getRootNode?.()?.host) || null
  }
  return null
}

/** 设置/清除本报表 tab 的 dirty（关闭时门户据此弹「是否保存」对话框）。 */
function markDirty (st, dirty) {
  st.__dirty = !!dirty
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) continue
    const ca = findContentArea(host)
    if (!ca || typeof ca.setTabDirty !== 'function') continue
    const tabId = ownTabId(host) || (ca.getActiveTabId ? ca.getActiveTabId() : ca._activeTab)
    if (tabId) { try { ca.setTabDirty(tabId, !!dirty) } catch (_) {} }
  }
}

/**
 * 监听门户「关闭含未保存修改的 tab → 点保存」的 portal-content-tab-save-request。
 * tabId 命中本实例 content 宿主 → 执行 saveLayout。每实例只挂一次。
 */
function setupSaveRequestListener (st) {
  if (st.__saveReqBound) return
  st.__saveReqBound = true
  document.addEventListener('portal-content-tab-save-request', (ev) => {
    const tabId = ev.detail?.tabId
    if (!tabId) return
    for (const host of Array.from(st.hosts)) {
      if (!host || !host.isConnected || host.__rptDesignerNativeView !== 'content') continue
      if (String(ownTabId(host)) !== String(tabId)) continue
      const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
      const sheet = root?.querySelector?.('[data-rd-spread]')
      if (sheet) saveLayout(sheet, st, root)
      break
    }
  })
}

function propertyElementViewId (st) {
  return `rpt-designer-${designerSid(st)}-prop-element`
}

function getState (ctx) {
  const props = propsOf(ctx)
  const key = instanceKey(props)
  if (!instances.has(key)) {
    instances.set(key, {
      props,
      hosts: new Set(),
      elementCategories: [],
      elements: [],
      elementsLoading: false,
      elementsLoaded: false,
      elementError: '',
      elementQuery: '',
      collapsedCategories: new Set(),
      selectedElementCode: '',
      propTab: 'report',
      metaTab: 'report', // 报表属性页子 tab：report | sheet | region | version
      cellTab: 'cell', // 单元格属性页子 tab：cell | element | formula
      selectedCell: 'B4',
      selectedRange: 'B4',
      activeSheet: 'Sheet1',
      // 报表主档详情（/reports/{code}?version=）
      reportDetail: null,
      detailLoading: false,
      detailError: '',
      // 版式元数据（区域 / 单元格映射）——属性页可编辑真相，saveLayout 时落库
      regions: [], // [{code,name,type,startCell,endCell,isDefault,sheetCode}]
      cellMap: {}, // cellRef -> {elementCode,valueType,dataSource,calcFormula,checkFormula,numberFormat,...}
      cellLive: {}, // 当前选中单元格的在屏快照 {addr,value,formula,type,row,col}
      regionDraft: { name: '', type: 'data', range: '' }, // 新建区域表单草稿
      // 函数目录（GET /report-design/functions，供公式向导）——懒加载一次
      functions: [],
      functionsLoaded: false,
      wizard: null, // 打开中的函数向导状态 {fn, args:[], target, field}
      sheetUi: {
        fontFamily: 'Arial',
        fontSize: '11',
        bold: false,
        italic: false,
        underline: false,
        align: 'left',
        valign: 'middle',
        format: 'general',
        fontColor: '#1d2d3e',
        fillColor: '#ffffff',
        gridlines: true,
        headers: true,
        editable: true,
      },
    })
  }
  const st = instances.get(key)
  st.props = props
  return st
}

function versionLabel (version) {
  return version || '默认版本'
}

function reportTitle (st) {
  const code = st.props.reportCode || ''
  const name = st.props.reportName || ''
  return name ? `${code}-${name}` : code || '未指定报表'
}

function normalizeElementsPayload (data) {
  const cats = Array.isArray(data?.categories) ? data.categories : []
  const elements = Array.isArray(data?.elements) ? data.elements : []
  const known = new Set(cats.map((c) => String(c.code || '')))
  const missing = elements
    .map((e) => String(e.category_code || '').trim())
    .filter((code) => code && !known.has(code))
    .filter((code, idx, arr) => arr.indexOf(code) === idx)
    .map((code) => ({ code, name: code, sort_no: 999999 }))
  return { categories: cats.concat(missing), elements }
}

function lowerText (s) {
  return String(s ?? '').trim().toLowerCase()
}

function elementInfoText (it) {
  const parts = [
    it.code,
    it.data_type && `类型:${it.data_type}`,
    it.unit && `单位:${it.unit}`,
    it.decimals != null && it.decimals !== '' ? `精度:${it.decimals}` : '',
    it.value_source && `来源:${it.value_source}`,
    it.formula_type && `公式:${it.formula_type}`,
  ].filter(Boolean)
  return parts.join(' · ')
}

function elementMatchesQuery (it, q) {
  if (!q) return true
  return [
    it.code,
    it.name,
    it.category_code,
    it.data_type,
    it.unit,
    it.value_source,
    it.formula_type,
    it.remark,
  ].some((v) => lowerText(v).includes(q))
}

function elementDragPayload (it, category) {
  return {
    type: 'report-data-element',
    code: it.code || '',
    name: it.name || '',
    categoryCode: it.category_code || category?.code || '',
    categoryName: category?.name || '',
    dataType: it.data_type || '',
    unit: it.unit || '',
    decimals: it.decimals ?? '',
    valueSource: it.value_source || '',
    formulaType: it.formula_type || '',
    calcFormula: it.calc_formula || '',
    checkFormula: it.check_formula || '',
    remark: it.remark || '',
  }
}

function categoryColor (st, code) {
  const idx = st.elementCategories.findIndex((c) => String(c.code || '') === String(code || ''))
  return CATEGORY_COLORS[(idx >= 0 ? idx : 0) % CATEGORY_COLORS.length]
}

function selectedElement (st) {
  if (!st.selectedElementCode) return null
  return st.elements.find((it) => String(it.code || '') === st.selectedElementCode) || null
}

function elementCategory (st, it) {
  const code = String(it?.category_code || '')
  return st.elementCategories.find((c) => String(c.code || '') === code) || null
}

async function loadElements (st, force = false) {
  if (st.elementsLoading) return
  if (force) { st.elementsLoaded = false }
  if (st.elementsLoaded) return
  st.elementsLoading = true
  st.elementError = ''
  refreshInstance(st, (view) => view === 'explorerData')
  try {
    const data = await apiJson('/api/report-design/elements')
    const normalized = normalizeElementsPayload(data)
    st.elementCategories = normalized.categories
    st.elements = normalized.elements
    st.elementsLoaded = true
  } catch (err) {
    st.elementError = err instanceof Error ? err.message : String(err || '数据元素加载失败')
  } finally {
    st.elementsLoading = false
    refreshInstance(st, (view) => view === 'explorerData' || view === 'propertyElement')
  }
}

/** 加载函数目录（GET /report-design/functions）：懒加载一次，供公式向导选函数/逐参渲染。 */
async function loadFunctions (st, force = false) {
  if (st.functionsLoaded && !force) return st.functions
  try {
    const data = await apiJson('/api/report-design/functions')
    st.functions = Array.isArray(data?.functions) ? data.functions : []
    st.functionsLoaded = true
  } catch (err) {
    st.functions = []
    st.functionsLoaded = true // 失败也不反复请求；向导给出提示
  }
  return st.functions
}

/** 加载报表主档详情（属性页用）：/reports/{code}?version= 。已加载则复用。 */
async function loadReportDetail (st, force = false) {
  try {
    const url = `/api/report-design/reports/${enc(st.props.reportCode)}${st.props.version ? `?version=${enc(st.props.version)}` : ''}`
    st.reportDetail = await apiJson(url)
  } catch (err) {
    st.detailError = err instanceof Error ? err.message : String(err || '报表详情加载失败')
  } finally {
    st.detailLoading = false
    refreshInstance(st, (view) => view === 'propertyMeta')
  }
}

/**
 * 跨宿主取当前实例的在屏 SpreadJS 组件。属性页与 content 页是不同 native-page 宿主，
 * 属性页里没有 sheet 元素，需从 content 宿主的 renderRoot 里捞 <cmx-spreadjs-sheet>。
 */
function liveSheetOf (st) {
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) continue
    if (host.__rptDesignerNativeView !== 'content') continue
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    const el = root?.querySelector?.('[data-rd-spread]')
    if (el) return el
  }
  return null
}

/** 从在屏 sheet 抓当前选中单元格快照（地址/值/公式/类型），供单元格属性页展示。 */
function captureLiveCell (st) {
  const sheet = liveSheetOf(st)
  const addr = st.selectedCell || 'A1'
  const snap = { addr, value: '', formula: '', type: 'empty', row: null, col: null }
  const wb = sheet?.getWorkbook?.()
  const ws = wb?.getActiveSheet?.()
  const p = parseA1(addr)
  if (ws && p) {
    snap.row = p.row; snap.col = p.col
    const formula = ws.getFormula ? ws.getFormula(p.row, p.col) : null
    const val = ws.getValue ? ws.getValue(p.row, p.col) : null
    if (formula) { snap.formula = `=${formula}`; snap.type = 'formula'; snap.value = val == null ? '' : String(val) }
    else if (val !== null && val !== undefined && val !== '') {
      snap.value = String(val)
      snap.type = typeof val === 'number' ? 'number' : 'text'
    }
  }
  st.cellLive = snap
  return snap
}

/** A1 → {row,col}（0基）；复用组件同款解析。 */
function parseA1 (addr) {
  const m = /^([A-Z]+)(\d+)$/.exec(String(addr || '').toUpperCase())
  if (!m) return null
  let col = 0
  for (let i = 0; i < m[1].length; i++) col = col * 26 + (m[1].charCodeAt(i) - 64)
  return { col: col - 1, row: Number(m[2]) - 1 }
}

function styleCss () {
  return `
    .rd{--rd-blue:#0a6ed1;--rd-cyan:#00a6c8;--rd-green:#10a760;--rd-purple:#7c3aed;--rd-amber:#d98200;--rd-border:var(--sapGroup_TitleBorderColor,#d9e2ec);
      height:100%;min-height:0;box-sizing:border-box;display:flex;flex-direction:column;overflow:hidden;background:var(--sapBackgroundColor,#f5f6f7);color:var(--sapTextColor,#1d2d3e);font:13px/1.45 var(--sapFontFamily,Arial,sans-serif)}
    .rd-head{height:46px;flex:0 0 auto;display:flex;align-items:center;gap:9px;padding:0 12px;border-bottom:1px solid var(--rd-border);background:var(--sapList_HeaderBackground,#f7f9fc)}
    .rd-head-ic{width:30px;height:30px;border-radius:8px;display:flex;align-items:center;justify-content:center;background:color-mix(in srgb,var(--rd-blue) 12%,transparent);color:var(--rd-blue)}
    .rd-head-ic ui5-icon{width:1rem;height:1rem}.rd-title{min-width:0}.rd-title b,.rd-title span{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rd-title b{font-size:14px}.rd-title span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-toolbar{margin-left:auto;display:flex;align-items:center;gap:8px;min-width:0}
    .rd-selection{height:26px;min-width:52px;max-width:150px;border-radius:6px;padding:0 10px;display:inline-flex;align-items:center;gap:5px;background:var(--sapField_Background,#fff);border:1px solid color-mix(in srgb,var(--rd-blue) 26%,var(--rd-border));color:var(--rd-blue);font:800 12px/1 ui-monospace,Menlo,Consolas,monospace;letter-spacing:.02em;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;box-shadow:inset 0 1px 2px rgba(10,31,68,.04)}.rd-selection::before{content:"";width:6px;height:6px;border-radius:50%;background:var(--rd-blue);flex:0 0 auto;box-shadow:0 0 0 3px color-mix(in srgb,var(--rd-blue) 16%,transparent)}
    .rd-hgroup{display:inline-flex;align-items:center;gap:2px;height:32px;padding:2px;border-radius:8px;background:color-mix(in srgb,var(--rd-border) 26%,transparent)}
    .rd-hbtn{position:relative;height:28px;min-width:28px;border:0;border-radius:6px;background:transparent;color:var(--sapContent_IconColor,#475059);display:inline-flex;align-items:center;justify-content:center;gap:6px;padding:0 8px;font:inherit;font-size:12px;font-weight:600;cursor:pointer;transition:background .12s,color .12s,box-shadow .12s;white-space:nowrap}
    .rd-hbtn svg{width:1.02rem;height:1.02rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round}
    .rd-hbtn:hover{background:var(--sapTile_Background,#fff);color:var(--rd-blue);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-hbtn:disabled{opacity:.36;cursor:not-allowed;background:transparent!important;color:var(--sapContent_IconColor,#475059)!important;box-shadow:none!important}
    .rd-hbtn.primary{background:linear-gradient(180deg,#1a7ee0,var(--rd-blue));color:#fff;box-shadow:0 1px 2px rgba(10,110,209,.36),inset 0 1px 0 rgba(255,255,255,.24);padding:0 12px;font-weight:700}
    .rd-hbtn.primary:hover{background:linear-gradient(180deg,#248ceb,#0a63bd);color:#fff;box-shadow:0 3px 10px rgba(10,110,209,.4),inset 0 1px 0 rgba(255,255,255,.28)}
    .rd-history{position:relative;display:inline-flex;align-items:center;height:28px;border-radius:6px;overflow:visible}.rd-history:hover:not(.disabled){background:var(--sapTile_Background,#fff);box-shadow:0 1px 4px rgba(10,31,68,.12)}.rd-history.disabled{opacity:.36}
    .rd-history-action,.rd-history-caret{height:28px;border:0;background:transparent;color:var(--sapContent_IconColor,#475059);display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0}.rd-history:hover:not(.disabled) .rd-history-action,.rd-history:hover:not(.disabled) .rd-history-caret{color:var(--rd-blue)}.rd-history-action{width:26px;border-radius:6px 0 0 6px}.rd-history-caret{width:14px;border-radius:0 6px 6px 0}.rd-history-caret:hover{background:color-mix(in srgb,var(--rd-blue) 12%,transparent)}.rd-history-action:disabled,.rd-history-caret:disabled{cursor:not-allowed}
    .rd-history-action svg{width:1.02rem;height:1.02rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round}.rd-history-caret svg{width:.56rem;height:.56rem;fill:none;stroke:currentColor;stroke-width:2.1;stroke-linecap:round;stroke-linejoin:round}
    .rd-history-menu{position:absolute;right:0;top:34px;z-index:30;display:none;width:230px;max-height:280px;overflow:auto;padding:6px;border:1px solid var(--rd-border);border-radius:9px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 36px rgba(10,31,68,.2)}.rd-history.open .rd-history-menu{display:block}.rd-history.open{background:var(--sapTile_Background,#fff);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-history-title{padding:4px 8px 6px;font-size:10.5px;font-weight:700;color:var(--sapContent_LabelColor,#6a6d70);text-transform:uppercase;letter-spacing:.04em}
    .rd-history-item{width:100%;height:30px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;font-size:12px;display:flex;align-items:center;gap:8px;padding:0 8px;text-align:left;cursor:pointer}.rd-history-item:hover,.rd-history-item.hot{background:color-mix(in srgb,var(--rd-blue) 10%,transparent);color:var(--rd-blue)}.rd-history-item i{flex:0 0 auto;width:16px;height:16px;display:inline-flex;align-items:center;justify-content:center;color:var(--sapContent_LabelColor,#6a6d70)}.rd-history-item:hover i,.rd-history-item.hot i{color:var(--rd-blue)}.rd-history-item i svg{width:.82rem;height:.82rem;fill:none;stroke:currentColor;stroke-width:1.9;stroke-linecap:round;stroke-linejoin:round}.rd-history-item span{min-width:0;flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.rd-history-item small{margin-left:auto;color:var(--sapContent_LabelColor,#8a9099);font:700 10px/1 ui-monospace,Menlo,monospace}.rd-history-item:hover small,.rd-history-item.hot small{color:var(--rd-blue)}.rd-history-empty{padding:12px 8px;color:var(--sapContent_LabelColor,#6a6d70);font-size:12px;text-align:center}.rd-btn,.rd-icon{height:30px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapButton_Background,#fff);color:var(--sapButton_TextColor,#0a6ed1);font:inherit;font-size:12px;display:inline-flex;align-items:center;justify-content:center;gap:5px;padding:0 9px;cursor:pointer}.rd-icon{width:30px;padding:0}.rd-btn.primary{background:var(--rd-blue);border-color:var(--rd-blue);color:#fff}.rd-btn ui5-icon,.rd-icon ui5-icon{width:1rem;height:1rem}
    .rd-body{flex:1;min-height:0;overflow:auto;padding:10px;box-sizing:border-box}.rd-tabs{display:flex;gap:6px;padding:8px 8px 0;border-bottom:1px solid var(--rd-border);background:color-mix(in srgb,var(--rd-blue) 5%,var(--sapList_HeaderBackground,#f7f9fc))}.rd-tab{height:34px;border:1px solid var(--rd-border);border-bottom:0;border-radius:8px 8px 0 0;background:var(--sapTile_Background,#fff);color:inherit;font-weight:700;padding:0 10px;display:flex;align-items:center;gap:6px}.rd-tab.active{color:var(--rd-blue);box-shadow:0 -2px 0 var(--rd-blue)}.rd-tab ui5-icon{width:1rem;height:1rem}
    .rd-el-panel{flex:1;min-height:0;display:flex;flex-direction:column}.rd-search{flex:0 0 auto;height:43px;box-sizing:border-box;display:flex;align-items:center;padding:0 10px;border-bottom:1px solid var(--rd-border);background:var(--sapList_HeaderBackground,#f7f9fc)}.rd-search-box{flex:1;height:32px;display:flex;align-items:center;gap:7px;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapField_Background,#fff);padding:0 8px;box-sizing:border-box}.rd-search-box ui5-icon{color:var(--rd-cyan);width:1rem;height:1rem}.rd-search-box input{flex:1;min-width:0;border:0;outline:0;background:transparent;color:inherit;font:inherit;font-size:12px}.rd-search-clear{width:22px;height:22px;border:0;border-radius:5px;background:transparent;color:var(--sapContent_LabelColor,#6a6d70);display:inline-flex;align-items:center;justify-content:center;cursor:pointer}.rd-search-clear ui5-icon{width:.8rem;height:.8rem}.rd-el-scroll{flex:1;min-height:0;overflow:auto;padding:10px;box-sizing:border-box}
    .rd-cat{--cat-color:var(--rd-cyan);border:1px solid color-mix(in srgb,var(--cat-color) 16%,var(--rd-border));border-radius:8px;background:color-mix(in srgb,var(--cat-color) 3%,var(--sapTile_Background,#fff));margin-bottom:9px;overflow:hidden}.rd-cat-h{width:100%;height:34px;border:0;border-bottom:1px solid color-mix(in srgb,var(--cat-color) 14%,var(--rd-border));display:flex;align-items:center;gap:8px;padding:0 10px;background:color-mix(in srgb,var(--cat-color) 8%,var(--sapList_HeaderBackground,#f7f9fc));color:inherit;font:inherit;font-weight:800;text-align:left;cursor:pointer}.rd-cat-h:hover{background:color-mix(in srgb,var(--cat-color) 13%,var(--sapList_HeaderBackground,#f7f9fc))}.rd-cat-h ui5-icon{color:var(--cat-color);width:1rem;height:1rem}.rd-cat-h small{margin-left:auto;min-width:24px;height:18px;border-radius:999px;display:inline-flex;align-items:center;justify-content:center;color:var(--cat-color);font-size:11px;background:color-mix(in srgb,var(--cat-color) 10%,transparent);border:1px solid color-mix(in srgb,var(--cat-color) 18%,var(--rd-border))}.rd-cat.closed .rd-cat-h{border-bottom-color:transparent}.rd-cat.closed .rd-cat-body{display:none}.rd-cat-body{display:grid;gap:6px;padding:7px}
    .rd-el{--el-color:var(--cat-color);position:relative;display:grid;grid-template-columns:minmax(0,1fr) 20px;gap:7px;align-items:center;min-height:48px;padding:7px 7px 7px 10px;border:1px solid color-mix(in srgb,var(--el-color) 18%,var(--rd-border));border-radius:7px;cursor:pointer;background:linear-gradient(90deg,color-mix(in srgb,var(--el-color) 6%,var(--sapTile_Background,#fff)),var(--sapTile_Background,#fff) 42%);box-shadow:0 1px 0 rgba(10,31,68,.04)}.rd-el:before{content:"";position:absolute;left:0;top:7px;bottom:7px;width:3px;border-radius:0 4px 4px 0;background:var(--el-color)}.rd-el:hover{border-color:color-mix(in srgb,var(--el-color) 42%,var(--rd-border));box-shadow:0 5px 14px rgba(10,31,68,.09);transform:translateY(-1px)}.rd-el:active{cursor:grabbing}.rd-el.dragging{opacity:.55;background:color-mix(in srgb,var(--el-color) 12%,var(--sapTile_Background,#fff))}.rd-el.selected{border-color:var(--el-color);background:linear-gradient(90deg,color-mix(in srgb,var(--el-color) 14%,var(--sapTile_Background,#fff)),var(--sapTile_Background,#fff) 48%);box-shadow:0 0 0 2px color-mix(in srgb,var(--el-color) 18%,transparent),0 7px 18px rgba(10,31,68,.11)}.rd-el.selected:after{content:"";position:absolute;right:6px;top:6px;width:6px;height:6px;border-radius:50%;background:var(--el-color);box-shadow:0 0 0 3px color-mix(in srgb,var(--el-color) 18%,transparent)}.rd-el-main{min-width:0}.rd-el-name{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:12px;font-weight:900;color:var(--sapTextColor,#1d2d3e);padding-right:9px}.rd-el-info{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:1px;font-size:10.5px;color:var(--sapContent_LabelColor,#6a6d70)}.rd-el-grip{width:18px;height:32px;border-radius:6px;display:flex;align-items:center;justify-content:center;color:var(--sapContent_LabelColor,#6a6d70);opacity:.75;background:color-mix(in srgb,var(--el-color) 5%,transparent);cursor:grab}.rd-el-grip ui5-icon{width:.9rem;height:.9rem}.rd-pill{border:1px solid color-mix(in srgb,var(--rd-cyan) 24%,var(--rd-border));border-radius:999px;padding:2px 7px;color:var(--rd-cyan);font-size:11px;font-weight:800;background:color-mix(in srgb,var(--rd-cyan) 6%,var(--sapTile_Background,#fff))}
    .rd-model{border:1px dashed color-mix(in srgb,var(--rd-purple) 34%,var(--rd-border));border-radius:8px;background:color-mix(in srgb,var(--rd-purple) 5%,var(--sapTile_Background,#fff));padding:12px;margin-bottom:9px}.rd-model b{display:block;color:var(--rd-purple)}.rd-model span{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-sheet-wrap{flex:1;min-height:0;display:flex;flex-direction:column;background:var(--sapBackgroundColor,#f5f6f7)}
    .rd-ribbon{flex:0 0 auto;height:43px;box-sizing:border-box;border-bottom:1px solid var(--rd-border);background:linear-gradient(180deg,#fff,var(--sapList_HeaderBackground,#f4f7fb));padding:0 10px;display:flex;align-items:center;gap:8px;overflow:visible;position:relative;box-shadow:0 1px 0 rgba(10,31,68,.03)}
    .rd-ribbon-main{flex:1;min-width:0;display:flex;align-items:center;gap:8px;overflow:hidden}
    .rd-ribbon-item{flex:0 0 auto;display:inline-flex;align-items:center}
    .rd-group{flex:0 0 auto;display:inline-flex;align-items:center;gap:1px;padding:2px;border-radius:8px;background:color-mix(in srgb,var(--rd-border) 22%,transparent)}
    .rd-ribbon-sep{width:0;height:0;margin:0}
    .rd-tool,.rd-menu-tool{min-width:28px;height:28px;border:0;border-radius:6px;background:transparent;color:var(--sapContent_IconColor,#475059);display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0;position:relative;transition:background .12s,color .12s,box-shadow .12s}
    .rd-tool:disabled{opacity:.36;cursor:not-allowed}
    .rd-tool svg,.rd-menu-tool svg{width:1.02rem;height:1.02rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round}.rd-tool ui5-icon,.rd-menu-tool ui5-icon{width:1rem;height:1rem}
    .rd-tool:hover,.rd-menu-tool:hover{background:#fff;color:var(--rd-blue);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-tool:disabled:hover{background:transparent;color:var(--sapContent_IconColor,#475059);box-shadow:none}
    .rd-tool.active{background:var(--rd-blue);color:#fff;box-shadow:0 1px 3px rgba(10,110,209,.36),inset 0 1px 0 rgba(255,255,255,.22)}
    .rd-tool.active:hover{background:#0a63bd;color:#fff}
    .rd-menu-tool{min-width:auto;gap:4px;padding:0 4px 0 7px}
    .rd-menu-tool .rd-mt-ic{display:inline-flex;align-items:center}.rd-menu-tool .rd-mt-val{font-size:12px;font-weight:600;min-width:0;max-width:120px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--sapTextColor,#1d2d3e)}
    .rd-menu-tool .rd-mt-car{flex:0 0 auto;width:.52rem;height:.52rem;opacity:.55}.rd-menu-tool .rd-mt-car svg{width:.52rem;height:.52rem;stroke-width:2.4}
    .rd-menu-tool.compact{padding:0 3px 0 6px}.rd-menu-tool.compact .rd-mt-val{width:22px;text-align:center}
    .rd-menu-tool:hover .rd-mt-val{color:var(--rd-blue)}
    .rd-menu-tool select{position:absolute;inset:0;width:100%;height:100%;opacity:0;cursor:pointer;-webkit-appearance:none;appearance:none;border:0}
    .rd-color{width:28px;height:28px;border-radius:6px;display:inline-flex;flex-direction:column;align-items:center;justify-content:center;gap:0;position:relative;color:var(--sapContent_IconColor,#475059);cursor:pointer;transition:background .12s,color .12s,box-shadow .12s}
    .rd-color:hover{background:#fff;color:var(--rd-blue);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-color svg{width:1.02rem;height:1.02rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round;margin-top:-1px}
    .rd-color .rd-color-bar{width:16px;height:3px;border-radius:2px;margin-top:1px;background:var(--rd-swatch,#1d2d3e);box-shadow:inset 0 0 0 1px rgba(0,0,0,.12)}
    .rd-color input{position:absolute;inset:0;opacity:0;cursor:pointer}
    .rd-more{flex:0 0 auto;position:relative}.rd-more[hidden]{display:none}.rd-more>.rd-tool.active,.rd-more>.rd-tool[aria-expanded="true"]{background:var(--rd-blue);color:#fff}
    .rd-more-menu{position:absolute;right:0;top:36px;z-index:20;min-width:64px;display:none;flex-direction:column;gap:6px;padding:8px;border:1px solid var(--rd-border);border-radius:9px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 36px rgba(10,31,68,.2)}.rd-more.open .rd-more-menu{display:flex}.rd-more-menu .rd-ribbon-item,.rd-more-menu .rd-group{display:inline-flex}.rd-more-menu .rd-group{background:color-mix(in srgb,var(--rd-border) 22%,transparent);flex-wrap:wrap}.rd-more-menu .rd-ribbon-sep{display:none}
    .rd-sheet-stage{flex:1;min-height:0;overflow:hidden;padding:12px;background:linear-gradient(180deg,color-mix(in srgb,var(--rd-blue) 4%,var(--sapBackgroundColor,#f5f6f7)),var(--sapBackgroundColor,#f5f6f7))}.rd-spread-host{height:100%;min-height:480px;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapTile_Background,#fff);box-shadow:0 4px 18px rgba(10,31,68,.08);overflow:hidden}.rd-spread{display:block;width:100%;height:100%;min-height:480px}
    .rd-sheet-stage.rd-drop-hot .rd-spread-host{border-color:var(--rd-blue);box-shadow:0 0 0 2px color-mix(in srgb,var(--rd-blue) 30%,transparent),0 4px 18px color-mix(in srgb,var(--rd-blue) 22%,transparent)}
    .rd-sheet-stage{position:relative}.rd-drop-hint{position:absolute;z-index:40;display:none;pointer-events:none;padding:3px 8px;border-radius:6px;background:var(--rd-blue);color:#fff;font:700 11px/1.4 var(--sapFontFamily,Arial,sans-serif);box-shadow:0 4px 12px rgba(10,110,209,.35);white-space:nowrap}
    .rd-prop-grid{display:grid;grid-template-columns:1fr;gap:8px}.rd-sec{border:1px solid var(--rd-border);border-radius:8px;background:var(--sapTile_Background,#fff);padding:10px}.rd-sec>b{display:block;margin-bottom:7px;color:var(--rd-blue)}.rd-row{display:grid;grid-template-columns:86px minmax(0,1fr);gap:8px;align-items:center;margin:6px 0}.rd-row span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}.rd-row b,.rd-row input{min-width:0}.rd-row input{height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapField_Background,#fff);color:inherit;padding:0 8px}.rd-note{border:1px dashed var(--rd-border);border-radius:8px;padding:12px;background:var(--sapList_HeaderBackground,#f7f9fc);color:var(--sapContent_LabelColor,#6a6d70)}.rd-empty{padding:18px;border:1px dashed var(--rd-border);border-radius:8px;background:var(--sapTile_Background,#fff);color:var(--sapContent_LabelColor,#6a6d70)}.rd-loading{display:flex;align-items:center;gap:8px;padding:14px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-toast{position:absolute;left:50%;bottom:22px;transform:translate(-50%,14px);z-index:60;max-width:min(560px,88%);padding:10px 16px;border-radius:9px;background:#1d2d3e;color:#fff;font-size:12.5px;font-weight:600;box-shadow:0 12px 32px rgba(10,31,68,.34);opacity:0;pointer-events:none;transition:opacity .22s,transform .22s;display:flex;align-items:center;gap:8px}.rd-toast.show{opacity:1;transform:translate(-50%,0)}.rd-toast[data-kind="success"]{background:linear-gradient(180deg,#12b56b,#0f9d5c)}.rd-toast[data-kind="error"]{background:linear-gradient(180deg,#e5544b,#c0392b)}.rd-toast::before{content:"";width:7px;height:7px;border-radius:50%;background:currentColor;opacity:.8;flex:0 0 auto}
    .rd-head-actions{margin-left:auto;display:flex;align-items:center;gap:6px}
    .rd-ibtn{width:28px;height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapButton_Background,#fff);color:var(--sapContent_IconColor,#475059);display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0;transition:background .12s,color .12s,box-shadow .12s}.rd-ibtn:hover{color:var(--rd-blue);border-color:color-mix(in srgb,var(--rd-blue) 40%,var(--rd-border));box-shadow:0 1px 4px rgba(10,31,68,.12)}.rd-ibtn ui5-icon{width:1rem;height:1rem}.rd-ibtn.spin ui5-icon{animation:rd-spin .8s linear infinite}@keyframes rd-spin{to{transform:rotate(360deg)}}
    .rd-tab{cursor:pointer}
    .rd-fields{display:grid;grid-template-columns:88px minmax(0,1fr);gap:7px 8px;align-items:center}
    .rd-fields label{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-fields input,.rd-fields select,.rd-fields textarea{min-width:0;height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapField_Background,#fff);color:inherit;padding:0 8px;font:inherit;font-size:12px;box-sizing:border-box}
    .rd-fields textarea{height:auto;min-height:52px;padding:6px 8px;resize:vertical;font-family:ui-monospace,Menlo,Consolas,monospace}
    .rd-fields input:focus,.rd-fields select:focus,.rd-fields textarea:focus{outline:0;border-color:var(--rd-blue);box-shadow:0 0 0 3px color-mix(in srgb,var(--rd-blue) 13%,transparent)}
    .rd-fields input[readonly]{background:var(--sapList_HeaderBackground,#f7f9fc);color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-fields .wide{grid-column:1 / -1}
    .rd-actions{display:flex;flex-wrap:wrap;gap:6px;margin-top:9px}
    .rd-sbtn{height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapButton_Background,#fff);color:var(--rd-blue);font:inherit;font-size:12px;font-weight:600;display:inline-flex;align-items:center;gap:5px;padding:0 10px;cursor:pointer}.rd-sbtn:hover{background:color-mix(in srgb,var(--rd-blue) 8%,#fff);border-color:color-mix(in srgb,var(--rd-blue) 40%,var(--rd-border))}.rd-sbtn.primary{background:var(--rd-blue);border-color:var(--rd-blue);color:#fff}.rd-sbtn.primary:hover{background:#0a63bd}.rd-sbtn.danger{color:var(--rd-red,#bb0000)}.rd-sbtn.danger:hover{background:color-mix(in srgb,#bb0000 8%,#fff);border-color:color-mix(in srgb,#bb0000 40%,var(--rd-border))}.rd-sbtn ui5-icon{width:.95rem;height:.95rem}.rd-sbtn:disabled{opacity:.4;cursor:not-allowed}
    .rd-chips{display:flex;flex-wrap:wrap;gap:5px;margin-top:3px}.rd-chip{display:inline-flex;align-items:center;gap:4px;border:1px solid var(--rd-border);border-radius:999px;padding:2px 8px;font-size:11px;background:var(--sapList_HeaderBackground,#f7f9fc)}.rd-chip.on{color:#fff;background:var(--rd-green);border-color:var(--rd-green)}
    .rd-list{display:flex;flex-direction:column;gap:6px;margin-top:4px}
    .rd-litem{border:1px solid var(--rd-border);border-radius:7px;background:var(--sapTile_Background,#fff);padding:7px 9px;display:grid;grid-template-columns:minmax(0,1fr) auto;gap:6px;align-items:center;cursor:pointer}.rd-litem:hover{border-color:color-mix(in srgb,var(--rd-blue) 40%,var(--rd-border))}.rd-litem.active{border-color:var(--rd-blue);background:color-mix(in srgb,var(--rd-blue) 7%,var(--sapTile_Background,#fff))}.rd-litem-main{min-width:0}.rd-litem-main b{display:block;font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rd-litem-main small{display:block;font-size:10.5px;color:var(--sapContent_LabelColor,#6a6d70);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-family:ui-monospace,Menlo,Consolas,monospace}.rd-litem-act{display:flex;gap:4px}.rd-litem-act button{width:24px;height:24px;border:0;border-radius:5px;background:transparent;color:var(--sapContent_LabelColor,#6a6d70);cursor:pointer;display:inline-flex;align-items:center;justify-content:center}.rd-litem-act button:hover{background:color-mix(in srgb,var(--rd-blue) 10%,transparent);color:var(--rd-blue)}.rd-litem-act button.danger:hover{background:color-mix(in srgb,#bb0000 10%,transparent);color:#bb0000}.rd-litem-act ui5-icon{width:.82rem;height:.82rem}
    .rd-badge{display:inline-block;font-size:9px;font-weight:800;letter-spacing:.04em;padding:1px 5px;border-radius:4px;background:color-mix(in srgb,var(--rd-cyan) 14%,transparent);color:var(--rd-cyan);vertical-align:middle;margin-left:5px}.rd-badge.default{background:color-mix(in srgb,var(--rd-green) 14%,transparent);color:var(--rd-green)}
    .rd-mini-empty{border:1px dashed var(--rd-border);border-radius:7px;padding:11px;text-align:center;color:var(--sapContent_LabelColor,#6a6d70);font-size:11.5px;background:var(--sapList_HeaderBackground,#f7f9fc)}
    .rd-live{display:flex;align-items:center;gap:7px;margin-bottom:8px;padding:7px 9px;border:1px solid color-mix(in srgb,var(--rd-blue) 22%,var(--rd-border));border-radius:7px;background:color-mix(in srgb,var(--rd-blue) 5%,var(--sapTile_Background,#fff))}.rd-live b{font:800 13px/1 ui-monospace,Menlo,Consolas,monospace;color:var(--rd-blue)}.rd-live span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-fx-mask{position:fixed;inset:0;background:rgba(20,30,45,.34);display:flex;align-items:center;justify-content:center;z-index:60}
    .rd-fx-dlg{width:min(560px,92vw);max-height:86vh;overflow:auto;background:var(--sapTile_Background,#fff);border:1px solid var(--rd-border);border-radius:12px;box-shadow:0 12px 40px rgba(0,0,0,.28)}
    .rd-fx-head{display:flex;align-items:center;justify-content:space-between;padding:12px 14px;border-bottom:1px solid var(--rd-border)}.rd-fx-head b{display:flex;align-items:center;gap:6px;color:var(--rd-blue);font-size:14px}
    .rd-fx-x{width:26px;height:26px;border:0;border-radius:6px;background:transparent;color:var(--sapContent_LabelColor,#6a6d70);cursor:pointer;display:inline-flex;align-items:center;justify-content:center}.rd-fx-x:hover{background:color-mix(in srgb,#bb0000 10%,transparent);color:#bb0000}
    .rd-fx-body{padding:14px}
    .rd-fx-group{margin-bottom:12px}.rd-fx-glabel{font-size:10.5px;font-weight:800;letter-spacing:.05em;color:var(--sapContent_LabelColor,#6a6d70);margin-bottom:5px}
    .rd-fx-pick{display:flex;flex-direction:column;gap:10px}
    .rd-fx-item{display:flex;flex-direction:column;align-items:flex-start;gap:2px;width:100%;text-align:left;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapList_HeaderBackground,#f7f9fc);padding:8px 10px;cursor:pointer;margin-bottom:5px}.rd-fx-item:hover{border-color:var(--rd-blue);background:color-mix(in srgb,var(--rd-blue) 7%,#fff)}
    .rd-fx-name{font:800 12.5px/1 ui-monospace,Menlo,Consolas,monospace;color:var(--rd-blue)}.rd-fx-help{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-fx-fn{margin-bottom:10px;font-size:12px}.rd-fx-fn b{font-family:ui-monospace,Menlo,Consolas,monospace;color:var(--rd-blue)}.rd-fx-eg{color:var(--sapContent_LabelColor,#6a6d70);font-size:11px}
    .rd-fx-grid{display:flex;flex-direction:column;gap:9px}
    .rd-fx-row{display:grid;grid-template-columns:88px minmax(0,1fr);gap:8px;align-items:center}.rd-fx-row>label{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-fx-row select,.rd-fx-row input{height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapField_Background,#fff);color:inherit;padding:0 8px;min-width:0}
    .rd-fx-row .rd-fx-abs{margin-top:4px;width:100%}
    .rd-fx-hint{grid-column:2;font-size:10px;color:var(--sapContent_LabelColor,#8a8d90)}
    .rd-fx-out{margin:12px 0 10px;display:grid;grid-template-columns:44px 1fr;gap:8px;align-items:center}.rd-fx-out label{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}.rd-fx-out code{font:700 12.5px/1.4 ui-monospace,Menlo,Consolas,monospace;color:var(--rd-green);background:color-mix(in srgb,var(--rd-green) 8%,#fff);border:1px solid color-mix(in srgb,var(--rd-green) 24%,var(--rd-border));border-radius:6px;padding:6px 8px;word-break:break-all}
    .rd-fx-actions{display:flex;justify-content:flex-end;gap:8px}
  `
}

function headHtml (st, title, icon, actions) {
  return `<div class="rd-head">
    <span class="rd-head-ic"><ui5-icon name="${esc(icon)}"></ui5-icon></span>
    <span class="rd-title"><b>${esc(title)}</b><span>${esc(reportTitle(st))} · ${esc(versionLabel(st.props.version))}</span></span>
    ${actions ? `<span class="rd-head-actions">${actions}</span>` : ''}
  </div>`
}

function explorerDataHtml (st) {
  const query = lowerText(st.elementQuery)
  let body = ''
  if (st.elementsLoading) {
    body = '<div class="rd-loading"><ui5-icon name="synchronize"></ui5-icon>正在从 fico-db 装载数据元素...</div>'
  } else if (st.elementError) {
    body = `<div class="rd-empty">数据元素加载失败：${esc(st.elementError)}</div>`
  } else if (!st.elementCategories.length) {
    body = '<div class="rd-empty">暂无数据元素类别。</div>'
  } else {
    const sections = st.elementCategories.map((c) => {
      const code = String(c.code || '')
      const color = categoryColor(st, code)
      const catText = lowerText(`${c.code || ''} ${c.name || ''} ${c.remark || ''}`)
      const catMatches = query && catText.includes(query)
      const items = st.elements
        .filter((it) => String(it.category_code || '') === code)
        .filter((it) => catMatches || elementMatchesQuery(it, query))
      if (query && !items.length && !catMatches) return ''
      const closed = st.collapsedCategories.has(code)
      const itemHtml = items.length
        ? items.map((it) => {
          const payload = elementDragPayload(it, c)
          const selected = String(it.code || '') === st.selectedElementCode
          return `<div class="rd-el ${selected ? 'selected' : ''}" draggable="true" data-element-select="${esc(it.code || '')}" data-element-drag="${esc(JSON.stringify(payload))}">
            <span class="rd-el-main">
              <b class="rd-el-name">${esc(it.name || it.code)}</b>
              <span class="rd-el-info">${esc(elementInfoText(it))}${it.remark ? ` · ${esc(it.remark)}` : ''}</span>
            </span>
            <span class="rd-el-grip"><ui5-icon name="vertical-grip"></ui5-icon></span>
          </div>`
        }).join('')
        : '<div class="rd-empty" style="margin:8px">该类别下还没有定义数据元素。</div>'
      return `<section class="rd-cat ${closed ? 'closed' : ''}" style="--cat-color:${esc(color)}">
        <button class="rd-cat-h" type="button" data-cat-toggle="${esc(code)}">
          <ui5-icon name="${closed ? 'navigation-right-arrow' : 'navigation-down-arrow'}"></ui5-icon>
          <span>${esc(c.name || c.code)}</span>
          <small>${items.length}</small>
        </button>
        <div class="rd-cat-body">${itemHtml}</div>
      </section>`
    }).filter(Boolean)
    body = sections.length ? sections.join('') : '<div class="rd-empty">没有找到匹配的数据元素。</div>'
  }
  return `<section class="rd">
    ${headHtml(st, '数据元素', 'database', `<button class="rd-ibtn ${st.elementsLoading ? 'spin' : ''}" type="button" data-el-refresh title="刷新数据元素"><ui5-icon name="refresh"></ui5-icon></button>`)}
    <div class="rd-el-panel">
      <div class="rd-search">
        <label class="rd-search-box">
          <ui5-icon name="search"></ui5-icon>
          <input data-el-search value="${esc(st.elementQuery)}" placeholder="搜索元素名称 / 编码 / 类型" autocomplete="off">
          ${st.elementQuery ? '<button class="rd-search-clear" type="button" data-el-clear title="清空"><ui5-icon name="decline"></ui5-icon></button>' : ''}
        </label>
      </div>
      <div class="rd-el-scroll">${body}</div>
    </div>
  </section>`
}

function explorerModelHtml (st) {
  const body = MODEL_PLACEHOLDERS.map((m) => `<div class="rd-model"><b>${esc(m.name)}</b><span>${esc(m.code)} · ${esc(m.desc)}</span></div>`).join('')
  return `<section class="rd">${headHtml(st, '多维模型', 'dimension')}<div class="rd-body">${body}<div class="rd-note">多维模型页面为占位页，后续接入模型选择、维度拖拽和数据集绑定。</div></div></section>`
}

function reportModel (st) {
  const title = reportTitle(st)
  const version = versionLabel(st.props.version)
  return {
    meta: {
      reportCode: st.props.reportCode || '',
      reportName: st.props.reportName || '',
      version: st.props.version || '',
    },
    sheets: [{
      id: 'sheet1',
      name: st.props.reportCode || 'Sheet1',
      grid: {
        rows: 60,
        cols: 18,
        colWidths: { A: 56, B: 140, C: 140, D: 120, E: 120 },
        rowHeights: { 1: 34, 2: 28, 3: 28 },
        merges: ['B1:E1'],
        styleClasses: {
          title: { bold: true, fontSize: 16, align: 'center', fillColor: '#eaf4ff', fontColor: '#0a6ed1' },
          label: { bold: true, fillColor: '#f3f6f9' },
        },
      },
      cells: {
        B1: { type: 'text', value: title, class: 'title' },
        B2: { type: 'text', value: '报表编码', class: 'label' },
        C2: { type: 'text', value: st.props.reportCode || '' },
        B3: { type: 'text', value: '版本', class: 'label' },
        C3: { type: 'text', value: version },
      },
    }],
  }
}

function fontSpec (ui) {
  const parts = []
  if (ui.italic) parts.push('italic')
  if (ui.bold) parts.push('bold')
  const size = Math.max(8, Math.min(72, Number(ui.fontSize) || 11))
  const family = String(ui.fontFamily || 'Arial').replace(/[;"']/g, '') || 'Arial'
  parts.push(`${size}px`)
  // ★ SpreadJS 渲染到 canvas，ctx.font 无法解析 CSS var()——含 var() 会让整串 font 失效，
  //   加粗/斜体/字号全不生效。故只输出 "样式 字号 字体族"，末尾就是字体族本身
  //   （组件 getSelectionState 用 split(/\s+/).pop() 回读字体族，末尾必须是族名不能是 sans-serif 兜底）。
  return `${parts.join(' ')} ${family}`
}

function toolIcon (name) {
  const icons = {
    'font-family': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 19 11 5h2l6 14"/><path d="M8 14h8"/></svg>',
    'font-size': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7V5h10v2"/><path d="M9 5v14"/><path d="M6 19h6"/><path d="M15 11h5"/><path d="M18 8v10"/><path d="M16 18h4"/></svg>',
    'bold-text': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5h5a4 4 0 0 1 0 8H8z"/><path d="M8 13h6a3 3 0 0 1 0 6H8z"/></svg>',
    'italic-text': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 5h8"/><path d="M6 19h8"/><path d="m14 5-4 14"/></svg>',
    'underline-text': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5v6a5 5 0 0 0 10 0V5"/><path d="M6 21h12"/></svg>',
    'text-color': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 19 11 5h2l6 14"/><path d="M8 14h8"/><path d="M6 22h12"/></svg>',
    palette: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4a8 8 0 0 0 0 16h1.5a2 2 0 0 0 1.4-3.4 1.6 1.6 0 0 1 1.1-2.6H18a4 4 0 0 0 0-8.7A9 9 0 0 0 12 4z"/><path d="M7.5 11h.1M9.5 7.5h.1M14 7.5h.1"/></svg>',
    border: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5h14v14H5z"/><path d="M12 5v14"/><path d="M5 12h14"/></svg>',
    'border-style': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6h12v12H6z"/><path d="M4 4h16v16H4z"/></svg>',
    decline: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12"/><path d="m18 6-12 12"/></svg>',
    'text-align-left': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 6h14"/><path d="M5 10h10"/><path d="M5 14h14"/><path d="M5 18h9"/></svg>',
    'text-align-center': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 6h14"/><path d="M8 10h8"/><path d="M5 14h14"/><path d="M7 18h10"/></svg>',
    'text-align-right': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 6h14"/><path d="M9 10h10"/><path d="M5 14h14"/><path d="M10 18h9"/></svg>',
    'vertical-align-top': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 5h12"/><path d="M12 19V8"/><path d="m8 12 4-4 4 4"/></svg>',
    'vertical-align-middle': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 12h12"/><path d="M12 5v14"/><path d="m8 9 4-4 4 4"/><path d="m8 15 4 4 4-4"/></svg>',
    'vertical-align-bottom': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 19h12"/><path d="M12 5v11"/><path d="m8 12 4 4 4-4"/></svg>',
    'text-wrap': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14"/><path d="M5 12h10a3 3 0 0 1 0 6h-2"/><path d="m15 15-3 3 3 3"/></svg>',
    combine: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14v10H5z"/><path d="M9 7v10"/><path d="M15 7v10"/><path d="m10 12 2-2 2 2"/><path d="m10 12 2 2 2-2"/></svg>',
    split: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14v10H5z"/><path d="M12 7v10"/><path d="m10 12-3-3"/><path d="m7 9v6"/><path d="m14 12 3-3"/><path d="m17 9v6"/></svg>',
    'number-format': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 18 9 6"/><path d="M15 18 18 6"/><path d="M5 10h14"/><path d="M4 14h14"/></svg>',
    overflow: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h.01"/><path d="M12 12h.01"/><path d="M19 12h.01"/></svg>',
    save: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5h12l2 2v12H5z"/><path d="M8 5v6h8V5"/><path d="M8 19v-5h8v5"/></svg>',
    download: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11"/><path d="m8 11 4 4 4-4"/><path d="M5 20h14"/></svg>',
    upload: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 15V4"/><path d="m8 8 4-4 4 4"/><path d="M5 20h14"/></svg>',
    undo: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7 5 11l4 4"/><path d="M5 11h10a5 5 0 0 1 0 10h-2"/></svg>',
    redo: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m15 7 4 4-4 4"/><path d="M19 11H9a5 5 0 0 0 0 10h2"/></svg>',
    eraser: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 16 9-9 4 4-7 7H9z"/><path d="M3 21h18"/></svg>',
    'clear-formatting': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 6h11"/><path d="M10 6v12"/><path d="M7 18h6"/><path d="m15 15 4 4"/><path d="m19 15-4 4"/></svg>',
    'chevron-down': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>',
    'rotate-ccw': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 5v6h6"/><path d="M3.5 11a9 9 0 1 1 .5 6"/></svg>',
    'rotate-cw': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 5v6h-6"/><path d="M20.5 11a9 9 0 1 0-.5 6"/></svg>',
  }
  return icons[name] || `<ui5-icon name="${esc(name)}"></ui5-icon>`
}

function ribbonButton (cmd, icon, title, active = false) {
  return `<button class="rd-tool rd-ribbon-item ${active ? 'active' : ''}" type="button" data-sheet-cmd="${esc(cmd)}" title="${esc(title)}" aria-label="${esc(title)}">${toolIcon(icon)}</button>`
}

function ribbonToggle (cmd, icon, title, checked) {
  return `<button class="rd-tool rd-ribbon-item ${checked ? 'active' : ''}" type="button" data-sheet-cmd="${esc(cmd)}" aria-pressed="${checked ? 'true' : 'false'}" title="${esc(title)}" aria-label="${esc(title)}">${toolIcon(icon)}</button>`
}

function historyButton (kind, icon, title) {
  return `<span class="rd-history" data-history="${esc(kind)}">
    <button class="rd-history-action" type="button" data-sheet-cmd="${esc(kind)}" title="${esc(title)}" aria-label="${esc(title)}">${toolIcon(icon)}</button>
    <button class="rd-history-caret" type="button" data-history-toggle="${esc(kind)}" title="${esc(title)}历史" aria-label="${esc(title)}历史">${toolIcon('chevron-down')}</button>
    <span class="rd-history-menu" data-history-menu="${esc(kind)}"><span class="rd-history-empty">暂无${esc(title)}记录</span></span>
  </span>`
}

/**
 * 值型下拉工具（字体 / 字号 / 数字格式）：显示当前值文本 + 下拉箭头，
 * 透明 <select> 覆盖整个控件承接原生下拉交互。compact 用于窄值（字号）。
 */
function fieldMenu (field, icon, title, options, value, opts = {}) {
  const current = options.find((it) => String(it.value) === String(value))
  const shown = opts.showLabel === false ? '' : `<span class="rd-mt-val">${esc(current ? (opts.short ? (current.short || current.label) : current.label) : (opts.placeholder || value || ''))}</span>`
  return `<label class="rd-menu-tool rd-ribbon-item ${opts.compact ? 'compact' : ''}" title="${esc(title)}" aria-label="${esc(title)}">
    ${icon ? `<span class="rd-mt-ic">${toolIcon(icon)}</span>` : ''}${shown}<span class="rd-mt-car">${toolIcon('chevron-down')}</span>
    <select data-sheet-field="${esc(field)}">${options.map((it) => `<option value="${esc(it.value)}" ${String(value) === String(it.value) ? 'selected' : ''}>${esc(it.label)}</option>`).join('')}</select>
  </label>`
}

function colorTool (field, icon, title, value, fallback) {
  return `<label class="rd-color rd-ribbon-item" title="${esc(title)}" aria-label="${esc(title)}" style="--rd-swatch:${esc(value || fallback)}">
    ${toolIcon(icon)}<span class="rd-color-bar"></span>
    <input type="color" data-sheet-field="${esc(field)}" value="${esc(value || fallback)}">
  </label>`
}

function ribbonGroup (...items) {
  return `<span class="rd-group rd-ribbon-item">${items.join('')}</span>`
}

function toolbarHtml (st) {
  const ui = st.sheetUi
  const fontOptions = ['Arial', 'Microsoft YaHei', 'SimSun', 'SimHei', 'KaiTi', 'Calibri', 'Times New Roman', 'Courier New'].map((v) => ({ value: v, label: v }))
  const sizeOptions = [9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36].map((v) => ({ value: v, label: String(v) }))
  const formatOptions = [
    { value: 'general', label: '常规', short: '常规' },
    { value: 'number', label: '数值 1,234.00', short: '数值' },
    { value: 'currency', label: '货币 ¥1,234.00', short: '货币' },
    { value: 'percent', label: '百分比 12.34%', short: '百分比' },
    { value: 'date', label: '日期 2026-07-16', short: '日期' },
    { value: 'integer', label: '整数 1,234', short: '整数' },
  ]
  return `<div class="rd-ribbon" data-rd-ribbon>
    <span class="rd-ribbon-main" data-rd-ribbon-main>
      ${ribbonGroup(
        fieldMenu('fontFamily', '', '字体', fontOptions, ui.fontFamily, { placeholder: '字体' }),
        fieldMenu('fontSize', '', '字号', sizeOptions, ui.fontSize, { compact: true, placeholder: '11' }),
      )}
      ${ribbonGroup(
        ribbonToggle('bold', 'bold-text', '加粗', ui.bold),
        ribbonToggle('italic', 'italic-text', '斜体', ui.italic),
        ribbonToggle('underline', 'underline-text', '下划线', ui.underline),
        colorTool('fontColor', 'text-color', '字体颜色', ui.fontColor, '#1d2d3e'),
        colorTool('fillColor', 'palette', '填充颜色', ui.fillColor, '#ffffff'),
      )}
      ${ribbonGroup(
        ribbonButton('align-left', 'text-align-left', '左对齐', ui.align === 'left'),
        ribbonButton('align-center', 'text-align-center', '居中对齐', ui.align === 'center'),
        ribbonButton('align-right', 'text-align-right', '右对齐', ui.align === 'right'),
        ribbonButton('valign-top', 'vertical-align-top', '顶端对齐', ui.valign === 'top'),
        ribbonButton('valign-middle', 'vertical-align-middle', '垂直居中', ui.valign === 'middle'),
        ribbonButton('valign-bottom', 'vertical-align-bottom', '底端对齐', ui.valign === 'bottom'),
        ribbonToggle('wrap', 'text-wrap', '自动换行', ui.wordWrap),
      )}
      ${ribbonGroup(
        ribbonButton('border-all', 'border', '所有框线'),
        ribbonButton('border-outline', 'border-style', '外侧框线'),
        ribbonButton('border-none', 'decline', '无框线'),
      )}
      ${ribbonGroup(
        ribbonButton('merge', 'combine', '合并居中'),
        ribbonButton('unmerge', 'split', '取消合并'),
      )}
      ${ribbonGroup(
        fieldMenu('format', 'number-format', '数字格式', formatOptions, ui.format, { short: true, placeholder: '常规' }),
      )}
    </span>
    <span class="rd-more" data-rd-more hidden>
      <button class="rd-tool" type="button" data-rd-more-toggle title="更多工具" aria-label="更多工具" aria-expanded="false">${toolIcon('overflow')}</button>
      <span class="rd-more-menu" data-rd-more-menu></span>
    </span>
  </div>`
}

function headerToolsHtml (st) {
  return `<span class="rd-toolbar">
    <span class="rd-selection" data-rd-selection title="当前选区">${esc(st.selectedRange || st.selectedCell || 'A1')}</span>
    <span class="rd-hgroup">
      ${historyButton('undo', 'undo', '撤销')}
      ${historyButton('redo', 'redo', '重做')}
    </span>
    <span class="rd-hgroup">
      <button class="rd-hbtn" type="button" data-sheet-cmd="clear-value" title="清除内容" aria-label="清除内容">${toolIcon('eraser')}</button>
      <button class="rd-hbtn" type="button" data-sheet-cmd="clear-format" title="清除格式" aria-label="清除格式">${toolIcon('clear-formatting')}</button>
    </span>
    <span class="rd-hgroup">
      <button class="rd-hbtn" type="button" data-sheet-cmd="import-xlsx" title="导入 Excel" aria-label="导入 Excel">${toolIcon('upload')}<span>导入</span></button>
      <button class="rd-hbtn" type="button" data-sheet-cmd="export-xlsx" title="导出 Excel" aria-label="导出 Excel">${toolIcon('download')}<span>导出</span></button>
      <input data-rd-import-file type="file" accept=".xlsx,.xls" hidden>
    </span>
    <button class="rd-hbtn primary" type="button" data-sheet-cmd="save" title="保存报表设计" aria-label="保存">${toolIcon('save')}<span>保存</span></button>
  </span>`
}

function contentHtml (st) {
  return `<section class="rd rd-sheet-wrap">
    <div class="rd-head">
      <span class="rd-head-ic"><ui5-icon name="table-chart"></ui5-icon></span>
      <span class="rd-title"><b>${esc(reportTitle(st))}</b><span>${esc(versionLabel(st.props.version))} · SpreadJS 电子表格</span></span>
      ${headerToolsHtml(st)}
    </div>
    ${toolbarHtml(st)}
    <div class="rd-sheet-stage"><div class="rd-spread-host"><cmx-spreadjs-sheet class="rd-spread" data-rd-spread data-cmx-report="${esc(JSON.stringify(reportModel(st)))}"></cmx-spreadjs-sheet></div></div>
  </section>`
}

const META_TABS = [
  { key: 'report', label: '报表', icon: 'document-text' },
  { key: 'sheet', label: 'Sheet', icon: 'table-view' },
  { key: 'region', label: '区域', icon: 'border' },
  { key: 'version', label: '版本', icon: 'add-document' },
]

function propertyMetaHtml (st) {
  const tab = st.metaTab || 'report'
  const tabs = META_TABS.map((t) => `<button class="rd-tab ${tab === t.key ? 'active' : ''}" type="button" data-meta-tab="${t.key}"><ui5-icon name="${t.icon}"></ui5-icon>${esc(t.label)}</button>`).join('')
  let body = ''
  if (tab === 'report') body = metaReportBody(st)
  else if (tab === 'sheet') body = metaSheetBody(st)
  else if (tab === 'region') body = metaRegionBody(st)
  else if (tab === 'version') body = metaVersionBody(st)
  const refreshAct = `<button class="rd-ibtn ${st.detailLoading ? 'spin' : ''}" type="button" data-meta-refresh title="刷新报表详情"><ui5-icon name="refresh"></ui5-icon></button>`
  return `<section class="rd">${headHtml(st, '报表属性', 'detail-view', refreshAct)}
    <div class="rd-tabs">${tabs}</div>
    <div class="rd-body">${st.detailError ? `<div class="rd-empty">加载失败：${esc(st.detailError)}</div>` : ''}${body}</div></section>`
}

/** 报表 tab：主档信息（来自 /reports/{code}），只读展示。 */
function metaReportBody (st) {
  const r = st.reportDetail?.report || {}
  if (st.detailLoading && !st.reportDetail) return '<div class="rd-loading"><ui5-icon name="synchronize"></ui5-icon>正在加载报表主档...</div>'
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>基础信息</b>
      ${roRow('报表编码', r.code || st.props.reportCode)}
      ${roRow('报表名称', r.name || st.props.reportName)}
      ${roRow('报表类型', r.report_type)}
      ${roRow('报表类别', r.report_category)}
      ${roRow('状态', Number(r.status) === 0 ? '停用' : '启用')}
    </section>
    <section class="rd-sec"><b>口径与取数</b>
      ${roRow('期间类型', r.period_type)}
      ${roRow('编制口径', r.entity_scope)}
      ${roRow('币种 / 单位', `${r.currency_code || '-'} / ${r.amount_unit || '-'}`)}
      ${roRow('取数来源', r.data_source || '未指定')}
      ${roRow('是否法定', Number(r.is_statutory) === 1 ? '是' : '否')}
    </section>
    <section class="rd-sec"><b>说明</b>${roRow('备注', r.remark || '暂无备注')}${roRow('更新时间', r.update_time || '-')}</section>
  </div>`
}

/** Sheet tab：当前 sheet 显示设置——联动在屏画布（网格线/表头/可编辑/行列数）。 */
function metaSheetBody (st) {
  const ui = st.sheetUi
  const snap = liveSheetDims(st)
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>当前 Sheet</b>
      <div class="rd-fields">
        <label>名称</label><input readonly value="${esc(st.activeSheet)}">
        <label>行数</label><input readonly value="${esc(snap.rows)}">
        <label>列数</label><input readonly value="${esc(snap.cols)}">
      </div>
    </section>
    <section class="rd-sec"><b>显示设置</b>
      <div class="rd-chips">
        <button class="rd-chip ${ui.gridlines ? 'on' : ''}" type="button" data-sheet-cmd="toggle-gridlines"><ui5-icon name="grid"></ui5-icon>网格线</button>
        <button class="rd-chip ${ui.headers ? 'on' : ''}" type="button" data-sheet-cmd="toggle-headers"><ui5-icon name="table-column"></ui5-icon>行列头</button>
        <button class="rd-chip ${ui.editable ? 'on' : ''}" type="button" data-sheet-cmd="toggle-editable"><ui5-icon name="edit"></ui5-icon>可编辑</button>
      </div>
    </section>
    <section class="rd-sec"><b>结构操作</b>
      <div class="rd-actions">
        <button class="rd-sbtn" type="button" data-sheet-cmd="insert-row"><ui5-icon name="add"></ui5-icon>插入行</button>
        <button class="rd-sbtn danger" type="button" data-sheet-cmd="delete-row"><ui5-icon name="less"></ui5-icon>删除行</button>
        <button class="rd-sbtn" type="button" data-sheet-cmd="insert-col"><ui5-icon name="add"></ui5-icon>插入列</button>
        <button class="rd-sbtn danger" type="button" data-sheet-cmd="delete-col"><ui5-icon name="less"></ui5-icon>删除列</button>
      </div>
    </section>
  </div>`
}

/** 区域 tab：区域列表 + 新建/删除；区域数据存 st.regions，saveLayout 时随投影落库。 */
function metaRegionBody (st) {
  const sheetCode = currentSheetCode(st)
  const regions = (st.regions || []).filter((r) => !r.sheetCode || r.sheetCode === sheetCode)
  const draft = st.regionDraft || { name: '', type: 'data', range: '' }
  const listHtml = regions.length
    ? regions.map((r) => `<div class="rd-litem" data-region-code="${esc(r.code)}">
        <span class="rd-litem-main"><b>${esc(r.name || r.code)}${r.isDefault ? '<span class="rd-badge default">默认</span>' : ''}</b><small>${esc(r.code)}${r.startCell ? ` · ${esc(r.startCell)}:${esc(r.endCell || r.startCell)}` : ''} · ${esc(r.type || 'data')}</small></span>
        ${r.isDefault ? '' : `<span class="rd-litem-act"><button type="button" class="danger" data-region-del="${esc(r.code)}" title="删除区域"><ui5-icon name="delete"></ui5-icon></button></span>`}
      </div>`).join('')
    : '<div class="rd-mini-empty">尚无显式区域。未定义区域时，本 Sheet 有数据的单元格归入默认区域 __default__。</div>'
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>区域列表 · ${esc(sheetCode)}</b>
      <div class="rd-list">${listHtml}</div>
    </section>
    <section class="rd-sec"><b>新建区域</b>
      <div class="rd-fields">
        <label>名称</label><input data-region-field="name" value="${esc(draft.name)}" placeholder="如：资产类">
        <label>类型</label><select data-region-field="type">
          ${['data', 'header', 'title', 'summary', 'repeat'].map((t) => `<option value="${t}" ${draft.type === t ? 'selected' : ''}>${t}</option>`).join('')}
        </select>
        <label>范围</label><input data-region-field="range" value="${esc(draft.range)}" placeholder="A1:E10（留空=选区）">
      </div>
      <div class="rd-actions">
        <button class="rd-sbtn" type="button" data-region-use-selection><ui5-icon name="pending"></ui5-icon>取当前选区</button>
        <button class="rd-sbtn primary" type="button" data-region-add><ui5-icon name="add"></ui5-icon>添加区域</button>
      </div>
    </section>
  </div>`
}

/** 版本 tab：版本序列（来自 /reports 详情），标注当前生效 + 设计资产统计。 */
function metaVersionBody (st) {
  const detail = st.reportDetail || {}
  const versions = Array.isArray(detail.versions) ? detail.versions : []
  const stats = detail.stats || {}
  const cur = st.props.version || detail.selectedVersion || ''
  const listHtml = versions.length
    ? versions.map((v) => `<div class="rd-litem ${v.code === cur ? 'active' : ''}">
        <span class="rd-litem-main"><b>${esc(v.name || v.code)}${Number(v.is_current) === 1 ? '<span class="rd-badge default">当前</span>' : ''}</b><small>${esc(v.code)} · ${esc(v.version_status || 'draft')}${v.change_summary ? ` · ${esc(v.change_summary)}` : ''}</small></span>
      </div>`).join('')
    : '<div class="rd-mini-empty">默认版本（未创建显式版本）。</div>'
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>版本序列</b><div class="rd-list">${listHtml}</div></section>
    <section class="rd-sec"><b>设计资产（当前版本）</b>
      <div class="rd-fields">
        <label>Sheet</label><input readonly value="${Number(stats.sheet_count || 0)}">
        <label>区域</label><input readonly value="${Number(stats.region_count || 0)}">
        <label>行 / 列</label><input readonly value="${Number(stats.row_count || 0)} / ${Number(stats.col_count || 0)}">
        <label>格式</label><input readonly value="${Number(stats.format_count || 0)}">
      </div>
    </section>
  </div>`
}

/** 在屏 sheet 的行列数（无组件时给模型默认）。 */
function liveSheetDims (st) {
  const sheet = liveSheetOf(st)
  const ws = sheet?.getWorkbook?.()?.getActiveSheet?.()
  return {
    rows: ws?.getRowCount ? ws.getRowCount() : 60,
    cols: ws?.getColumnCount ? ws.getColumnCount() : 18,
  }
}

/** 把 st.activeSheet 同步为在屏 SpreadJS 的真实活动 sheet 名（区域/行列归属都以它为准）。 */
function syncActiveSheetName (sheet, st) {
  const ws = (sheet || liveSheetOf(st))?.getWorkbook?.()?.getActiveSheet?.()
  const name = ws?.name ? ws.name() : ''
  if (name) st.activeSheet = name
}

/** 当前 sheet 编码：优先在屏 SpreadJS 活动 sheet 真名，回退 st.activeSheet（区域归属基准）。 */
function currentSheetCode (st) {
  const ws = liveSheetOf(st)?.getWorkbook?.()?.getActiveSheet?.()
  return (ws?.name ? ws.name() : '') || st.activeSheet || 'Sheet1'
}

const CELL_TABS = [
  { key: 'cell', label: '单元格', icon: 'grid' },
  { key: 'element', label: '元素', icon: 'database' },
  { key: 'formula', label: '公式', icon: 'fx' },
]

function propertyCellHtml (st) {
  const tab = st.cellTab || 'cell'
  const snap = captureLiveCell(st)
  const tabs = CELL_TABS.map((t) => `<button class="rd-tab ${tab === t.key ? 'active' : ''}" type="button" data-cell-tab="${t.key}"><ui5-icon name="${t.icon}"></ui5-icon>${esc(t.label)}</button>`).join('')
  const live = `<div class="rd-live"><b>${esc(snap.addr)}</b><span>${cellTypeLabel(snap.type)}${snap.formula ? ` · ${esc(snap.formula)}` : (snap.value ? ` · ${esc(snap.value)}` : ' · 空')}</span></div>`
  let body = ''
  if (tab === 'cell') body = cellBasicBody(st, snap)
  else if (tab === 'element') body = cellElementBody(st, snap)
  else if (tab === 'formula') body = cellFormulaBody(st, snap)
  return `<section class="rd">${headHtml(st, '单元格属性', 'target-group')}
    <div class="rd-tabs">${tabs}</div>
    <div class="rd-body">${live}${body}</div></section>`
}

function cellTypeLabel (t) {
  return ({ empty: '空单元格', text: '文本', number: '数值', formula: '公式' })[t] || t
}

/** 单元格 tab：地址/类型/值 + 直接写值/清除。 */
function cellBasicBody (st, snap) {
  const cm = st.cellMap[snap.addr] || {}
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>基本</b>
      <div class="rd-fields">
        <label>地址</label><input readonly value="${esc(snap.addr)}">
        <label>类型</label><input readonly value="${esc(cellTypeLabel(snap.type))}">
        <label>当前值</label><input data-cell-input="value" value="${esc(snap.formula || snap.value)}" placeholder="输入值或 =公式">
      </div>
      <div class="rd-actions">
        <button class="rd-sbtn primary" type="button" data-cell-apply="value"><ui5-icon name="accept"></ui5-icon>写入单元格</button>
        <button class="rd-sbtn" type="button" data-sheet-cmd="clear-value"><ui5-icon name="eraser"></ui5-icon>清除内容</button>
      </div>
    </section>
    <section class="rd-sec"><b>数据元素绑定</b>
      ${cm.elementCode
        ? `<div class="rd-chips"><span class="rd-chip on"><ui5-icon name="database"></ui5-icon>${esc(cm.elementCode)}</span></div>`
        : '<div class="rd-mini-empty">未绑定数据元素。切到「元素」页可绑定。</div>'}
    </section>
  </div>`
}

/** 元素 tab：把选中数据元素绑定到当前单元格（写入 st.cellMap，saveLayout 落 cell_element_map）。 */
function cellElementBody (st, snap) {
  const cm = st.cellMap[snap.addr] || {}
  const el = selectedElement(st)
  const bound = cm.elementCode
    ? `<div class="rd-fields">
        <label>元素编码</label><input readonly value="${esc(cm.elementCode)}">
        <label>值类型</label><input readonly value="${esc(cm.valueType || '-')}">
        <label>取数来源</label><input readonly value="${esc(cm.dataSource || '-')}">
      </div>
      <div class="rd-actions"><button class="rd-sbtn danger" type="button" data-cell-unbind><ui5-icon name="decline"></ui5-icon>解除绑定</button></div>`
    : '<div class="rd-mini-empty">当前单元格未绑定数据元素。</div>'
  const picker = el
    ? `<div class="rd-fields">
        <label>待绑定</label><input readonly value="${esc(el.name || el.code)} (${esc(el.code)})">
        <label>数据类型</label><input readonly value="${esc(el.data_type || '-')}">
      </div>
      <div class="rd-actions"><button class="rd-sbtn primary" type="button" data-cell-bind="${esc(el.code)}"><ui5-icon name="chain-link"></ui5-icon>绑定到 ${esc(snap.addr)}</button></div>`
    : '<div class="rd-mini-empty">先在左侧「数据元素」中选择一个元素，再回到此处绑定。</div>'
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>当前绑定</b>${bound}</section>
    <section class="rd-sec"><b>绑定新元素</b>${picker}</section>
  </div>`
}

/** 公式 tab：单元格公式编辑（写回 SpreadJS）+ 元素映射侧的取数/校验公式（存 cellMap，计算另案）。 */
function cellFormulaBody (st, snap) {
  const cm = st.cellMap[snap.addr] || {}
  return `<div class="rd-prop-grid">
    <section class="rd-sec"><b>单元格公式</b>
      <div class="rd-fields">
        <label class="wide">公式（= 开头）</label>
        <textarea class="wide" data-cell-input="formula" placeholder="=SUM(C4:C10)">${esc(snap.formula)}</textarea>
      </div>
      <div class="rd-actions">
        <button class="rd-sbtn primary" type="button" data-cell-apply="formula"><ui5-icon name="accept"></ui5-icon>写入公式</button>
        <button class="rd-sbtn" type="button" data-cell-clear-formula><ui5-icon name="eraser"></ui5-icon>清除公式</button>
      </div>
    </section>
    <section class="rd-sec"><b>取数 / 校验公式（元素映射）</b>
      <div class="rd-fields">
        <label class="wide">取数公式 calcFormula</label>
        <textarea class="wide" data-cellmap-field="calcFormula" placeholder="QM('1001')  取科目期末余额">${esc(cm.calcFormula || '')}</textarea>
        <label class="wide">校验公式 checkFormula</label>
        <textarea class="wide" data-cellmap-field="checkFormula" placeholder="A1 = B1 + C1">${esc(cm.checkFormula || '')}</textarea>
      </div>
      <div class="rd-actions">
        <button class="rd-sbtn primary" type="button" data-fx-wizard="calcFormula"><ui5-icon name="function"></ui5-icon>函数向导</button>
        <button class="rd-sbtn" type="button" data-cellmap-save><ui5-icon name="save"></ui5-icon>暂存映射</button>
      </div>
      <div class="rd-note" style="margin-top:8px">用「函数向导」选函数、逐参填（期间/组织/取数对象），不手写括号引号；随「保存」落库，「计算报表」触发后端真算。</div>
    </section>
    ${wizardHtml(st)}
  </div>`
}

// ─────────────────────── 函数向导（选函数 → 逐参填 → 拼串 → 插入） ───────────────────────

/** 向导浮层：未打开则空。分「选函数」与「填参数」两态。 */
function wizardHtml (st) {
  const w = st.wizard
  if (!w) return ''
  const body = w.fn ? wizardParamsHtml(st, w) : wizardPickHtml(st)
  return `<div class="rd-fx-mask" data-fx-close-mask>
    <div class="rd-fx-dlg" role="dialog">
      <div class="rd-fx-head"><b><ui5-icon name="function"></ui5-icon> 函数向导</b>
        <button class="rd-fx-x" type="button" data-fx-cancel><ui5-icon name="decline"></ui5-icon></button></div>
      <div class="rd-fx-body">${body}</div>
    </div>
  </div>`
}

const FX_CAT_LABEL = { fetch: '取数', ref: '引用', agg: '汇总', logic: '逻辑', math: '数学' }

/** 第一步：按分类列函数。 */
function wizardPickHtml (st) {
  if (!st.functionsLoaded) return '<div class="rd-loading"><ui5-icon name="synchronize"></ui5-icon>正在加载函数目录…</div>'
  if (!st.functions.length) return '<div class="rd-empty">函数目录为空或加载失败。</div>'
  const groups = {}
  for (const f of st.functions) { (groups[f.category] = groups[f.category] || []).push(f) }
  const order = ['fetch', 'ref', 'agg', 'logic', 'math']
  const secs = order.filter((c) => groups[c]).map((c) => {
    const items = groups[c].map((f) => `<button class="rd-fx-item" type="button" data-fx-pick="${esc(f.name)}">
      <span class="rd-fx-name">${esc(f.name)}</span><span class="rd-fx-help">${esc(f.help || '')}</span></button>`).join('')
    return `<div class="rd-fx-group"><div class="rd-fx-glabel">${esc(FX_CAT_LABEL[c] || c)}</div>${items}</div>`
  }).join('')
  return `<div class="rd-fx-pick">${secs}</div>`
}

/** 第二步：按 prototype 逐参渲染控件 + 实时公式串 + 预览。 */
function wizardParamsHtml (st, w) {
  const fn = w.fn
  const params = wizardParamList(fn)
  const rows = params.map((p, i) => {
    const val = w.args[i] != null ? w.args[i] : (p.default || '')
    return `<div class="rd-fx-row"><label>${esc(p.name)}${p.required ? ' *' : ''}</label>
      ${wizardControl(st, p, i, val)}
      <div class="rd-fx-hint">${esc(p.hint || '')}</div></div>`
  }).join('')
  const formula = buildFormula(fn, w.args)
  return `<div class="rd-fx-params">
    <div class="rd-fx-fn"><b>${esc(fn.name)}</b> — ${esc(fn.help || '')} <span class="rd-fx-eg">例：${esc(fn.example || '')}</span></div>
    <div class="rd-fx-grid">${rows || '<div class="rd-fx-hint">该函数无固定参数</div>'}</div>
    <div class="rd-fx-out"><label>公式</label><code>${esc(formula || '—')}</code></div>
    <div class="rd-fx-actions">
      <button class="rd-sbtn" type="button" data-fx-back><ui5-icon name="nav-back"></ui5-icon>重选函数</button>
      <button class="rd-sbtn primary" type="button" data-fx-insert><ui5-icon name="accept"></ui5-icon>插入到 ${esc(w.target || '')}</button>
    </div>
  </div>`
}

/** 参数列表（固定参 + 变参展开一格供追加）。 */
function wizardParamList (fn) {
  const params = (fn.prototype && fn.prototype.params) ? fn.prototype.params.slice() : []
  const v = fn.prototype && fn.prototype.variadic
  if (v) params.push({ ...v, name: (v.name || '值') + '…', variadic: true })
  return params
}

/** 参数控件：按 kind 渲染（期间/组织/对象=下拉复用元素与组织；其余=文本框）。 */
function wizardControl (st, p, i, val) {
  const attr = `data-fx-arg="${i}"`
  if (p.kind === 'period') {
    const opts = [['0', '本期(0)'], ['-1', '上期(-1)'], ['-2', '上两期(-2)'], ['-12', '上年同期(-12)']]
    const list = opts.map(([v, l]) => `<option value="${v}" ${String(val) === v ? 'selected' : ''}>${l}</option>`).join('')
    return `<select ${attr}>${list}<option value="__abs" ${!opts.some(([v]) => v === String(val)) && val ? 'selected' : ''}>绝对期间…</option></select>
      <input ${attr}-abs placeholder="或输入 2026-06" value="${esc(!opts.some(([v]) => v === String(val)) ? val : '')}" class="rd-fx-abs">`
  }
  if (p.kind === 'org') {
    return `<select ${attr}><option value="@current" ${val === '@current' || !val ? 'selected' : ''}>@当前组织</option>
      <option value="@parent" ${val === '@parent' ? 'selected' : ''}>@上级组织</option>
      <option value="__code" ${val && val[0] !== '@' ? 'selected' : ''}>指定组织码…</option></select>
      <input ${attr}-code placeholder="组织码" value="${esc(val && val[0] !== '@' ? val : '')}" class="rd-fx-abs">`
  }
  if (p.kind === 'object') {
    const els = (st.elements || []).slice(0, 200)
    const opts = els.map((e) => `<option value="${esc(e.code)}" ${val === e.code ? 'selected' : ''}>${esc(e.code)} ${esc(e.name || '')}</option>`).join('')
    return `<input ${attr} list="rd-fx-obj-${i}" placeholder="科目码/元素码" value="${esc(val)}">
      <datalist id="rd-fx-obj-${i}">${opts}</datalist>`
  }
  if (p.kind === 'direction') {
    return `<select ${attr}><option value="net" ${val === 'net' || !val ? 'selected' : ''}>净额</option>
      <option value="debit" ${val === 'debit' ? 'selected' : ''}>借方</option>
      <option value="credit" ${val === 'credit' ? 'selected' : ''}>贷方</option></select>`
  }
  // cellref / report / version / number / text / expr → 文本框
  return `<input ${attr} placeholder="${esc(p.hint || '')}" value="${esc(val)}">`
}

/** 由函数 + 参数值拼公式串（引号/括号自动，用户不手写）。 */
function buildFormula (fn, args) {
  const params = wizardParamList(fn)
  const parts = []
  for (let i = 0; i < params.length; i++) {
    const p = params[i]
    let v = args[i]
    if (v == null || v === '') { if (p.required && p.kind !== 'expr') v = p.default || ''; else continue }
    if (v == null || v === '') continue
    parts.push(formatArg(p, v))
  }
  // 去掉尾部空缺
  while (parts.length && (parts[parts.length - 1] === '' || parts[parts.length - 1] == null)) parts.pop()
  return `${fn.name}(${parts.join(',')})`
}

/** 单参格式化：对象/科目码/文本加引号，期间/组织/数值/单元格/表达式裸写。 */
function formatArg (p, v) {
  const s = String(v)
  if (p.kind === 'object' || p.kind === 'report' || p.kind === 'version' || p.kind === 'text' || p.kind === 'direction') {
    return `'${s.replace(/'/g, '')}'`
  }
  return s // period(0/-1/2026-06) / org(@current/CODE) / number / cellref / expr
}

/** 参数改动时只更新公式串预览（不整页重渲染，避免输入焦点丢失）。 */
function refreshWizardOut (root, st) {
  if (!st.wizard || !st.wizard.fn) return
  const out = root.querySelector('.rd-fx-out code')
  if (out) out.textContent = buildFormula(st.wizard.fn, st.wizard.args) || '—'
}

function propertyElementHtml (st) {
  const it = selectedElement(st)
  const refreshAct = `<button class="rd-ibtn ${st.elementsLoading ? 'spin' : ''}" type="button" data-el-refresh title="刷新数据元素"><ui5-icon name="refresh"></ui5-icon></button>`
  if (!it) {
    const hint = st.elementsLoading
      ? '<div class="rd-loading"><ui5-icon name="synchronize"></ui5-icon>正在加载数据元素...</div>'
      : (st.elements.length
        ? '<div class="rd-empty">请在左侧数据元素中选择一个元素。</div>'
        : '<div class="rd-empty">尚未加载数据元素，点击右上角刷新。</div>')
    return `<section class="rd">${headHtml(st, '元素属性', 'database', refreshAct)}<div class="rd-body">${hint}</div></section>`
  }
  const cat = elementCategory(st, it)
  const target = st.selectedCell || 'A1'
  return `<section class="rd">${headHtml(st, '元素属性', 'database', refreshAct)}<div class="rd-body">
    <div class="rd-actions" style="margin:0 0 9px">
      <button class="rd-sbtn primary" type="button" data-el-insert="${esc(it.code)}"><ui5-icon name="download-from-cloud"></ui5-icon>填入 ${esc(target)}</button>
      <button class="rd-sbtn" type="button" data-el-bind="${esc(it.code)}"><ui5-icon name="chain-link"></ui5-icon>绑定到 ${esc(target)}</button>
    </div>
    <div class="rd-prop-grid">
    <section class="rd-sec"><b>基本信息</b>
      ${roRow('元素编码', it.code)}
      ${roRow('元素名称', it.name)}
      ${roRow('所属类别', cat ? `${cat.name || cat.code} (${cat.code || ''})` : it.category_code)}
      ${roRow('状态', it.status == null ? '启用' : it.status)}
    </section>
    <section class="rd-sec"><b>数据属性</b>
      ${roRow('数据类型', it.data_type)}
      ${roRow('单位', it.unit)}
      ${roRow('精度', it.decimals)}
      ${roRow('值来源', it.value_source)}
      ${roRow('公式类型', it.formula_type)}
    </section>
    <section class="rd-sec"><b>公式与校验</b>
      ${roRow('计算公式', it.calc_formula)}
      ${roRow('校验公式', it.check_formula)}
    </section>
    <section class="rd-sec"><b>说明</b>
      ${roRow('备注', it.remark)}
    </section>
  </div></div></section>`
}

/** 只读属性行（键值对，值走 input readonly，长值可选中复制）。 */
function roRow (label, value) {
  return `<div class="rd-row"><span>${esc(label)}</span><input readonly value="${esc(value == null || value === '' ? '-' : value)}"></div>`
}

function viewHtml (view, st) {
  if (view === 'explorerModel') return explorerModelHtml(st)
  if (view === 'propertyMeta') return propertyMetaHtml(st)
  if (view === 'propertyCell') return propertyCellHtml(st)
  if (view === 'propertyElement') return propertyElementHtml(st)
  if (view === 'content') return contentHtml(st)
  return explorerDataHtml(st)
}

function bind (root, st, host) {
  const view = host?.__rptDesignerNativeView
  bindElementExplorer(root, st, host)
  if (view === 'content') {
    bindSheetToolbar(root, st)
    initSpreadComponent(root, st)
  } else if (view === 'propertyMeta' || view === 'propertyCell' || view === 'propertyElement') {
    bindPropertyPage(root, st, host, view)
  }
}

/**
 * 属性页交互绑定。属性页无自身 sheet 元素——所有 [data-sheet-cmd] 与写值动作都路由到
 * content 宿主里的在屏 SpreadJS 组件（liveSheetOf）。改动后本地重渲染对应属性视图。
 */
function bindPropertyPage (root, st, host, view) {
  const live = () => liveSheetOf(st)
  const rerender = () => refreshInstance(st, (v) => v === view)

  // 顶部刷新按钮（元素刷新由 bindElementExplorer 统一绑定，此处仅报表详情）
  root.querySelectorAll('[data-meta-refresh]').forEach((b) => b.addEventListener('click', () => loadReportDetail(st, true)))

  // tab 切换
  root.querySelectorAll('[data-meta-tab]').forEach((b) => b.addEventListener('click', () => {
    st.metaTab = b.getAttribute('data-meta-tab') || 'report'
    if (st.metaTab === 'report' || st.metaTab === 'version') loadReportDetail(st)
    rerender()
  }))
  root.querySelectorAll('[data-cell-tab]').forEach((b) => b.addEventListener('click', () => {
    st.cellTab = b.getAttribute('data-cell-tab') || 'cell'
    rerender()
  }))

  // 透传到在屏 sheet 的表格命令（网格线/表头/可编辑/插入删除行列/清除）
  root.querySelectorAll('[data-sheet-cmd]').forEach((b) => b.addEventListener('click', () => {
    const sheet = live()
    if (!sheet) { toast(root, '请先在设计区打开电子表格', 'error'); return }
    runSheetCommand(b.getAttribute('data-sheet-cmd') || '', sheet, st, root)
    rerender()
  }))

  // —— 区域管理 ——
  root.querySelectorAll('[data-region-field]').forEach((el) => el.addEventListener('input', () => {
    st.regionDraft = st.regionDraft || { name: '', type: 'data', range: '' }
    st.regionDraft[el.getAttribute('data-region-field')] = el.value
  }))
  root.querySelector('[data-region-use-selection]')?.addEventListener('click', () => {
    const sheet = live()
    const sel = sheet?.readSelection ? sheet.readSelection() : (sheet?.getSelectionState?.().selection || '')
    st.regionDraft = { ...(st.regionDraft || { name: '', type: 'data' }), range: sel || '' }
    rerender()
  })
  root.querySelector('[data-region-add]')?.addEventListener('click', () => addRegion(st, root))
  root.querySelectorAll('[data-region-del]').forEach((b) => b.addEventListener('click', () => {
    const code = b.getAttribute('data-region-del')
    st.regions = (st.regions || []).filter((r) => r.code !== code)
    markDirty(st, true)
    toast(root, `已删除区域 ${code}`, 'success')
    rerender()
  }))

  // —— 单元格写值 / 公式 ——
  root.querySelectorAll('[data-cell-apply]').forEach((b) => b.addEventListener('click', () => {
    const kind = b.getAttribute('data-cell-apply')
    const box = root.querySelector(`[data-cell-input="${kind}"]`)
    applyCellInput(st, root, kind, box ? box.value : '')
  }))
  root.querySelector('[data-cell-clear-formula]')?.addEventListener('click', () => applyCellInput(st, root, 'formula', ''))
  root.querySelectorAll('[data-cellmap-field]').forEach((el) => el.addEventListener('input', () => {
    const addr = st.selectedCell || 'A1'
    const cm = st.cellMap[addr] = st.cellMap[addr] || {}
    cm[el.getAttribute('data-cellmap-field')] = el.value
    markDirty(st, true)
  }))
  root.querySelector('[data-cellmap-save]')?.addEventListener('click', () => {
    markDirty(st, true)
    toast(root, `已暂存 ${st.selectedCell} 的公式映射（保存报表时落库）`, 'success')
  })

  // —— 函数向导 ——
  root.querySelectorAll('[data-fx-wizard]').forEach((b) => b.addEventListener('click', () => {
    const field = b.getAttribute('data-fx-wizard')
    st.wizard = { fn: null, args: [], target: st.selectedCell || 'A1', field }
    loadFunctions(st).then(() => rerender())
    rerender()
  }))
  root.querySelector('[data-fx-cancel]')?.addEventListener('click', () => { st.wizard = null; rerender() })
  root.querySelector('[data-fx-close-mask]')?.addEventListener('click', (ev) => {
    if (ev.target === ev.currentTarget) { st.wizard = null; rerender() }
  })
  root.querySelectorAll('[data-fx-pick]').forEach((b) => b.addEventListener('click', () => {
    const name = b.getAttribute('data-fx-pick')
    const fn = st.functions.find((f) => f.name === name)
    if (fn && st.wizard) { st.wizard.fn = fn; st.wizard.args = wizardParamList(fn).map((p) => p.default || '') }
    rerender()
  }))
  root.querySelector('[data-fx-back]')?.addEventListener('click', () => { if (st.wizard) { st.wizard.fn = null; st.wizard.args = [] } rerender() })
  root.querySelectorAll('[data-fx-arg]').forEach((el) => el.addEventListener('input', () => {
    if (!st.wizard) return
    const i = Number(el.getAttribute('data-fx-arg'))
    // 期间/组织的「绝对/指定」联动：主 select 选 __abs/__code 时取兄弟输入框
    let v = el.value
    if (v === '__abs' || v === '__code') {
      const sib = el.parentElement.querySelector('.rd-fx-abs')
      v = sib ? sib.value : ''
    } else if (el.classList.contains('rd-fx-abs')) {
      // 输入绝对值：直接作为该参值
      const sel = el.parentElement.querySelector('[data-fx-arg]')
      if (sel && (sel.value === '__abs' || sel.value === '__code')) { st.wizard.args[i] = v; refreshWizardOut(root, st); return }
    }
    st.wizard.args[i] = v
    refreshWizardOut(root, st)
  }))
  root.querySelector('[data-fx-insert]')?.addEventListener('click', () => {
    if (!st.wizard || !st.wizard.fn) return
    const formula = buildFormula(st.wizard.fn, st.wizard.args)
    const addr = st.wizard.target || st.selectedCell || 'A1'
    const field = st.wizard.field || 'calcFormula'
    const cm = st.cellMap[addr] = st.cellMap[addr] || {}
    cm[field] = formula
    st.wizard = null
    markDirty(st, true)
    rerender()
    toast(root, `已插入公式到 ${addr}：${formula}`, 'success')
  })

  // —— 元素绑定 / 填入 ——
  root.querySelectorAll('[data-cell-bind],[data-el-bind]').forEach((b) => b.addEventListener('click', () => {
    bindElementToCell(st, root, b.getAttribute('data-cell-bind') || b.getAttribute('data-el-bind'))
  }))
  root.querySelector('[data-cell-unbind]')?.addEventListener('click', () => {
    const addr = st.selectedCell || 'A1'
    if (st.cellMap[addr]) { delete st.cellMap[addr].elementCode; delete st.cellMap[addr].valueType; delete st.cellMap[addr].dataSource }
    markDirty(st, true)
    toast(root, `已解除 ${addr} 的元素绑定`, 'success')
    rerender()
  })
  root.querySelectorAll('[data-el-insert]').forEach((b) => b.addEventListener('click', () => {
    insertElementValue(st, root, b.getAttribute('data-el-insert'))
  }))
}

/** 新建区域：校验范围（A1:E10）→ 入 st.regions（保存时随投影落库）。 */
function addRegion (st, root) {
  const d = st.regionDraft || {}
  const name = String(d.name || '').trim()
  const range = String(d.range || '').trim().toUpperCase()
  if (!name) { toast(root, '请填写区域名称', 'error'); return }
  if (range && !expandRange(range)) { toast(root, '范围格式应为 A1:E10', 'error'); return }
  const box = range ? expandRange(range) : null
  const code = `RG_${slug(name)}_${(st.regions || []).length + 1}`
  const startCell = box ? `${indexToCol(box.c1)}${box.r1 + 1}` : ''
  const endCell = box ? `${indexToCol(box.c2)}${box.r2 + 1}` : ''
  st.regions = st.regions || []
  st.regions.push({ code, name, type: d.type || 'data', startCell, endCell, isDefault: false, sheetCode: currentSheetCode(st) })
  st.regionDraft = { name: '', type: 'data', range: '' }
  markDirty(st, true)
  toast(root, `已添加区域 ${name}${range ? ` (${range})` : ''}`, 'success')
  refreshInstance(st, (v) => v === 'propertyMeta')
}

/** 把值/公式写入在屏 sheet 的当前选中单元格。 */
function applyCellInput (st, root, kind, raw) {
  const sheet = liveSheetOf(st)
  const ws = sheet?.getWorkbook?.()?.getActiveSheet?.()
  const p = parseA1(st.selectedCell || 'A1')
  if (!ws || !p) { toast(root, '请先在设计区选中单元格', 'error'); return }
  const value = String(raw ?? '')
  const run = () => {
    if (kind === 'formula') {
      if (value.trim()) ws.setFormula(p.row, p.col, value.trim().replace(/^=/, ''))
      else { ws.setFormula(p.row, p.col, null); ws.setValue(p.row, p.col, '') }
    } else if (value.startsWith('=')) {
      ws.setFormula(p.row, p.col, value.slice(1))
    } else {
      ws.setFormula(p.row, p.col, null)
      const num = value !== '' && !Number.isNaN(Number(value)) ? Number(value) : value
      ws.setValue(p.row, p.col, num)
    }
  }
  if (sheet._runUndoable) sheet._runUndoable(kind === 'formula' ? 'cmxFormulaBarEdit' : 'editCell', run)
  else run()
  toast(root, `已写入 ${st.selectedCell}`, 'success')
  refreshInstance(st, (v) => v === 'propertyCell')
}

/** 绑定数据元素到当前单元格（记入 st.cellMap；不改画布值）。 */
function bindElementToCell (st, root, code) {
  if (!code) return
  const el = st.elements.find((x) => String(x.code) === String(code))
  if (!el) { toast(root, '未找到元素', 'error'); return }
  const addr = st.selectedCell || 'A1'
  const cm = st.cellMap[addr] = st.cellMap[addr] || {}
  cm.elementCode = el.code
  cm.valueType = el.data_type || ''
  cm.dataSource = el.value_source || ''
  cm.calcFormula = cm.calcFormula || el.calc_formula || ''
  cm.checkFormula = cm.checkFormula || el.check_formula || ''
  st.selectedElementCode = el.code
  markDirty(st, true)
  toast(root, `已绑定 ${el.code} → ${addr}`, 'success')
  refreshInstance(st, (v) => v === 'propertyCell' || v === 'propertyElement')
}

/** 把元素名称填入当前单元格（作为文本标签），并顺带绑定。 */
function insertElementValue (st, root, code) {
  const el = st.elements.find((x) => String(x.code) === String(code))
  if (!el) { toast(root, '未找到元素', 'error'); return }
  applyCellInput(st, root, 'value', el.name || el.code)
  bindElementToCell(st, root, code)
}

/**
 * 把数据元素绑定到指定单元格地址（拖拽落点用）。已绑定则覆盖。payload 可来自拖拽数据或 st.elements。
 * 绑定信息写入 st.cellMap[addr]（saveLayout 时随 cellMap 落 cr_cell_element_map）。
 */
function bindElementToCellAddr (st, root, payload, addr) {
  if (!addr) return false
  // payload 可能是拖拽 JSON（code/name/dataType/valueSource/calcFormula/checkFormula）或仅 code 字符串
  let p = payload
  if (typeof payload === 'string') {
    const el = st.elements.find((x) => String(x.code) === String(payload))
    p = el ? { code: el.code, name: el.name, dataType: el.data_type, valueSource: el.value_source, calcFormula: el.calc_formula, checkFormula: el.check_formula } : { code: payload }
  }
  const code = p?.code
  if (!code) { toast(root, '无效的数据元素', 'error'); return false }
  const existed = !!st.cellMap[addr]?.elementCode
  const cm = st.cellMap[addr] = st.cellMap[addr] || {}
  // 覆盖现有绑定
  cm.elementCode = code
  cm.valueType = p.dataType || p.valueType || ''
  cm.dataSource = p.valueSource || p.dataSource || ''
  cm.calcFormula = p.calcFormula || ''
  cm.checkFormula = p.checkFormula || ''
  cm.numberFormat = cm.numberFormat || ''
  st.selectedElementCode = code
  st.selectedCell = addr
  markDirty(st, true)
  toast(root, `${existed ? '已覆盖绑定' : '已绑定'} ${code} → ${addr}`, 'success')
  refreshInstance(st, (v) => v === 'propertyCell' || v === 'propertyElement')
  return true
}

/** 落点客户端坐标 → 单元格地址。用 SpreadJS workbook.hitTest（坐标相对 spread 宿主元素）。 */
function cellAddrFromDrop (sheet, clientX, clientY) {
  try {
    const wb = sheet.getWorkbook?.()
    if (!wb || typeof wb.hitTest !== 'function') return null
    // hitTest 坐标须相对 SpreadJS 画布宿主（组件 shadow 内 .sheet / canvas 容器）
    const shadow = sheet.shadowRoot
    const hostEl = shadow?.querySelector('.sheet') || shadow?.querySelector('canvas')?.parentElement || shadow?.querySelector('canvas')
    const rect = (hostEl || sheet).getBoundingClientRect()
    const x = clientX - rect.left
    const y = clientY - rect.top
    const info = wb.hitTest(x, y)
    const hi = info?.worksheetHitInfo
    if (!hi || hi.row == null || hi.col == null || hi.row < 0 || hi.col < 0) return null
    return `${indexToCol(hi.col)}${hi.row + 1}`
  } catch (_) { return null }
}

/** 从拖拽 dataTransfer 解析数据元素 payload（多 MIME 兜底）。 */
function parseDragElement (dt) {
  if (!dt) return null
  for (const mime of ['application/x-cmx-report-element', 'application/json']) {
    try {
      const raw = dt.getData(mime)
      if (raw) { const p = JSON.parse(raw); if (p && p.code) return p }
    } catch (_) {}
  }
  const txt = (() => { try { return dt.getData('text/plain') } catch (_) { return '' } })()
  return txt ? { code: txt } : null
}

function sheetElOf (root) {
  return root.querySelector('[data-rd-spread]')
}

function applySheetUiStyle (sheet, st, patch = {}) {
  Object.assign(st.sheetUi, patch)
  if (!sheet || typeof sheet.applySelectionStyle !== 'function') return
  const ui = st.sheetUi
  sheet.applySelectionStyle({
    font: fontSpec(ui),
    hAlign: ui.align,
    vAlign: ui.valign === 'middle' ? 'center' : ui.valign,
    textDecoration: ui.underline ? 'underline' : 'none',
    foreColor: ui.fontColor,
    backColor: ui.fillColor,
    formatter: NUMBER_FORMATS[ui.format] || '',
    wordWrap: ui.wordWrap,
  })
}

function syncSheetUiFromSelection (sheet, st) {
  if (!sheet || typeof sheet.getSelectionState !== 'function') return
  const s = sheet.getSelectionState() || {}
  if (s.selection) st.selectedRange = s.selection
  if (s.addr) st.selectedCell = s.addr
  // 组件 getSelectionState 用 split(/\s+/).pop() 回读字体族，多词族(如 "Microsoft YaHei")会被截成 "YaHei"。
  // 若回读值是某个已知族的末词，还原成完整族名，避免反复切换时字体族逐步丢失。
  const readFamily = reconcileFontFamily(s.fontFamily, st.sheetUi.fontFamily)
  Object.assign(st.sheetUi, {
    bold: !!s.bold,
    italic: !!s.italic,
    underline: !!s.underline,
    align: s.align || st.sheetUi.align || 'left',
    valign: s.valign || st.sheetUi.valign || 'middle',
    fontFamily: readFamily || st.sheetUi.fontFamily || 'Arial',
    fontSize: s.fontSize || st.sheetUi.fontSize || '11',
    fontColor: s.fontColor || st.sheetUi.fontColor || '#1d2d3e',
    fillColor: s.fillColor || st.sheetUi.fillColor || '#ffffff',
    wordWrap: !!s.wordWrap,
  })
  const fmt = Object.entries(NUMBER_FORMATS).find(([, pattern]) => pattern === (s.format || ''))
  st.sheetUi.format = fmt ? fmt[0] : (s.format ? 'general' : st.sheetUi.format || 'general')
}

/** 把被截断的字体族末词还原成完整族名（对齐字体下拉选项 + 当前族）。 */
const FONT_FAMILIES = ['Arial', 'Microsoft YaHei', 'SimSun', 'SimHei', 'KaiTi', 'Calibri', 'Times New Roman', 'Courier New']
function reconcileFontFamily (readback, current) {
  const rb = String(readback || '').trim()
  if (!rb) return current || ''
  // 若当前族的末词 == 回读值，说明只是被截断，保留当前完整族
  if (current && String(current).split(/\s+/).pop() === rb && String(current) !== rb) return current
  // 否则匹配下拉选项里末词相同的完整族
  const full = FONT_FAMILIES.find((f) => f.split(/\s+/).pop() === rb)
  return full || rb
}

function updateToolbarControls (root, st) {
  const ui = st.sheetUi
  const sel = root.querySelector('[data-rd-selection]')
  if (sel) sel.textContent = st.selectedRange || st.selectedCell || 'A1'
  root.querySelectorAll('[data-sheet-cmd]').forEach((btn) => {
    const cmd = btn.getAttribute('data-sheet-cmd')
    const active = (cmd === 'bold' && ui.bold) ||
      (cmd === 'italic' && ui.italic) ||
      (cmd === 'underline' && ui.underline) ||
      (cmd === 'align-left' && ui.align === 'left') ||
      (cmd === 'align-center' && ui.align === 'center') ||
      (cmd === 'align-right' && ui.align === 'right') ||
      (cmd === 'valign-top' && ui.valign === 'top') ||
      (cmd === 'valign-middle' && ui.valign === 'middle') ||
      (cmd === 'valign-bottom' && ui.valign === 'bottom') ||
      (cmd === 'wrap' && ui.wordWrap) ||
      (cmd === 'toggle-gridlines' && ui.gridlines) ||
      (cmd === 'toggle-headers' && ui.headers) ||
      (cmd === 'toggle-editable' && ui.editable)
    btn.classList.toggle('active', !!active)
    if (btn.hasAttribute('aria-pressed')) btn.setAttribute('aria-pressed', active ? 'true' : 'false')
  })
  root.querySelectorAll('[data-sheet-field]').forEach((el) => {
    const field = el.getAttribute('data-sheet-field')
    if (!field || ui[field] == null) return
    if (el.type === 'color') {
      const val = ui[field] || (field === 'fillColor' ? '#ffffff' : '#1d2d3e')
      el.value = val
      const holder = el.closest('.rd-color')
      if (holder) holder.style.setProperty('--rd-swatch', val)
    } else {
      el.value = String(ui[field])
      const holder = el.closest('.rd-menu-tool')
      const valEl = holder?.querySelector('.rd-mt-val')
      if (valEl) {
        const opt = el.selectedOptions && el.selectedOptions[0]
        valEl.textContent = field === 'format'
          ? (FORMAT_SHORT[ui[field]] || opt?.textContent || String(ui[field]))
          : (opt?.textContent || String(ui[field]))
      }
    }
  })
  const sheet = sheetElOf(root)
  const history = sheet?.getHistoryState?.() || {}
  root.querySelectorAll('[data-sheet-cmd="undo"]').forEach((btn) => { btn.disabled = history.canUndo !== true })
  root.querySelectorAll('[data-sheet-cmd="redo"]').forEach((btn) => { btn.disabled = history.canRedo !== true })
  root.querySelectorAll('[data-history-toggle="undo"]').forEach((btn) => { btn.disabled = history.canUndo !== true })
  root.querySelectorAll('[data-history-toggle="redo"]').forEach((btn) => { btn.disabled = history.canRedo !== true })
  root.querySelectorAll('[data-history="undo"]').forEach((el) => el.classList.toggle('disabled', history.canUndo !== true))
  root.querySelectorAll('[data-history="redo"]').forEach((el) => el.classList.toggle('disabled', history.canRedo !== true))
}

function renderHistoryMenu (root, sheet, kind) {
  const menu = root.querySelector(`[data-history-menu="${kind}"]`)
  if (!menu) return
  const history = sheet?.getHistoryState?.() || {}
  const items = kind === 'redo' ? (history.redo || []) : (history.undo || [])
  const title = kind === 'redo' ? '重做至此' : '撤销至此'
  if (!items.length) {
    menu.innerHTML = `<span class="rd-history-empty">暂无${kind === 'redo' ? '重做' : '撤销'}记录</span>`
    return
  }
  const icon = toolIcon(kind === 'redo' ? 'rotate-cw' : 'rotate-ccw')
  menu.innerHTML = `<div class="rd-history-title">${esc(title)}</div>` + items.slice(0, 30).map((it) => `<button class="rd-history-item" type="button" data-history-step="${esc(kind)}" data-history-count="${Number(it.steps) || 1}">
    <i>${icon}</i>
    <span>${esc(it.label)}</span>
    <small>${Number(it.steps) || 1}</small>
  </button>`).join('')
}

function closeHistoryMenus (root) {
  root.querySelectorAll('[data-history].open').forEach((el) => el.classList.remove('open'))
}

/**
 * 保存报表设计。后端 /api/report-design/* 暂无“保存设计版式”端点，
 * 先把当前 SpreadJS 版式序列化为 JSON 下载到本地（占位），同时广播
 * cmx-report-save-request 供后续接入落库端点时监听。
 */
function saveDesign (sheet, st, root) {
  saveLayout(sheet, st, root)
}

// ============================================================================
// 报表两模式加载存储 —— ReportModel 单一事实源
//   模式一 版式：cr_report_fmt(BLOB) + 关系投影(sheet/region/row/col)
//   模式二 数据：cr_cell_data 按 org+period
// 结构层级 Report ▸ Sheet ▸ Region ▸ Row×Col ▸ Cell；无区域→默认区域 __default__。
// ============================================================================

const DEFAULT_REGION = '__default__'

/** URL 路径分段编码 */
const enc = (s) => encodeURIComponent(String(s ?? ''))

/** base64 编解码 SSJSON（UTF-8 安全） */
function encodeDoc (obj) {
  const json = JSON.stringify(obj || {})
  const bytes = new TextEncoder().encode(json)
  let bin = ''
  const chunk = 0x8000
  for (let i = 0; i < bytes.length; i += chunk) {
    bin += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk))
  }
  return btoa(bin)
}
function decodeDoc (b64) {
  if (!b64) return null
  try {
    const bin = atob(b64)
    const bytes = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
    return JSON.parse(new TextDecoder().decode(bytes))
  } catch (_) { return null }
}

/**
 * 从后端 layout 响应组装完整 ReportModel（结构化视角）。BLOB 存在时以 workbookJson 为渲染主真相。
 * 关系表 rows/cols/cells 按 sheet+region 归并到 regions[] 三层嵌套。
 */
function buildReportModel (st, data) {
  const meta = {
    reportCode: st.props.reportCode || '',
    reportName: st.props.reportName || '',
    version: st.props.version || '',
  }
  const sheetsRaw = Array.isArray(data?.sheets) ? data.sheets : []
  const regionsRaw = Array.isArray(data?.regions) ? data.regions : []
  const rowsRaw = Array.isArray(data?.rows) ? data.rows : []
  const colsRaw = Array.isArray(data?.cols) ? data.cols : []

  const sheets = sheetsRaw.map((sh) => {
    const sheetCode = String(sh.name || sh.sheet_code || sh.report_code || 'Sheet1')
    const regionsForSheet = regionsRaw.filter((r) => String(r.sheet_code || '') === sheetCode)
    const regs = (regionsForSheet.length ? regionsForSheet : [{ region_code: DEFAULT_REGION, region_name: '默认区域', is_default: 1 }])
      .map((rg) => {
        const rc = String(rg.region_code || DEFAULT_REGION)
        return {
          code: rc,
          name: rg.region_name || '',
          type: rg.region_type || '',
          startCell: rg.start_cell || '', endCell: rg.end_cell || '',
          isDefault: rc === DEFAULT_REGION || Number(rg.is_default) === 1,
          rows: rowsRaw.filter((r) => String(r.sheet_code) === sheetCode && String(r.region_code || DEFAULT_REGION) === rc)
            .map((r) => ({ id: r.id, code: r.code, name: r.name, rowNo: r.row_no, rowType: r.row_type, parentId: r.parent_id, levelNo: r.level_no, formula: r.formula })),
          cols: colsRaw.filter((c) => String(c.sheet_code) === sheetCode && String(c.region_code || DEFAULT_REGION) === rc)
            .map((c) => ({ id: c.id, code: c.code, name: c.name, colNo: c.col_no, colLetter: c.col_letter, colType: c.col_type, valueType: c.value_type, formula: c.formula })),
          cells: {},
        }
      })
    return { id: sheetCode, name: sheetCode, sheetIndex: sh.sheet_index, props: { rowCount: sh.row_count, colCount: sh.col_count }, grid: {}, regions: regs }
  })

  return { meta, sheets, workbookJson: decodeDoc(data?.fmt?.docContent) }
}

/**
 * 从在屏工作簿派生关系投影（存版式用）。规则：
 *   - 表格上「有数据/公式/样式」的单元格 → 生成其行、列、单元格；
 *   - 未定义区域 → 默认区域 __default__，本 sheet 所有行列单元格归其下；
 *   - 有显式区域 → 按 startCell:endCell 判定单元格落在哪个区域（否则默认区域兜底）。
 * 返回 { sheets, regions, rows, cols, cellMap }，rows/cols 带临时 id（后端铸真号回 idMap）。
 */
function deriveProjection (sheet, st) {
  const wb = sheet.getWorkbook?.()
  const out = { sheets: [], regions: [], rows: [], cols: [], cellMap: [] }
  if (!wb) return out
  const count = wb.getSheetCount ? wb.getSheetCount() : 1
  for (let si = 0; si < count; si++) {
    const ws = wb.getSheet ? wb.getSheet(si) : wb.getActiveSheet()
    if (!ws) continue
    const sheetCode = ws.name ? ws.name() : `Sheet${si + 1}`
    const rowCount = ws.getRowCount ? ws.getRowCount() : 0
    const colCount = ws.getColumnCount ? ws.getColumnCount() : 0
    out.sheets.push({ sheetIndex: si, name: sheetCode, rowCount, colCount, sortNo: si })

    // 显式区域（来自属性页维护的 st.regions；无 sheetCode 视为本 sheet；默认区域除外）
    const explicitRegions = (st.regions || []).filter((r) => !r.isDefault && r.code !== DEFAULT_REGION && (!r.sheetCode || r.sheetCode === sheetCode))
    const regionForCell = (r, c) => {
      for (const rg of explicitRegions) {
        const box = expandRange(rg.startCell && rg.endCell ? `${rg.startCell}:${rg.endCell}` : '')
        if (box && r >= box.r1 && r <= box.r2 && c >= box.c1 && c <= box.c2) return rg.code
      }
      return DEFAULT_REGION
    }
    // 单元格↔区域反查（cellMap 落库时要带真实 region）
    const usedRegions = new Set()

    // 扫描有内容的单元格 → 收集行号/列号（按区域分组）
    const rowsByRegion = {} // region -> Set(rowIndex)
    const colsByRegion = {} // region -> Set(colIndex)
    const seenRefs = new Set() // 已产出 cellMap 的 cellRef，避免与绑定单元格重复
    const scanRows = Math.min(rowCount, 500)
    const scanCols = Math.min(colCount, 100)
    for (let r = 0; r < scanRows; r++) {
      for (let c = 0; c < scanCols; c++) {
        const val = ws.getValue ? ws.getValue(r, c) : null
        const formula = ws.getFormula ? ws.getFormula(r, c) : null
        if ((val === null || val === undefined || val === '') && !formula) continue
        const rc = regionForCell(r, c)
        usedRegions.add(rc)
        ;(rowsByRegion[rc] = rowsByRegion[rc] || new Set()).add(r)
        ;(colsByRegion[rc] = colsByRegion[rc] || new Set()).add(c)
        const cellRef = `${indexToCol(c)}${r + 1}`
        seenRefs.add(cellRef)
        // 元素/公式映射（属性页维护的 st.cellMap）合并进 cellMap 记录
        const bind = (si === 0 && st.cellMap[cellRef]) ? st.cellMap[cellRef] : null
        out.cellMap.push({
          sheetCode, regionCode: rc,
          rowId: `t:${sheetCode}:${rc}:r${r}`, colId: `t:${sheetCode}:${rc}:c${c}`,
          cellRef,
          elementCode: bind?.elementCode || '',
          valueType: bind?.valueType || (formula ? 'formula' : (typeof val === 'number' ? 'number' : 'text')),
          dataSource: bind?.dataSource || '',
          calcFormula: bind?.calcFormula || (formula ? `=${formula}` : ''),
          checkFormula: bind?.checkFormula || '',
          numberFormat: bind?.numberFormat || '',
        })
      }
    }
    // ★ 绑定了数据元素但单元格本身为空（拖拽绑定不写值）的格子也要产出 cellMap，否则保存后绑定丢失。
    //   仅第 0 sheet（st.cellMap 按 cellRef 存，不区分 sheet）。
    if (si === 0) {
      for (const [cellRef, bind] of Object.entries(st.cellMap || {})) {
        if (!bind || !bind.elementCode || seenRefs.has(cellRef)) continue
        const p = parseA1(cellRef)
        if (!p || p.row < 0 || p.col < 0) continue
        const rc = regionForCell(p.row, p.col)
        usedRegions.add(rc)
        ;(rowsByRegion[rc] = rowsByRegion[rc] || new Set()).add(p.row)
        ;(colsByRegion[rc] = colsByRegion[rc] || new Set()).add(p.col)
        seenRefs.add(cellRef)
        out.cellMap.push({
          sheetCode, regionCode: rc,
          rowId: `t:${sheetCode}:${rc}:r${p.row}`, colId: `t:${sheetCode}:${rc}:c${p.col}`,
          cellRef,
          elementCode: bind.elementCode,
          valueType: bind.valueType || '',
          dataSource: bind.dataSource || '',
          calcFormula: bind.calcFormula || '',
          checkFormula: bind.checkFormula || '',
          numberFormat: bind.numberFormat || '',
        })
      }
    }
    // 属性页登记但可能尚无数据的显式区域也要保留（空区域）
    explicitRegions.forEach((rg) => usedRegions.add(rg.code))
    if (!usedRegions.size) usedRegions.add(DEFAULT_REGION)

    // 区域记录
    for (const rc of usedRegions) {
      const rg = explicitRegions.find((x) => x.code === rc)
      out.regions.push({
        sheetCode, code: rc, name: rg?.name || (rc === DEFAULT_REGION ? '默认区域' : rc),
        type: rg?.type || (rc === DEFAULT_REGION ? 'data' : ''), startCell: rg?.startCell || '', endCell: rg?.endCell || '',
        isDefault: rc === DEFAULT_REGION ? 1 : 0,
      })
      // 行记录（每区域内按行号排序，code 用 R{n}）
      for (const r of [...(rowsByRegion[rc] || new Set())].sort((a, b) => a - b)) {
        out.rows.push({ sheetCode, regionCode: rc, code: `R${r + 1}`, name: '', rowNo: r, id: `t:${sheetCode}:${rc}:r${r}` })
      }
      for (const c of [...(colsByRegion[rc] || new Set())].sort((a, b) => a - b)) {
        out.cols.push({ sheetCode, regionCode: rc, code: indexToCol(c), name: '', colNo: c, colLetter: indexToCol(c), id: `t:${sheetCode}:${rc}:c${c}` })
      }
    }
  }
  return out
}

/** 模式一 · 存版式：wb.toJSON() → BLOB + 派生关系投影 → POST layout */
async function saveLayout (sheet, st, root) {
  const wbJson = sheet.getWorkbookJson ? sheet.getWorkbookJson() : (sheet.getWorkbook?.()?.toJSON?.() || null)
  if (!wbJson) { toast(root, '保存失败：工作簿未就绪', 'error'); return }
  const proj = deriveProjection(sheet, st)
  const payload = {
    version: st.props.version || '',
    fmt: { docContent: encodeDoc(wbJson), docFormat: 'ssjson', mimeType: 'application/json', contentHash: st.__contentHash || null },
    sheets: proj.sheets, regions: proj.regions, rows: proj.rows, cols: proj.cols, cellMap: proj.cellMap,
  }
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/layout`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload),
    })
    st.__contentHash = res?.contentHash || st.__contentHash
    markDirty(st, false) // 版式已保存 → 清除未保存标记
    toast(root, '报表版式已保存', 'success')
    return true
  } catch (err) {
    const msg = String(err?.message || err)
    toast(root, msg.includes('409') || msg.includes('他人') ? '版式已被他人更新，请刷新后重试' : `保存失败：${msg}`, 'error')
    return false
  }
}

/** 模式一 · 加载版式：GET layout → 有 BLOB 用 setWorkbookJson 无损复原，否则按结构渲染 */
async function loadLayout (sheet, st, root) {
  try {
    const data = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/layout?version=${enc(st.props.version || '')}`)
    st.__model = buildReportModel(st, data)
    st.__contentHash = data?.fmt?.contentHash || null
    hydratePropsFromLayout(st, data)
    const wbJson = st.__model.workbookJson
    if (wbJson && sheet.setWorkbookJson) {
      await sheet.setWorkbookJson(wbJson)
    } else if (sheet.setReportModel) {
      sheet.setReportModel(reportModel(st)) // 无 BLOB：渲染初始骨架
    }
    // 同步真实活动 sheet 名（BLOB 恢复后可能是 STAT_01_D 等，而非默认 Sheet1）
    syncActiveSheetName(sheet, st)
    syncSheetUiFromSelection(sheet, st)
    updateToolbarControls(root, st)
    // 属性页可能已打开——用加载到的区域/映射刷新
    refreshInstance(st, (v) => v === 'propertyMeta' || v === 'propertyCell')
    return true
  } catch (err) {
    // 首次设计（无版式记录）走初始骨架，不算错误
    if (sheet.setReportModel) sheet.setReportModel(reportModel(st))
    return false
  }
}

/** 把后端 layout 的区域/单元格映射回填到 st.regions / st.cellMap，供属性页展示与再保存。 */
function hydratePropsFromLayout (st, data) {
  const regionsRaw = Array.isArray(data?.regions) ? data.regions : []
  st.regions = regionsRaw
    .filter((r) => String(r.region_code || '') !== DEFAULT_REGION)
    .map((r) => ({
      code: String(r.region_code || ''),
      name: r.region_name || '',
      type: r.region_type || 'data',
      startCell: r.start_cell || '',
      endCell: r.end_cell || '',
      isDefault: false,
      sheetCode: r.sheet_code || st.activeSheet,
    }))
  const cmRaw = Array.isArray(data?.cellMap) ? data.cellMap : []
  const cm = {}
  for (const m of cmRaw) {
    const ref = m.cell_ref
    if (!ref) continue
    cm[ref] = {
      elementCode: m.element_code || '',
      valueType: m.value_type || '',
      dataSource: m.data_source || '',
      calcFormula: m.calc_formula || '',
      checkFormula: m.check_formula || '',
      numberFormat: m.number_format || '',
    }
  }
  st.cellMap = cm
}

/** 模式二 · 加载数据：POST data/query → 回填画布值（保留版式与公式） */
async function loadReportData (sheet, st, root, orgCode, periodCode) {
  if (!orgCode || !periodCode) { toast(root, '请先选择组织与期间', 'error'); return }
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/data/query`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version: st.props.version || '', orgCode, periodCode }),
    })
    const cells = res?.cells || []
    const valuesMap = {}
    for (const r of cells) {
      if (!r.cellRef) continue
      valuesMap[r.cellRef] = r.valueType === 'number' ? r.numValue : r.textValue
    }
    if (sheet.setCellValues) sheet.setCellValues(valuesMap)
    toast(root, `已加载 ${cells.length} 个单元格数据`, 'success')
  } catch (err) {
    toast(root, `加载数据失败：${String(err?.message || err)}`, 'error')
  }
}

/** 模式二 · 存数据：收集画布有值单元格 → POST data（按 org+period UPSERT cr_cell_data） */
async function saveReportData (sheet, st, root, orgCode, periodCode) {
  if (!orgCode || !periodCode) { toast(root, '请先选择组织与期间', 'error'); return }
  const wb = sheet.getWorkbook?.()
  const ws = wb?.getActiveSheet?.()
  if (!ws) { toast(root, '保存失败：工作簿未就绪', 'error'); return }
  const sheetCode = ws.name ? ws.name() : 'Sheet1'
  const cells = []
  const rc = Math.min(ws.getRowCount ? ws.getRowCount() : 0, 500)
  const cc = Math.min(ws.getColumnCount ? ws.getColumnCount() : 0, 100)
  for (let r = 0; r < rc; r++) {
    for (let c = 0; c < cc; c++) {
      const formula = ws.getFormula ? ws.getFormula(r, c) : null
      if (formula) continue // 公式格不落数据（由取数/计算产生）
      const val = ws.getValue ? ws.getValue(r, c) : null
      if (val === null || val === undefined || val === '') continue
      const isNum = typeof val === 'number' && Number.isFinite(val)
      cells.push({
        sheetCode, regionCode: DEFAULT_REGION,
        rowId: 0, colId: 0, cellRef: `${indexToCol(c)}${r + 1}`,
        valueType: isNum ? 'number' : 'text',
        textValue: isNum ? null : String(val),
        numValue: isNum ? String(val) : null,
        isManual: 1,
      })
    }
  }
  try {
    await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/data`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version: st.props.version || '', orgCode, periodCode, cells }),
    })
    toast(root, `已保存 ${cells.length} 个单元格数据`, 'success')
  } catch (err) {
    toast(root, `保存数据失败：${String(err?.message || err)}`, 'error')
  }
}



/** 轻量 toast 提示（挂在当前视图 section 内，绝对定位，自动淡出）。 */
function toast (root, message, kind = 'info') {
  // 延后一帧：多数调用方会紧接着 refreshInstance 重渲染 root，先渲染再挂 toast 才不会被抹掉。
  requestAnimationFrame(() => {
    const host = root.querySelector('.rd-sheet-wrap') || root.querySelector('.rd') || root
    if (!host) return
    if (host.classList?.contains('rd') && getComputedStyle(host).position === 'static') host.style.position = 'relative'
    let box = host.querySelector(':scope > .rd-toast')
    if (!box) {
      box = document.createElement('div')
      box.className = 'rd-toast'
      host.appendChild(box)
    }
    box.setAttribute('data-kind', kind)
    box.textContent = message
    box.classList.remove('show')
    void box.offsetWidth
    box.classList.add('show')
    clearTimeout(box.__t)
    box.__t = setTimeout(() => box.classList.remove('show'), 3200)
  })
}

function runSheetCommand (cmd, sheet, st, root) {
  const ui = st.sheetUi
  if (!sheet) return
  switch (cmd) {
    case 'save':
      saveDesign(sheet, st, root)
      break
    case 'export-xlsx':
      sheet.exportXlsx?.(`${st.props.reportCode || 'report'}-${st.props.version || 'default'}`)
      break
    case 'import-xlsx':
      root.querySelector('[data-rd-import-file]')?.click()
      break
    case 'undo': sheet.undo?.(); break
    case 'redo': sheet.redo?.(); break
    case 'clear-value': sheet.clearSelection?.('value'); break
    case 'clear-format': sheet.clearSelection?.('format'); break
    case 'bold': applySheetUiStyle(sheet, st, { bold: !ui.bold }); break
    case 'italic': applySheetUiStyle(sheet, st, { italic: !ui.italic }); break
    case 'underline': applySheetUiStyle(sheet, st, { underline: !ui.underline }); break
    case 'align-left': applySheetUiStyle(sheet, st, { align: 'left' }); break
    case 'align-center': applySheetUiStyle(sheet, st, { align: 'center' }); break
    case 'align-right': applySheetUiStyle(sheet, st, { align: 'right' }); break
    case 'valign-top': applySheetUiStyle(sheet, st, { valign: 'top' }); break
    case 'valign-middle': applySheetUiStyle(sheet, st, { valign: 'middle' }); break
    case 'valign-bottom': applySheetUiStyle(sheet, st, { valign: 'bottom' }); break
    case 'wrap': applySheetUiStyle(sheet, st, { wordWrap: !ui.wordWrap }); break
    case 'border-all': sheet.applySelectionBorder?.('all'); break
    case 'border-outline': sheet.applySelectionBorder?.('outline'); break
    case 'border-none': sheet.applySelectionBorder?.('none'); break
    case 'merge': sheet.mergeSelection?.(); break
    case 'unmerge': sheet.unmergeSelection?.(); break
    case 'insert-row': sheet.insertRows?.(1); break
    case 'delete-row': sheet.deleteRows?.(1); break
    case 'insert-col': sheet.insertColumns?.(1); break
    case 'delete-col': sheet.deleteColumns?.(1); break
    case 'toggle-gridlines':
      ui.gridlines = !ui.gridlines
      sheet.showGridlines?.(ui.gridlines)
      break
    case 'toggle-headers':
      ui.headers = !ui.headers
      sheet.showHeaders?.(ui.headers)
      break
    case 'toggle-editable':
      ui.editable = !ui.editable
      sheet.setEditable?.(ui.editable)
      break
  }
  // 会改动版式的命令 → 置 dirty（保存/导出/导入/撤销重做/纯显示切换不算）。
  const MUTATING = new Set([
    'clear-value', 'clear-format', 'bold', 'italic', 'underline',
    'align-left', 'align-center', 'align-right', 'valign-top', 'valign-middle', 'valign-bottom',
    'wrap', 'border-all', 'border-outline', 'border-none', 'merge', 'unmerge',
    'insert-row', 'delete-row', 'insert-col', 'delete-col',
  ])
  if (MUTATING.has(cmd) && !st.__loading) markDirty(st, true)
  syncSheetUiFromSelection(sheet, st)
  updateToolbarControls(root, st)
}

function setupHistoryMenus (root, st, sheet) {
  if (!sheet || root.__rdHistoryBound) return
  root.__rdHistoryBound = true
  root.querySelectorAll('[data-history-toggle]').forEach((btn) => {
    btn.addEventListener('click', (event) => {
      event.stopPropagation()
      const kind = btn.getAttribute('data-history-toggle') || 'undo'
      const wrap = btn.closest('[data-history]')
      const willOpen = !wrap?.classList.contains('open')
      closeHistoryMenus(root)
      if (!willOpen || !wrap) return
      renderHistoryMenu(root, sheet, kind)
      wrap.classList.add('open')
    })
  })
  root.addEventListener('click', (event) => {
    const item = event.target.closest?.('[data-history-step]')
    if (!item) return
    const kind = item.getAttribute('data-history-step') || 'undo'
    const count = Math.max(1, Number(item.getAttribute('data-history-count')) || 1)
    if (kind === 'redo') sheet.redoSteps?.(count)
    else sheet.undoSteps?.(count)
    closeHistoryMenus(root)
    syncSheetUiFromSelection(sheet, st)
    updateToolbarControls(root, st)
  })
  document.addEventListener('click', () => closeHistoryMenus(root))
}

function setupRibbonOverflow (root) {
  const ribbon = root.querySelector('[data-rd-ribbon]')
  const main = root.querySelector('[data-rd-ribbon-main]')
  const more = root.querySelector('[data-rd-more]')
  const menu = root.querySelector('[data-rd-more-menu]')
  const toggle = root.querySelector('[data-rd-more-toggle]')
  if (!ribbon || !main || !more || !menu || !toggle || ribbon.__rdOverflowBound) return
  ribbon.__rdOverflowBound = true
  const rebalance = () => {
    if (!ribbon.isConnected) return
    more.classList.remove('open')
    toggle.setAttribute('aria-expanded', 'false')
    Array.from(menu.children).forEach((item) => main.appendChild(item))
    more.hidden = true
    if (main.scrollWidth <= main.clientWidth) return
    more.hidden = false
    while (main.scrollWidth > main.clientWidth && main.lastElementChild) {
      menu.prepend(main.lastElementChild)
    }
    more.hidden = !menu.children.length
  }
  toggle.addEventListener('click', (event) => {
    event.stopPropagation()
    const open = more.classList.toggle('open')
    toggle.classList.toggle('active', open)
    toggle.setAttribute('aria-expanded', open ? 'true' : 'false')
  })
  document.addEventListener('click', (event) => {
    if (!more.contains(event.target)) {
      more.classList.remove('open')
      toggle.classList.remove('active')
      toggle.setAttribute('aria-expanded', 'false')
    }
  })
  if (typeof ResizeObserver === 'function') {
    const ro = new ResizeObserver(() => requestAnimationFrame(rebalance))
    ro.observe(ribbon)
  } else {
    window.addEventListener('resize', rebalance)
  }
  requestAnimationFrame(rebalance)
}

function bindSheetToolbar (root, st) {
  const sheet = sheetElOf(root)
  if (!sheet) return
  root.querySelectorAll('[data-sheet-cmd]').forEach((btn) => {
    btn.addEventListener('click', () => runSheetCommand(btn.getAttribute('data-sheet-cmd') || '', sheet, st, root))
  })
  root.querySelectorAll('[data-sheet-field]').forEach((field) => {
    field.addEventListener('change', () => {
      const key = field.getAttribute('data-sheet-field')
      if (!key) return
      const value = field.value
      if (key === 'format') applySheetUiStyle(sheet, st, { format: value || 'general' })
      else applySheetUiStyle(sheet, st, { [key]: value })
      if (!st.__loading) markDirty(st, true) // 字体/字号/颜色/格式改动 → 未保存
      updateToolbarControls(root, st)
    })
  })
  const file = root.querySelector('[data-rd-import-file]')
  file?.addEventListener('change', () => {
    const f = file.files?.[0]
    if (!f || typeof sheet.importXlsx !== 'function') return
    sheet.importXlsx(f).then((res) => {
      const model = reportModel(st)
      if (res?.sheets?.length) model.sheets = res.sheets
      sheet.setReportModel?.(model)
      file.value = ''
    }).catch(() => { file.value = '' })
  })
  syncSheetUiFromSelection(sheet, st)
  updateToolbarControls(root, st)
  setupHistoryMenus(root, st, sheet)
  setupRibbonOverflow(root)
}

function bindElementExplorer (root, st, host) {
  root.querySelectorAll('[data-el-refresh]').forEach((btn) => {
    btn.addEventListener('click', () => loadElements(st, true))
  })
  const search = root.querySelector('[data-el-search]')
  if (search && !search.__rdBound) {
    search.__rdBound = true
    search.addEventListener('input', () => {
      st.elementQuery = search.value || ''
      if (st.__elementSearchTimer) clearTimeout(st.__elementSearchTimer)
      st.__elementSearchTimer = setTimeout(() => {
        refreshInstance(st, (view) => view === 'explorerData')
        requestAnimationFrame(() => {
          const next = findExplorerRoot(st)?.querySelector('[data-el-search]')
          if (next) {
            next.focus()
            const len = next.value.length
            try { next.setSelectionRange(len, len) } catch {}
          }
        })
      }, 120)
    })
  }
  root.querySelectorAll('[data-el-clear]').forEach((btn) => {
    btn.addEventListener('click', () => {
      st.elementQuery = ''
      refreshInstance(st, (view) => view === 'explorerData')
      requestAnimationFrame(() => findExplorerRoot(st)?.querySelector('[data-el-search]')?.focus())
    })
  })
  root.querySelectorAll('[data-cat-toggle]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const code = btn.getAttribute('data-cat-toggle') || ''
      if (!code) return
      if (st.collapsedCategories.has(code)) st.collapsedCategories.delete(code)
      else st.collapsedCategories.add(code)
      refreshInstance(st, (view) => view === 'explorerData')
    })
  })
  root.querySelectorAll('[data-element-drag]').forEach((item) => {
    item.addEventListener('click', () => {
      const code = item.getAttribute('data-element-select') || ''
      if (!code) return
      st.selectedElementCode = code
      refreshInstance(st, (view) => view === 'explorerData' || view === 'propertyElement')
      activatePropertyElementView(st, host || root)
    })
    item.addEventListener('dragstart', (e) => {
      const raw = item.getAttribute('data-element-drag') || '{}'
      let payload = {}
      try { payload = JSON.parse(raw) } catch {}
      if (payload.code) {
        st.selectedElementCode = String(payload.code)
        refreshInstance(st, (view) => view === 'propertyElement')
      }
      item.classList.add('dragging')
      if (e.dataTransfer) {
        e.dataTransfer.effectAllowed = 'copy'
        e.dataTransfer.setData('application/json', JSON.stringify(payload))
        e.dataTransfer.setData('application/x-cmx-report-element', JSON.stringify(payload))
        e.dataTransfer.setData('text/plain', payload.code || payload.name || '')
      }
    })
    item.addEventListener('dragend', () => item.classList.remove('dragging'))
  })
}

function findExplorerRoot (st) {
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected || host.__rptDesignerNativeView !== 'explorerData') continue
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    if (root) return root
  }
  return null
}

function collectDeep (root, selector, out = []) {
  if (!root) return out
  try {
    if (root.querySelectorAll) {
      root.querySelectorAll(selector).forEach((el) => out.push(el))
      root.querySelectorAll('*').forEach((el) => {
        if (el.shadowRoot) collectDeep(el.shadowRoot, selector, out)
      })
    }
  } catch {}
  return out
}

function activatePropertyElementView (st, source) {
  const viewId = propertyElementViewId(st)
  const detail = { area: 'property', view: 'propertyElement', viewId }
  const workspace = source?.workspace || source?.host?.workspace || findWorkspaceFromDom(source)
  if (activateWorkspaceView(workspace, detail)) return
  try { source?.dispatchEvent?.(new CustomEvent('cmx-workspace-activate-view', { detail, bubbles: true, composed: true })) } catch {}

  const tryActivate = () => {
    const embeds = collectDeep(document, 'cmx-embed-page')
    for (const embed of embeds) {
      const pages = String(embed.getAttribute?.('pages') || '')
      if (!pages.split(/[,\s]+/).includes(viewId)) continue
      try {
        embed.setAttribute('initial-view', viewId)
        if (typeof embed._activate === 'function') embed._activate(viewId)
        return true
      } catch {}
    }
    const btns = collectDeep(document, `[data-view-id="${viewId}"],[data-view="${viewId}"]`)
    const btn = btns.find((el) => typeof el.click === 'function')
    if (btn) {
      try { btn.click(); return true } catch {}
    }
    return false
  }

  if (tryActivate()) return
  setTimeout(tryActivate, 60)
  setTimeout(tryActivate, 180)
}

function findWorkspaceFromDom (source) {
  let node = source instanceof Element ? source : source?.host
  while (node) {
    if (node.workspace) return node.workspace
    if (node.dataset?.cmxWorkspaceId) {
      const wsId = node.dataset.cmxWorkspaceId
      const ma = globalThis.mainapp
      return ma?.workspaces?.[wsId] || ma?.activityScopes?.[wsId] || null
    }
    node = node.parentElement || (node.parentNode instanceof ShadowRoot ? node.parentNode.host : null)
  }
  const ma = globalThis.mainapp
  const wsId = ma?.activeWorkspaceId
  return wsId ? ma?.workspaces?.[wsId] : null
}

function activateWorkspaceView (workspace, detail) {
  if (!workspace) return false
  const attempts = [
    ['activateView', [{ region: detail.area, area: detail.area, view: detail.view, viewId: detail.viewId }]],
    ['activateView', [detail.area, detail.viewId]],
    ['activateView', [detail.viewId]],
    ['selectView', [{ region: detail.area, area: detail.area, view: detail.view, viewId: detail.viewId }]],
    ['selectView', [detail.area, detail.viewId]],
    ['selectView', [detail.viewId]],
    ['setActiveView', [{ region: detail.area, area: detail.area, view: detail.view, viewId: detail.viewId }]],
    ['setActiveView', [detail.area, detail.viewId]],
    ['setActiveView', [detail.viewId]],
    ['activateRegionView', [{ region: detail.area, area: detail.area, view: detail.view, viewId: detail.viewId }]],
    ['activateRegionView', [detail.area, detail.viewId]],
    ['selectRegionView', [detail.area, detail.viewId]],
    ['setActiveRegionView', [detail.area, detail.viewId]],
    ['viewManager.activate', [{ region: detail.area, area: detail.area, view: detail.view, viewId: detail.viewId }]],
    ['viewManager.activate', [detail.area, detail.viewId]],
    ['viewManager.select', [detail.area, detail.viewId]],
    ['viewManager.setActive', [detail.area, detail.viewId]],
  ]
  for (const [path, args] of attempts) {
    const fn = path.split('.').reduce((obj, key) => obj?.[key], workspace)
    if (typeof fn !== 'function') continue
    try {
      fn.apply(path.includes('.') ? workspace.viewManager : workspace, args)
      return true
    } catch {}
  }
  try { workspace.dispatchEvent?.(new CustomEvent('cmx-workspace-activate-view', { detail })) } catch {}
  return false
}

function ensureSpreadElementRegistered () {
  if (customElements.get('cmx-spreadjs-sheet')) return true
  const C = (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}
  if (C.CmxSpreadjsSheet) {
    try {
      customElements.define('cmx-spreadjs-sheet', C.CmxSpreadjsSheet)
      return true
    } catch {}
  }
  return false
}

/**
 * 直接绑 SpreadJS 的用户编辑事件 → markDirty。
 * ★ 组件内部只绑了 Events.CellChanged（编程改值触发），**用户键盘输入提交走 ValueChanged/EditEnded，
 * 不一定触发 CellChanged**，故在此补绑。编辑事件既可绑 workbook 也可绑 worksheet，两者都绑最稳。
 * 工作簿未就绪则重试。
 */
function bindWorkbookEditEvents (sheet, st, tries = 0) {
  const wb = sheet.getWorkbook?.()
  if (!wb) {
    if (tries < 20) setTimeout(() => bindWorkbookEditEvents(sheet, st, tries + 1), 300)
    return
  }
  if (wb.__rdEditBound) return
  wb.__rdEditBound = true
  const onEdit = () => { if (!st.__loading) markDirty(st, true) }
  // 含行高/列宽调整（设计器改版式，ColumnWidthChanged/RowHeightChanged 也算修改）
  const EVENTS = ['ValueChanged', 'EditEnded', 'ClipboardPasted', 'RangeChanged', 'CellChanged',
    'DragDropBlockCompleted', 'DragFillBlockCompleted', 'ColumnWidthChanged', 'RowHeightChanged']
  for (const name of EVENTS) { try { wb.bind(name, onEdit) } catch (_) {} }
  const bindSheet = (ws) => {
    if (!ws || ws.__rdEditBound) return
    ws.__rdEditBound = true
    for (const name of EVENTS) { try { ws.bind(name, onEdit) } catch (_) {} }
  }
  try { const cnt = wb.getSheetCount?.() || 1; for (let i = 0; i < cnt; i++) bindSheet(wb.getSheet?.(i)) } catch (_) { bindSheet(wb.getActiveSheet?.()) }
  try { wb.bind('ActiveSheetChanged', () => bindSheet(wb.getActiveSheet?.())) } catch (_) {}
}

/**
 * 直接绑 SpreadJS SelectionChanged/LeaveCell/EnterCell → 同步选中格到 st.selectedCell + 刷新 property 单元格页。
 * ★ 组件虽也绑了 SelectionChanged 派发 cmx-cell-selected，但真机上该派发到不了本 root（跨宿主）；
 *   且 SpreadJS 编程改选区不触发 SelectionChanged。故除事件外**再加一个活动格轮询**兜底（getActiveAddr
 *   实时反映当前选中格，250ms 轮询变化即同步），保证鼠标点选/键盘移动都联动。工作簿未就绪则重试。
 */
function bindSelectionSync (sheet, st, root, tries = 0) {
  const wb = sheet.getWorkbook?.()
  if (!wb) {
    if (tries < 20) setTimeout(() => bindSelectionSync(sheet, st, root, tries + 1), 300)
    return
  }
  if (wb.__rdSelBound) return
  wb.__rdSelBound = true
  const onSelect = () => {
    // 读“在屏”content 宿主的活动格（可能有多个 content 实例，liveSheetOf 取活着的那个）
    const live = liveSheetOf(st) || sheet
    const addr = (typeof live.getActiveAddr === 'function') ? live.getActiveAddr() : null
    if (!addr || addr === st.selectedCell) return
    st.selectedCell = addr
    syncSheetUiFromSelection(live, st)
    // property 单元格页跨宿主刷新（联动核心）+ content 工具栏回显
    refreshInstance(st, (view) => view === 'propertyCell')
    updateToolbarControlsAll(st)
  }
  const EVENTS = ['SelectionChanged', 'LeaveCell', 'EnterCell']
  for (const name of EVENTS) { try { wb.bind(name, onSelect) } catch (_) {} }
  const bindSheet = (ws) => {
    if (!ws || ws.__rdSelBound) return
    ws.__rdSelBound = true
    for (const name of EVENTS) { try { ws.bind(name, onSelect) } catch (_) {} }
  }
  try { const cnt = wb.getSheetCount?.() || 1; for (let i = 0; i < cnt; i++) bindSheet(wb.getSheet?.(i)) } catch (_) { bindSheet(wb.getActiveSheet?.()) }
  try { wb.bind('ActiveSheetChanged', () => bindSheet(wb.getActiveSheet?.())) } catch (_) {}
  // 兜底轮询：编程选区变化 + 部分真机交互 SelectionChanged 不触发，靠比对 getActiveAddr 捕获。
  if (!st.__rdSelPoll) {
    st.__rdSelPoll = setInterval(() => {
      // 只在有 content 宿主活着时轮询；实例全关则停
      const hasContent = Array.from(st.hosts).some((h) => h && h.isConnected && h.__rptDesignerNativeView === 'content')
      if (!hasContent) { clearInterval(st.__rdSelPoll); st.__rdSelPoll = null; return }
      onSelect()
    }, 250)
  }
}

/** 跨所有 content 宿主刷新工具栏控件（选区变化时更新工具栏回显）。 */
function updateToolbarControlsAll (st) {
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected || host.__rptDesignerNativeView !== 'content') continue
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    if (root) { try { updateToolbarControls(root, st) } catch (_) {} }
  }
}

/**
 * 数据元素拖拽落到画布单元格 → 绑定（已绑则覆盖）。
 * dragover 必须 preventDefault 才允许 drop；drop 时解析 payload + hitTest 落点求格 → bindElementToCellAddr。
 * ★ 性能：dragover 每秒触发几十次，绝不能在里面 hitTest/setActiveCell/setSelection（每次都强制 SpreadJS
 *   重绘 + 唤醒选区轮询 → 假死）。dragover 只做 preventDefault + 轻量边框高亮；落点格解析只在 drop 时做一次。
 */
function bindElementDrop (root, sheet, st) {
  const stage = root.querySelector('.rd-sheet-stage') || root.querySelector('.rd-spread-host') || sheet
  if (!stage || stage.__rdDropBound) return
  stage.__rdDropBound = true
  const isElementDrag = (dt) => {
    if (!dt) return false
    const types = Array.from(dt.types || [])
    return types.includes('application/x-cmx-report-element') || types.includes('application/json') || types.includes('text/plain')
  }
  stage.addEventListener('dragover', (e) => {
    if (!isElementDrag(e.dataTransfer)) return
    e.preventDefault() // 允许 drop（这是 dragover 里唯一必须做的事）
    try { e.dataTransfer.dropEffect = 'copy' } catch (_) {}
    if (!stage.classList.contains('rd-drop-hot')) stage.classList.add('rd-drop-hot')
    // 节流的落点格提示：只更新一个轻量 DOM 标签(不碰 SpreadJS 画布/不改选区)，最多 ~12fps。
    const now = Date.now()
    if (now - (stage.__rdDropTs || 0) < 80) return
    stage.__rdDropTs = now
    const addr = cellAddrFromDrop(sheet, e.clientX, e.clientY)
    showDropHint(stage, addr, e.clientX, e.clientY)
  })
  stage.addEventListener('dragleave', (e) => {
    if (!stage.contains(e.relatedTarget)) { stage.classList.remove('rd-drop-hot'); hideDropHint(stage) }
  })
  stage.addEventListener('drop', (e) => {
    stage.classList.remove('rd-drop-hot'); hideDropHint(stage)
    if (!isElementDrag(e.dataTransfer)) return
    e.preventDefault()
    const payload = parseDragElement(e.dataTransfer)
    if (!payload) { toast(root, '未识别到拖拽的数据元素', 'error'); return }
    // 落点格解析只在 drop 时做一次
    const addr = cellAddrFromDrop(sheet, e.clientX, e.clientY) || st.selectedCell || 'A1'
    bindElementToCellAddr(st, root, payload, addr)
  })
}

/** 拖拽落点提示标签（纯 DOM 覆盖层，跟随光标显示目标单元格地址，不触发 SpreadJS 重绘）。 */
function showDropHint (stage, addr, clientX, clientY) {
  let hint = stage.__rdDropHint
  if (!hint) {
    hint = document.createElement('div')
    hint.className = 'rd-drop-hint'
    stage.appendChild(hint)
    stage.__rdDropHint = hint
  }
  hint.textContent = addr ? `绑定到 ${addr}` : ''
  hint.style.display = addr ? 'block' : 'none'
  const r = stage.getBoundingClientRect()
  hint.style.left = `${clientX - r.left + 14}px`
  hint.style.top = `${clientY - r.top + 14}px`
}

function hideDropHint (stage) {
  if (stage.__rdDropHint) stage.__rdDropHint.style.display = 'none'
}

function initSpreadComponent (root, st) {
  const sheet = root.querySelector('[data-rd-spread]')
  if (!sheet || sheet.__rdBound) return
  sheet.__rdBound = true
  setupSaveRequestListener(st)
  // 拖动改行高/列宽 → 组件派发 cmx-row-resized/cmx-col-resized（用户拖拽才触发，初始装载忽略）→ 置 dirty
  sheet.addEventListener('cmx-col-resized', () => { if (!st.__loading) markDirty(st, true) })
  sheet.addEventListener('cmx-row-resized', () => { if (!st.__loading) markDirty(st, true) })
  // 数据元素拖拽落到单元格 → 绑定（已绑则覆盖）。落点 hitTest 求格；含拖拽经过高亮。
  bindElementDrop(root, sheet, st)
  const model = reportModel(st)
  const applyModel = () => {
    try {
      if (typeof sheet.showFormulaBar === 'function') sheet.showFormulaBar(true)
      if (typeof sheet.showHeaders === 'function') sheet.showHeaders(true)
      if (typeof sheet.showGridlines === 'function') sheet.showGridlines(true)
      // 打开报表：从后端加载已存版式（有 BLOB 无损复原，无则初始骨架）
      st.__loading = true
      loadLayout(sheet, st, root).catch(() => {
        if (typeof sheet.setReportModel === 'function') sheet.setReportModel(model)
      }).finally(() => {
        setTimeout(() => { st.__loading = false }, 300)
        bindWorkbookEditEvents(sheet, st) // 用户键盘编辑靠这个（组件只绑 CellChanged，用户输入不触发）
        bindSelectionSync(sheet, st, root) // 选中单元格联动 property 单元格页（组件派发跨宿主到不了本 root）
      })
      syncSheetUiFromSelection(sheet, st)
      updateToolbarControls(root, st)
    } catch (err) {
      st.__loading = false
      sheet.insertAdjacentHTML('afterend', `<div class="rd-note" style="margin:10px">SpreadJS 初始化失败：${esc(err instanceof Error ? err.message : String(err))}</div>`)
    }
  }
  sheet.addEventListener('cmx-cell-selected', (e) => {
    const addr = e.detail?.addr
    if (!addr) return
    syncSheetUiFromSelection(sheet, st)
    updateToolbarControls(root, st)
    st.selectedCell = addr
    refreshInstance(st, (view) => view === 'propertyCell')
  })
  sheet.addEventListener('cmx-cell-edited', () => {
    syncSheetUiFromSelection(sheet, st)
    updateToolbarControls(root, st)
    refreshInstance(st, (view) => view === 'propertyCell')
    if (!st.__loading) markDirty(st, true) // 用户改单元格 → 未保存
  })
  sheet.addEventListener('cmx-sheet-changed', (e) => {
    syncActiveSheetName(sheet, st)
    if (!st.activeSheet) {
      const idx = Number(e.detail?.index)
      st.activeSheet = Number.isFinite(idx) ? `Sheet${idx + 1}` : st.activeSheet
    }
    refreshInstance(st, (view) => view === 'propertyMeta')
  })
  if (ensureSpreadElementRegistered()) {
    applyModel()
    return
  }
  customElements.whenDefined('cmx-spreadjs-sheet').then(applyModel)
  setTimeout(() => {
    if (!customElements.get('cmx-spreadjs-sheet')) {
      sheet.insertAdjacentHTML('afterend', '<div class="rd-note" style="margin:10px">cmx-spreadjs-sheet 组件尚未注册，请确认 cmx-data-comp 已在宿主中预加载。</div>')
    }
  }, 1200)
}

function mount (ctx, view) {
  const st = getState(ctx)
  const host = ctx.host
  st.hosts.add(host)
  if (host) host.__rptDesignerNativeView = view
  const render = () => {
    const root = host?.renderRoot || host?.shadowRoot?.querySelector('.native-page-root')
    if (!root || !root.isConnected) return
      root.innerHTML = `<style>${styleCss()}</style>${viewHtml(view, st)}`
    bind(root, st, host)
  }
  requestAnimationFrame(render)
  if (view === 'explorerData') loadElements(st)
  if (view === 'propertyElement') loadElements(st)
  if (view === 'propertyMeta') loadReportDetail(st)
  return `<style>${styleCss()}</style>${viewHtml(view, st)}`
}

function refreshInstance (st, predicate) {
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) { st.hosts.delete(host); continue }
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    if (!root) continue
    const view = host.__rptDesignerNativeView || 'content'
    if (predicate && !predicate(view)) continue
    root.innerHTML = `<style>${styleCss()}</style>${viewHtml(view, st)}`
    bind(root, st, host)
  }
}

export default {
  defaultView: 'content',
  views: {
    async explorerData (ctx) { return mount(ctx, 'explorerData') },
    async explorerModel (ctx) { return mount(ctx, 'explorerModel') },
    async content (ctx) { return mount(ctx, 'content') },
    async propertyMeta (ctx) { return mount(ctx, 'propertyMeta') },
    async propertyCell (ctx) { return mount(ctx, 'propertyCell') },
    async propertyElement (ctx) { return mount(ctx, 'propertyElement') },
  },
}
