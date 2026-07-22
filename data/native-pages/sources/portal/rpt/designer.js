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
    // 结构化抛错：对齐 cmx-doc-source 的错误契约，供 presentDocError 分 conflict/validation/generic 三态。
    const err = new Error((j && (j.msg || j.error)) || `HTTP ${res.status}`)
    if (j && j.code != null) err.code = j.code
    // 乐观锁冲突（后端保留裸 409）：res.status===409 或信封 code===409。
    if (res.status === 409 || (j && j.code === 409)) err.conflict = true
    // 列级校验明细（门户拦截器已把 data.violations 平铺/透传）：结构化逐行展示。
    const vio = (j && (Array.isArray(j.violations) ? j.violations
      : (j.data && Array.isArray(j.data.violations) ? j.data.violations : null)))
    if (vio && vio.length) { err.violations = vio; err.validation = true }
    throw err
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

/** 穿透 shadow DOM 取真正的活动元素（document.activeElement 只到宿主，需逐层下钻 shadowRoot）。 */
function deepActiveElement () {
  let el = (typeof document !== 'undefined' && document.activeElement) || null
  while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement
  return el
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
      fxPanel: null, // content 区 fx 按钮的 Excel 样式函数面板状态 {open,fn,args,target,search,cat,pick}
      // ── 协同 B 档：操作队列 + 追平 ──
      opSeq: 0, // 本地已知的服务端最新 seq（提交基点 baseSeq / 追平游标）
      opQueue: [], // 待发语义操作 [{type,target,payload,clientOpId,baseSeq}]（去抖合并后 flush）
      opFlushTimer: null,
      opPollTimer: null,
      opClientSerial: 0, // clientOpId 流水
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
  let fallback = null
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) continue
    if (host.__rptDesignerNativeView !== 'content') continue
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    const el = root?.querySelector?.('[data-rd-spread]')
    if (!el) continue
    // 多 content 宿主（切页残留等）时优先取在屏可见的那个；都不可见则兜底返回首个。
    const visible = (el.offsetParent !== null) || (el.getClientRects && el.getClientRects().length > 0)
    if (visible) return el
    if (!fallback) fallback = el
  }
  return fallback
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
    /* 报表 split button（顶栏保存/导入/导出下拉） */
    .rd-rpt{position:relative;display:inline-flex;align-items:center}
    .rd-rpt-main{border-radius:6px 0 0 6px;padding:0 10px}
    .rd-rpt-caret{border-radius:0 6px 6px 0;min-width:22px;padding:0 4px;margin-left:1px}
    .rd-rpt-caret svg{width:.62rem;height:.62rem;stroke-width:2.4}
    .rd-rpt-menu{position:fixed;z-index:1000;display:none;flex-direction:column;gap:1px;width:186px;padding:6px;border:1px solid var(--rd-border);border-radius:9px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 36px rgba(10,31,68,.2)}
    .rd-rpt.open .rd-rpt-menu{display:flex}
    .rd-rpt-item{display:flex;align-items:center;gap:8px;width:100%;height:32px;padding:0 10px;border:0;border-radius:6px;background:transparent;color:var(--sapTextColor,#1d2d3e);font:inherit;font-size:12.5px;cursor:pointer;text-align:left}
    .rd-rpt-item svg{width:1rem;height:1rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round;flex:0 0 auto;color:var(--sapContent_IconColor,#475059)}
    .rd-rpt-item:hover{background:color-mix(in srgb,var(--rd-blue) 10%,#fff);color:var(--rd-blue)}.rd-rpt-item:hover svg{color:var(--rd-blue)}
    .rd-rpt-sep{height:1px;margin:4px 6px;background:var(--rd-border)}
    .rd-border{position:relative;display:inline-flex}
    .rd-border-btn .rd-mt-ic svg{width:1.02rem;height:1.02rem}
    .rd-border-menu{position:fixed;z-index:1000;display:none;width:150px;padding:5px;border:1px solid var(--rd-border);border-radius:9px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 36px rgba(10,31,68,.2)}
    .rd-border.open .rd-border-menu{display:block}.rd-border.open .rd-border-btn{background:var(--sapTile_Background,#fff);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-border-item{width:100%;height:30px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;font-size:12px;display:flex;align-items:center;gap:9px;padding:0 8px;text-align:left;cursor:pointer}
    .rd-border-item:hover{background:color-mix(in srgb,var(--rd-blue) 10%,transparent);color:var(--rd-blue)}
    .rd-border-item i{flex:0 0 auto;width:18px;height:18px;display:inline-flex;align-items:center;justify-content:center}.rd-border-item i svg{width:18px;height:18px}
    .rd-border-item span{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .rd-border-menu{width:190px}
    .rd-border-sec{padding:4px 8px 3px;font-size:10px;font-weight:700;color:var(--sapContent_LabelColor,#8a9099);text-transform:uppercase;letter-spacing:.03em}
    .rd-border-lines{display:grid;grid-template-columns:1fr 1fr;gap:2px;padding:0 3px 4px;margin-bottom:3px;border-bottom:1px solid var(--rd-border)}
    .rd-border-colors{display:grid;grid-template-columns:repeat(10,1fr);gap:3px;padding:0 3px 5px;margin-bottom:3px;border-bottom:1px solid var(--rd-border)}
    .rd-border-colors .rd-swatch{width:100%;aspect-ratio:1/1;border:1px solid rgba(0,0,0,.14);border-radius:3px;cursor:pointer;padding:0;transition:transform .1s,box-shadow .1s}
    .rd-border-colors .rd-swatch:hover{transform:scale(1.18);box-shadow:0 2px 6px rgba(10,31,68,.28)}
    .rd-border-colors .rd-swatch.on{outline:2px solid var(--rd-blue);outline-offset:1px}
    .rd-border-line{display:flex;align-items:center;gap:6px;height:26px;padding:0 6px;border:1px solid transparent;border-radius:6px;background:transparent;color:inherit;font:inherit;font-size:11px;cursor:pointer}
    .rd-border-line:hover{background:color-mix(in srgb,var(--rd-blue) 9%,transparent)}
    .rd-border-line.on{background:color-mix(in srgb,var(--rd-blue) 14%,transparent);border-color:color-mix(in srgb,var(--rd-blue) 40%,transparent);color:var(--rd-blue)}
    .rd-border-line svg{width:34px;height:11px;flex:0 0 auto}.rd-border-line span{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
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
    /* 常规下拉选择器（字体/字号）：有边框/背景/自绘箭头，一眼可选，替换透明 select 覆盖式 */
    .rd-select{height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapField_Background,#fff);color:var(--sapTextColor,#1d2d3e);font:12px var(--sapFontFamily,Arial,sans-serif);padding:0 24px 0 9px;cursor:pointer;-webkit-appearance:none;appearance:none;background-image:url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 4.5L6 8l3.5-3.5' fill='none' stroke='%23475059' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");background-repeat:no-repeat;background-position:right 7px center;background-size:11px;transition:border-color .12s,box-shadow .12s}
    .rd-select:hover{border-color:color-mix(in srgb,var(--rd-blue) 40%,var(--rd-border))}
    .rd-select:focus{outline:0;border-color:var(--rd-blue);box-shadow:0 0 0 2px color-mix(in srgb,var(--rd-blue) 16%,transparent)}
    .rd-select.rd-select-font{min-width:104px}
    .rd-select.rd-select-size{width:60px;padding:0 22px 0 8px;text-align:left}
    /* 颜色下拉：按钮(图标+当前色条+小箭头) + 弹出色板（fixed，逃逸 ribbon overflow） */
    .rd-colorbtn{display:inline-flex;align-items:center;gap:3px;height:28px;min-width:34px;padding:0 5px;border:0;border-radius:6px;background:transparent;color:var(--sapContent_IconColor,#475059);cursor:pointer;position:relative;transition:background .12s,color .12s,box-shadow .12s}
    .rd-colorbtn:hover{background:#fff;color:var(--rd-blue);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .rd-colorbtn .rd-cb-ic{display:inline-flex;flex-direction:column;align-items:center;line-height:0}
    .rd-colorbtn .rd-cb-ic svg{width:1.02rem;height:1.02rem;fill:none;stroke:currentColor;stroke-width:1.85;stroke-linecap:round;stroke-linejoin:round}
    .rd-colorbtn .rd-cb-bar{width:16px;height:3px;border-radius:2px;margin-top:1px;background:var(--rd-swatch,#1d2d3e);box-shadow:inset 0 0 0 1px rgba(0,0,0,.12)}
    .rd-colorbtn .rd-cb-car{width:.5rem;height:.5rem;opacity:.5}.rd-colorbtn .rd-cb-car svg{width:.5rem;height:.5rem;stroke-width:2.4}
    .rd-colormenu{position:fixed;z-index:1000;display:none;flex-direction:column;gap:8px;width:206px;padding:10px;border:1px solid var(--rd-border);border-radius:10px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 38px rgba(10,31,68,.22)}
    .rd-colorwrap.open .rd-colormenu{display:flex}
    .rd-colormenu .rd-cm-sec{font-size:10px;font-weight:700;letter-spacing:.04em;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-swatch-grid{display:grid;grid-template-columns:repeat(8,1fr);gap:4px}
    .rd-swatch{width:100%;aspect-ratio:1/1;border:1px solid rgba(0,0,0,.12);border-radius:4px;cursor:pointer;padding:0;transition:transform .1s,box-shadow .1s}
    .rd-swatch:hover{transform:scale(1.16);box-shadow:0 2px 6px rgba(10,31,68,.28);border-color:var(--rd-blue)}
    .rd-cm-none{display:flex;align-items:center;gap:7px;height:28px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapList_HeaderBackground,#f7f9fc);color:var(--sapTextColor,#1d2d3e);font-size:12px;cursor:pointer;padding:0 9px}
    .rd-cm-none:hover{border-color:var(--rd-blue);color:var(--rd-blue)}
    .rd-cm-more{display:flex;align-items:center;justify-content:space-between;font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
    .rd-cm-more label{display:inline-flex;align-items:center;gap:6px;cursor:pointer}.rd-cm-more input[type=color]{width:26px;height:22px;border:1px solid var(--rd-border);border-radius:4px;background:#fff;cursor:pointer;padding:0}
    .rd-more{flex:0 0 auto;position:relative}.rd-more[hidden]{display:none}.rd-more>.rd-tool.active,.rd-more>.rd-tool[aria-expanded="true"]{background:var(--rd-blue);color:#fff}
    .rd-more-menu{position:absolute;right:0;top:36px;z-index:20;min-width:64px;display:none;flex-direction:column;gap:6px;padding:8px;border:1px solid var(--rd-border);border-radius:9px;background:var(--sapPopover_Background,#fff);box-shadow:0 14px 36px rgba(10,31,68,.2)}.rd-more.open .rd-more-menu{display:flex}.rd-more-menu .rd-ribbon-item,.rd-more-menu .rd-group{display:inline-flex}.rd-more-menu .rd-group{background:color-mix(in srgb,var(--rd-border) 22%,transparent);flex-wrap:wrap}.rd-more-menu .rd-ribbon-sep{display:none}
    .rd-sheet-stage{flex:1;min-height:0;overflow:hidden;padding:12px;background:linear-gradient(180deg,color-mix(in srgb,var(--rd-blue) 4%,var(--sapBackgroundColor,#f5f6f7)),var(--sapBackgroundColor,#f5f6f7))}.rd-spread-host{height:100%;min-height:480px;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapTile_Background,#fff);box-shadow:0 4px 18px rgba(10,31,68,.08);overflow:hidden}.rd-spread{display:block;width:100%;height:100%;min-height:480px}
    /* Excel 样式公式栏：名称框 | fx | 公式输入框，一行贴在工具栏与网格之间 */
    .rd-fxbar{flex:0 0 auto;display:flex;align-items:center;height:32px;padding:0 10px;border-bottom:1px solid var(--rd-border);background:var(--sapTile_Background,#fff);box-shadow:inset 0 -1px 0 color-mix(in srgb,var(--rd-border) 60%,transparent)}
    .rd-namebox{position:relative;flex:0 0 auto;width:124px;height:24px;display:flex;align-items:center;border:1px solid color-mix(in srgb,var(--rd-blue) 24%,var(--rd-border));border-radius:5px;background:var(--sapField_Background,#fff);box-shadow:inset 0 1px 2px rgba(10,31,68,.04)}
    .rd-namebox:focus-within{border-color:var(--rd-blue);box-shadow:0 0 0 2px color-mix(in srgb,var(--rd-blue) 18%,transparent)}
    .rd-namebox-input{flex:1;min-width:0;height:100%;border:0;outline:0;background:transparent;padding:0 4px 0 8px;font:700 12px/1 ui-monospace,Menlo,Consolas,monospace;letter-spacing:.02em;color:var(--rd-blue)}
    .rd-namebox-caret{flex:0 0 auto;padding:0 6px 0 2px;font-size:9px;color:color-mix(in srgb,var(--rd-blue) 60%,#888);pointer-events:none}
    .rd-fxbar-sep{flex:0 0 auto;width:1px;height:18px;background:var(--rd-border);margin:0 8px}
    .rd-fx-btn{flex:0 0 auto;height:24px;min-width:30px;padding:0 8px;border:1px solid transparent;border-radius:5px;background:transparent;color:var(--sapContent_LabelColor,#5b6b7b);cursor:pointer;display:inline-flex;align-items:center;justify-content:center;transition:background .12s,color .12s,box-shadow .12s}
    .rd-fx-btn i{font:italic 700 13px/1 "Times New Roman",Georgia,serif;letter-spacing:.02em}
    .rd-fx-btn:hover{background:color-mix(in srgb,var(--rd-blue) 12%,transparent);color:var(--rd-blue);box-shadow:0 1px 3px rgba(10,31,68,.12)}
    .rd-fx-btn:active{background:color-mix(in srgb,var(--rd-blue) 20%,transparent)}
    .rd-fxbar-input{flex:1;min-width:0;height:24px;border:0;outline:0;background:transparent;padding:0 6px;font:13px/1 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e)}
    .rd-fxbar-input::placeholder{color:var(--sapContent_LabelColor,#9aa4b0);font-style:italic}
    .rd-fxbar-input:focus{background:color-mix(in srgb,var(--rd-blue) 5%,transparent);border-radius:5px}
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
    /* Excel 样式函数定义面板（fx 按钮弹出，两栏，独立于旧 wizard） */
    /* ===== fx 函数/公式编辑器：非模态可拖动浮层 · 黄金分割横版 · DARK 自适应 ===== */
    /* 原则：底色/文字一律走 SAP 主题变量（dark 下自动深底浅字），强调只用 --rd-*，tint 混到
       --sapTile_Background（非 #fff，否则 dark 下发白刺眼）。浮层在 .rd 外，故自带 --rd-* 变量。 */
    .rd-fxp-mask{position:static;--rd-blue:#0a6ed1;--rd-cyan:#00a6c8;--rd-green:#10a760;--rd-purple:#7c3aed;--rd-amber:#d98200;--rd-red:#c9372c;--rd-border:var(--sapGroup_TitleBorderColor,#d9e2ec)}
    /* 黄金分割：宽 720 ≈ 高 445 × 1.618，宽 > 高的横版 */
    .rd-fxp-panel{position:fixed;z-index:1000;width:min(720px,94vw);height:446px;max-height:86vh;display:flex;flex-direction:column;background:var(--sapTile_Background,#fff);color:var(--sapTextColor,#1d2d3e);border:1px solid color-mix(in srgb,var(--rd-blue) 30%,var(--rd-border));border-radius:14px;box-shadow:0 26px 70px rgba(0,0,0,.5),0 3px 12px rgba(0,0,0,.34);overflow:hidden}
    .rd-fxp-head{display:flex;align-items:center;justify-content:space-between;padding:10px 14px;cursor:move;background:linear-gradient(135deg,var(--rd-blue),#0a4f9c);color:#fff;user-select:none;flex:0 0 auto}
    .rd-fxp-head b{display:flex;align-items:center;gap:9px;font-size:14px;font-weight:700;color:#fff}
    .rd-fxp-badge{display:inline-flex;align-items:center;justify-content:center;width:26px;height:22px;border-radius:6px;background:rgba(255,255,255,.22);font:italic 800 14px/1 "Times New Roman",Georgia,serif}
    .rd-fxp-x{width:26px;height:26px;border:0;border-radius:7px;background:rgba(255,255,255,.16);color:#fff;cursor:pointer;display:inline-flex;align-items:center;justify-content:center}.rd-fxp-x:hover{background:rgba(255,255,255,.32)}
    /* 公式编辑区（顶部整宽横条，绿色系强调） */
    .rd-fxp-editrow{flex:0 0 auto;display:flex;align-items:stretch;margin:11px 14px 8px;border:1.5px solid color-mix(in srgb,var(--rd-green) 45%,var(--rd-border));border-radius:9px;overflow:hidden}
    .rd-fxp-eq{display:flex;align-items:center;justify-content:center;width:30px;flex:0 0 auto;background:color-mix(in srgb,var(--rd-green) 22%,var(--sapField_Background,#fff));color:var(--rd-green);font:800 16px/1 ui-monospace,Menlo,Consolas,monospace;border-right:1px solid color-mix(in srgb,var(--rd-green) 34%,var(--rd-border))}
    .rd-fxp-expr{flex:1;min-height:44px;max-height:96px;resize:vertical;border:0;outline:0;padding:8px 11px;font:600 13.5px/1.5 ui-monospace,Menlo,Consolas,monospace;color:var(--sapTextColor,#1d2d3e);background:color-mix(in srgb,var(--rd-green) 10%,var(--sapField_Background,#fff))}
    .rd-fxp-expr::placeholder{color:var(--sapContent_LabelColor,#9aa4b0);font-weight:400;font-style:italic}
    /* 调色板：横版三区并排（运算符窄列 + 内置 + 取数），搜索跨整行 */
    .rd-fxp-pal{flex:1;min-height:0;overflow:auto;padding:2px 14px 12px;display:grid;grid-template-columns:150px 1fr 1fr;grid-auto-rows:min-content;gap:9px}
    .rd-fxp-search{grid-column:1/-1;display:flex;align-items:center;gap:7px;height:32px;padding:0 10px;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapField_Background,#fff)}
    .rd-fxp-search ui5-icon{width:.9rem;height:.9rem;color:var(--sapContent_LabelColor,#8a8d90)}
    .rd-fxp-search input{flex:1;border:0;outline:0;background:transparent;font:13px var(--sapFontFamily,Arial);color:var(--sapTextColor,#1d2d3e)}
    .rd-fxp-zone{border-radius:10px;padding:8px 9px 9px;border:1px solid var(--rd-border);border-left-width:4px;min-width:0}
    .rd-fxp-zone-op{border-left-color:var(--rd-amber);background:color-mix(in srgb,var(--rd-amber) 12%,var(--sapTile_Background,#fff))}
    .rd-fxp-zone-builtin{border-left-color:var(--rd-cyan);background:color-mix(in srgb,var(--rd-cyan) 12%,var(--sapTile_Background,#fff))}
    .rd-fxp-zone-fetch{border-left-color:var(--rd-purple);background:color-mix(in srgb,var(--rd-purple) 12%,var(--sapTile_Background,#fff))}
    .rd-fxp-zt{font-size:11px;font-weight:800;letter-spacing:.02em;margin-bottom:7px}
    .rd-fxp-zone-op .rd-fxp-zt{color:var(--rd-amber)}.rd-fxp-zone-builtin .rd-fxp-zt{color:var(--rd-cyan)}.rd-fxp-zone-fetch .rd-fxp-zt{color:var(--rd-purple)}
    .rd-fxp-ops{display:flex;flex-wrap:wrap;gap:5px}
    .rd-fx-op{min-width:32px;height:30px;padding:0 9px;border:1px solid color-mix(in srgb,var(--rd-amber) 40%,var(--rd-border));border-radius:7px;background:var(--sapField_Background,#fff);color:var(--rd-amber);font:700 14px/1 ui-monospace,Menlo,Consolas,monospace;cursor:pointer;transition:transform .1s,box-shadow .1s,background .1s}
    .rd-fx-op:hover{background:color-mix(in srgb,var(--rd-amber) 22%,var(--sapField_Background,#fff));transform:translateY(-1px);box-shadow:0 2px 7px color-mix(in srgb,var(--rd-amber) 40%,transparent)}
    .rd-fx-op-cell{font-family:var(--sapFontFamily,Arial);font-size:11px;font-weight:600;min-width:auto;width:100%;margin-top:2px}
    .rd-fxp-fns{display:flex;flex-direction:column;gap:6px}
    .rd-fx-fn{display:flex;flex-direction:column;align-items:flex-start;gap:1px;text-align:left;padding:6px 9px;border:1px solid var(--rd-border);border-radius:8px;background:var(--sapField_Background,#fff);cursor:pointer;transition:transform .1s,box-shadow .1s,border-color .1s}
    .rd-fx-fn:hover{transform:translateY(-1px)}
    .rd-fx-fnname{font:800 13px/1.1 ui-monospace,Menlo,Consolas,monospace}
    .rd-fx-fnhelp{font-size:10.5px;color:var(--sapContent_LabelColor,#6a6d70);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:100%}
    .rd-fx-fn-builtin .rd-fx-fnname{color:var(--rd-cyan)}.rd-fx-fn-builtin:hover{border-color:var(--rd-cyan);box-shadow:0 2px 8px color-mix(in srgb,var(--rd-cyan) 30%,transparent)}
    .rd-fx-fn-fetch .rd-fx-fnname{color:var(--rd-purple)}.rd-fx-fn-fetch:hover{border-color:var(--rd-purple);box-shadow:0 2px 8px color-mix(in srgb,var(--rd-purple) 30%,transparent)}
    /* 取数参数子面板（横版：逐参两列摊开） */
    .rd-fxp-sub{flex:1;min-height:0;overflow:auto;padding:4px 14px 12px;display:flex;flex-direction:column;gap:9px}
    .rd-fxp-subhead{display:flex;align-items:center;gap:8px;padding:7px 10px;border-radius:9px;background:color-mix(in srgb,var(--rd-purple) 16%,var(--sapTile_Background,#fff));border:1px solid color-mix(in srgb,var(--rd-purple) 32%,var(--rd-border))}
    .rd-fxp-subback{width:26px;height:26px;border:0;border-radius:6px;background:color-mix(in srgb,var(--rd-purple) 22%,transparent);color:var(--rd-purple);cursor:pointer;display:inline-flex;align-items:center;justify-content:center}.rd-fxp-subback:hover{background:color-mix(in srgb,var(--rd-purple) 38%,transparent)}
    .rd-fxp-subttl{font-size:12.5px;color:var(--sapTextColor,#1d2d3e)}.rd-fxp-subttl b{color:var(--rd-purple);font-family:ui-monospace,Menlo,Consolas,monospace}
    .rd-fxp-subgrid{display:grid;grid-template-columns:1fr 1fr;gap:4px 16px}
    .rd-fxp-prow{display:grid;grid-template-columns:92px minmax(0,1fr);gap:9px;align-items:start;padding:6px 0;border-bottom:1px dashed color-mix(in srgb,var(--rd-border) 70%,transparent)}
    .rd-fxp-plabel{font-size:12px;color:var(--sapTextColor,#1d2d3e);padding-top:7px;font-weight:600}.rd-fxp-plabel .req{color:var(--rd-red,#c9372c);margin-left:2px}
    .rd-fxp-pctl{display:flex;flex-direction:column;gap:4px;min-width:0}
    .rd-fxp-pctl select,.rd-fxp-pctl input{height:30px;border:1px solid var(--rd-border);border-radius:6px;background:var(--sapField_Background,#fff);color:var(--sapTextColor,#1d2d3e);padding:0 9px;font:13px var(--sapFontFamily,Arial);min-width:0}
    .rd-fxp-pctl select:focus,.rd-fxp-pctl input:focus{border-color:var(--rd-purple);outline:0;box-shadow:0 0 0 2px color-mix(in srgb,var(--rd-purple) 22%,transparent)}
    .rd-fxp-phint{font-size:11px;color:var(--sapContent_LabelColor,#8a8d90)}
    .rd-fxp-subout{grid-column:1/-1;display:flex;align-items:center;gap:10px;margin-top:2px}
    .rd-fxp-subout label{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70);flex:0 0 auto}
    .rd-fxp-subout code{flex:1;font:700 12.5px/1.4 ui-monospace,Menlo,Consolas,monospace;color:var(--rd-purple);background:color-mix(in srgb,var(--rd-purple) 14%,var(--sapTile_Background,#fff));border:1px solid color-mix(in srgb,var(--rd-purple) 30%,var(--rd-border));border-radius:7px;padding:6px 9px;word-break:break-all}
    /* 底部动作 */
    .rd-fxp-foot{flex:0 0 auto;display:flex;justify-content:space-between;align-items:center;gap:8px;padding:10px 14px;border-top:1px solid var(--rd-border);background:var(--sapList_HeaderBackground,#f7f9fc)}
    .rd-fxp-tgt{font-size:11.5px;color:var(--sapContent_LabelColor,#6a6d70)}.rd-fxp-tgt b{color:var(--rd-blue);font-family:ui-monospace,Menlo,Consolas,monospace;font-size:12.5px}
    .rd-fxp-btns{display:flex;gap:8px}
    .rd-fxp-empty,.rd-fxp-loading{padding:14px;text-align:center;color:var(--sapContent_LabelColor,#8a8d90);font-size:12px;grid-column:1/-1}
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
    currency: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v16"/><path d="M16 8a4 4 0 0 0-4-2c-2 0-4 1-4 3s2 2.6 4 3 4 1 4 3-2 3-4 3a4 4 0 0 1-4-2"/></svg>',
    percent: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 19 14-14"/><circle cx="7.5" cy="7.5" r="2.5"/><circle cx="16.5" cy="16.5" r="2.5"/></svg>',
    comma: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 8h4M6 12h8M6 16h5"/><path d="M17 15c.5.4.6 1.2.2 1.9l-.9 1.6"/></svg>',
    'decimal-increase': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8v5a2 2 0 0 0 2 2 2 2 0 0 0 2-2V8a2 2 0 0 0-2-2 2 2 0 0 0-2 2z"/><circle cx="11" cy="15" r="1"/><path d="M17 6v8M13 10h8"/></svg>',
    'decimal-decrease': '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8v5a2 2 0 0 0 2 2 2 2 0 0 0 2-2V8a2 2 0 0 0-2-2 2 2 0 0 0-2 2z"/><circle cx="11" cy="15" r="1"/><path d="M13 10h8"/></svg>',
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

/** 常规下拉选择器（字体/字号）：真·可见 <select>，一眼看出可选（替换透明覆盖式 fieldMenu）。 */
function selectField (field, title, options, value, cls = '') {
  const opts = options.map((it) => `<option value="${esc(it.value)}" ${String(value) === String(it.value) ? 'selected' : ''}>${esc(it.label)}</option>`).join('')
  return `<select class="rd-select ${cls} rd-ribbon-item" data-sheet-field="${esc(field)}" title="${esc(title)}" aria-label="${esc(title)}">${opts}</select>`
}

/** 取色调色板（主题/标准色，Excel 常见）：5 行 × 8 列。 */
const COLOR_PALETTE = [
  '#000000', '#404040', '#595959', '#7f7f7f', '#a6a6a6', '#d9d9d9', '#f2f2f2', '#ffffff',
  '#c00000', '#ff0000', '#ffc000', '#ffff00', '#92d050', '#00b050', '#00b0f0', '#0070c0',
  '#002060', '#7030a0', '#e84c3d', '#e67e22', '#f1c40f', '#2ecc71', '#1abc9c', '#3498db',
  '#9b59b6', '#34495e', '#c0392b', '#d35400', '#f39c12', '#27ae60', '#16a085', '#2980b9',
  '#8e44ad', '#1d2d3e', '#e74c3c', '#e59866', '#f7dc6f', '#7dcea0', '#76d7c4', '#85c1e9',
]

/**
 * 颜色下拉工具（前景/背景）：按钮(图标+当前色条+箭头) + 弹出色板(预设网格 + 无填充/自动 + 更多颜色)。
 * 替换原生 <input type=color>（OS 取色框交互隐晦、易丢选区）。交互见 setupColorMenus。
 */
function colorMenuTool (field, icon, title, value, fallback) {
  const swatches = COLOR_PALETTE.map((c) => `<button class="rd-swatch" type="button" data-color="${c}" data-color-field="${esc(field)}" title="${c}" style="background:${c}"></button>`).join('')
  const noneLabel = field === 'fillColor' ? '无填充' : '自动'
  return `<span class="rd-colorwrap rd-ribbon-item" data-rd-colorwrap="${esc(field)}">
    <button class="rd-colorbtn" type="button" data-color-toggle="${esc(field)}" title="${esc(title)}" aria-label="${esc(title)}" aria-haspopup="true" aria-expanded="false" style="--rd-swatch:${esc(value || fallback)}">
      <span class="rd-cb-ic">${toolIcon(icon)}<span class="rd-cb-bar"></span></span><span class="rd-cb-car">${toolIcon('chevron-down')}</span>
    </button>
    <span class="rd-colormenu" data-rd-colormenu="${esc(field)}">
      <button class="rd-cm-none" type="button" data-color-none="${esc(field)}"><span style="width:13px;height:13px;border:1px solid #c3ccd6;border-radius:3px;display:inline-block;position:relative;overflow:hidden"><span style="position:absolute;left:-1px;top:6px;width:19px;height:1.5px;background:#e74c3c;transform:rotate(-45deg)"></span></span>${esc(noneLabel)}</button>
      <div class="rd-cm-sec">主题颜色</div>
      <div class="rd-swatch-grid">${swatches}</div>
      <div class="rd-cm-more"><label>更多颜色…<input type="color" data-color-custom="${esc(field)}" value="${esc(value || fallback)}"></label></div>
    </span>
  </span>`
}

function ribbonGroup (...items) {
  return `<span class="rd-group rd-ribbon-item">${items.join('')}</span>`
}

/** 边框下拉工具（Excel 式）：一个按钮 + 弹出菜单（线位选择：所有/外侧/内部/上下左右/无）。 */
const BORDER_ITEMS = [
  { kind: 'all', label: '所有框线', icon: 'border' },
  { kind: 'outline', label: '外侧框线', icon: 'border-style' },
  { kind: 'inside', label: '内部框线', icon: 'grid' },
  { kind: 'innerHorizontal', label: '内部横线', icon: 'horizontal-grid' },
  { kind: 'innerVertical', label: '内部竖线', icon: 'vertical-grid' },
  { kind: 'top', label: '上框线', icon: 'border-top' },
  { kind: 'bottom', label: '下框线', icon: 'border-bottom' },
  { kind: 'left', label: '左框线', icon: 'border-left' },
  { kind: 'right', label: '右框线', icon: 'border-right' },
  { kind: 'none', label: '无框线', icon: 'decline' },
]
/** 边框线型（Excel 常见）：细/中/粗/虚线/点线/双线。 */
const BORDER_LINES = [
  { style: 'thin', label: '细线' },
  { style: 'medium', label: '中等' },
  { style: 'thick', label: '粗线' },
  { style: 'dashed', label: '虚线' },
  { style: 'dotted', label: '点线' },
  { style: 'double', label: '双线' },
]
/** 边框颜色预设（精简，对齐色板视觉）。 */
const BORDER_COLORS = ['#000000', '#595959', '#8a8f94', '#c00000', '#ff0000', '#ffc000', '#00b050', '#0070c0', '#7030a0', '#ffffff']
function borderMenuTool (st) {
  const curLine = (st && st.borderLineStyle) || 'thin'
  const curColor = (st && st.borderColor) || '#8a8f94'
  const colorRow = BORDER_COLORS.map((c) => `<button class="rd-swatch${c === curColor ? ' on' : ''}" type="button" data-border-color="${c}" title="${c}" style="background:${c}"></button>`).join('')
  const lineRow = BORDER_LINES.map((it) => `<button class="rd-border-line${it.style === curLine ? ' on' : ''}" type="button" data-border-line="${esc(it.style)}" title="${esc(it.label)}">
    ${lineStyleIcon(it.style)}<span>${esc(it.label)}</span>
  </button>`).join('')
  const items = BORDER_ITEMS.map((it) => `<button class="rd-border-item" type="button" data-border-kind="${esc(it.kind)}" title="${esc(it.label)}">
    <i>${borderIcon(it.kind)}</i><span>${esc(it.label)}</span>
  </button>`).join('')
  return `<span class="rd-border" data-rd-border>
    <button class="rd-menu-tool rd-border-btn rd-ribbon-item" type="button" data-border-toggle title="边框" aria-label="边框" aria-haspopup="true" aria-expanded="false">
      <span class="rd-mt-ic">${borderIcon('all')}</span><span class="rd-mt-car">${toolIcon('chevron-down')}</span>
    </button>
    <span class="rd-border-menu" data-rd-border-menu>
      <div class="rd-border-sec">颜色</div>
      <div class="rd-border-colors">${colorRow}</div>
      <div class="rd-border-sec">线型</div>
      <div class="rd-border-lines">${lineRow}</div>
      <div class="rd-border-sec">位置</div>
      ${items}
    </span>
  </span>`
}
/** 线型预览小 SVG：一条对应样式的横线。 */
function lineStyleIcon (style) {
  const col = '#37414f'
  if (style === 'double') return `<svg viewBox="0 0 40 12" aria-hidden="true"><line x1="2" y1="4.5" x2="38" y2="4.5" stroke="${col}" stroke-width="1"/><line x1="2" y1="7.5" x2="38" y2="7.5" stroke="${col}" stroke-width="1"/></svg>`
  const w = style === 'thick' ? 3 : style === 'medium' ? 2 : 1
  const da = style === 'dashed' ? ' stroke-dasharray="5 3"' : style === 'dotted' ? ' stroke-dasharray="1.5 2.5"' : ''
  return `<svg viewBox="0 0 40 12" aria-hidden="true"><line x1="2" y1="6" x2="38" y2="6" stroke="${col}" stroke-width="${w}"${da} stroke-linecap="round"/></svg>`
}
/** 边框位置的迷你 SVG：格子 + 对应边高亮（自绘，无需图标库）。 */
function borderIcon (kind) {
  const on = '#0a6ed1'; const off = '#c3ccd6'
  const t = kind === 'all' || kind === 'outline' || kind === 'top'
  const b = kind === 'all' || kind === 'outline' || kind === 'bottom'
  const l = kind === 'all' || kind === 'outline' || kind === 'left'
  const r = kind === 'all' || kind === 'outline' || kind === 'right'
  if (kind === 'none') {
    return `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="16" fill="none" stroke="${off}" stroke-width="1.4" stroke-dasharray="2 2"/></svg>`
  }
  const ln = (x1, y1, x2, y2, hot) => `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${hot ? on : off}" stroke-width="${hot ? 2.2 : 1.2}" stroke-linecap="round"/>`
  // 内部中线：竖中线(vMid)/横中线(hMid)。inside 两条都亮；innerHorizontal 只亮横中线；innerVertical 只亮竖中线。
  const hMid = kind === 'all' || kind === 'inside' || kind === 'innerHorizontal'
  const vMid = kind === 'all' || kind === 'inside' || kind === 'innerVertical'
  const inner = ln(12, 4, 12, 20, vMid) + ln(4, 12, 20, 12, hMid)
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${ln(4, 4, 20, 4, t)}${ln(4, 20, 20, 20, b)}${ln(4, 4, 4, 20, l)}${ln(20, 4, 20, 20, r)}${inner}</svg>`
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
        selectField('fontFamily', '字体', fontOptions, ui.fontFamily, 'rd-select-font'),
        selectField('fontSize', '字号', sizeOptions, ui.fontSize, 'rd-select-size'),
      )}
      ${ribbonGroup(
        ribbonToggle('bold', 'bold-text', '加粗', ui.bold),
        ribbonToggle('italic', 'italic-text', '斜体', ui.italic),
        ribbonToggle('underline', 'underline-text', '下划线', ui.underline),
        colorMenuTool('fontColor', 'text-color', '字体颜色', ui.fontColor, '#1d2d3e'),
        colorMenuTool('fillColor', 'palette', '填充颜色', ui.fillColor, '#ffffff'),
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
        borderMenuTool(st),
      )}
      ${ribbonGroup(
        ribbonButton('merge-toggle', 'combine', '合并/取消合并'),
      )}
      ${ribbonGroup(
        ribbonButton('fmt-currency', 'currency', '货币格式'),
        ribbonButton('fmt-percent', 'percent', '百分比格式'),
        ribbonButton('fmt-comma', 'comma', '千分位'),
        ribbonButton('fmt-dec-inc', 'decimal-increase', '增加小数位'),
        ribbonButton('fmt-dec-dec', 'decimal-decrease', '减少小数位'),
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
    <span class="rd-hgroup">
      ${historyButton('undo', 'undo', '撤销')}
      ${historyButton('redo', 'redo', '重做')}
    </span>
    <span class="rd-hgroup">
      <button class="rd-hbtn" type="button" data-sheet-cmd="clear-value" title="清除内容" aria-label="清除内容">${toolIcon('eraser')}</button>
      <button class="rd-hbtn" type="button" data-sheet-cmd="clear-format" title="清除格式" aria-label="清除格式">${toolIcon('clear-formatting')}</button>
    </span>
    ${reportSplitButton()}
    <input data-rd-import-file type="file" accept=".xlsx,.xls" hidden>
    <input data-rd-import-json-file type="file" accept=".json,application/json" hidden>
  </span>`
}

/** 顶栏「报表 ▾」split button：主按钮=保存；下拉=保存/导入/导出/导入Excel/导出Excel。 */
function reportSplitButton () {
  const item = (cmd, icon, label) => `<button class="rd-rpt-item" type="button" data-sheet-cmd="${cmd}">${toolIcon(icon)}<span>${label}</span></button>`
  return `<span class="rd-rpt" data-rd-rpt>
    <button class="rd-hbtn primary rd-rpt-main" type="button" data-sheet-cmd="save" title="保存报表设计" aria-label="保存报表">${toolIcon('save')}<span>报表</span></button>
    <button class="rd-hbtn primary rd-rpt-caret" type="button" data-rpt-toggle title="报表操作" aria-label="报表操作" aria-haspopup="true" aria-expanded="false">${toolIcon('chevron-down')}</button>
    <span class="rd-rpt-menu" data-rd-rpt-menu>
      ${item('save', 'save', '保存')}
      <span class="rd-rpt-sep"></span>
      ${item('import-json', 'upload', '导入')}
      ${item('export-json', 'download', '导出')}
      <span class="rd-rpt-sep"></span>
      ${item('import-xlsx', 'upload', '导入 Excel')}
      ${item('export-xlsx', 'download', '导出 Excel')}
    </span>
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
    <div class="rd-fxbar">
      <div class="rd-namebox" title="名称框：输入单元格(如 B4)或区域(如 A1:C5)，回车跳转/选中">
        <input class="rd-namebox-input" data-rd-namebox spellcheck="false" autocomplete="off"
               value="${esc(st.selectedRange || st.selectedCell || 'A1')}" aria-label="名称框">
        <span class="rd-namebox-caret" aria-hidden="true">▾</span>
      </div>
      <span class="rd-fxbar-sep"></span>
      <button class="rd-fx-btn" type="button" data-rd-fxbtn title="插入函数（fx）：进入公式编辑" aria-label="插入函数"><i>fx</i></button>
      <span class="rd-fxbar-sep"></span>
      <input class="rd-fxbar-input" data-rd-fxinput spellcheck="false" autocomplete="off"
             placeholder="输入内容，或以 = 开头输入公式" aria-label="公式输入框" value="">
    </div>
    <div class="rd-sheet-stage"><div class="rd-spread-host"><cmx-spreadjs-sheet class="rd-spread" data-rd-spread data-cmx-formula-bar="false" data-cmx-report="${esc(JSON.stringify(reportModel(st)))}"></cmx-spreadjs-sheet></div></div>
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

/**
 * cellMap 复合键：`sheet名!单元格地址`（如 `利润表!C5`）。
 * st.cellMap 存「属性页维护的元素/取数/校验公式映射」，必须按 sheet 区分——否则各 sheet 同位置
 * 单元格（都叫 C5）会互相串。与后端 ops 的 `sheet!cell` 目标、cr_cell_element_map 的
 * `sheet_code|region_code|cell_ref` 业务键同构。sheetCode 缺省取在屏活动 sheet。
 */
function cellKey (st, addr, sheetCode) {
  const sc = sheetCode || currentSheetCode(st)
  return `${sc}!${String(addr || '').toUpperCase()}`
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
  const cm = st.cellMap[cellKey(st, snap.addr)] || {}
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
  const cm = st.cellMap[cellKey(st, snap.addr)] || {}
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
  const cm = st.cellMap[cellKey(st, snap.addr)] || {}
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
    const key = cellKey(st, addr)
    const cm = st.cellMap[key] = st.cellMap[key] || {}
    cm[el.getAttribute('data-cellmap-field')] = el.value
    markDirty(st, true)
  }))
  root.querySelector('[data-cellmap-save]')?.addEventListener('click', () => {
    markDirty(st, true)
    const addr = st.selectedCell || 'A1'
    emitCellMapOps(st, addr, st.cellMap[cellKey(st, addr)] || {}) // 协同 B 档：公式编辑走 Op 通道即时落库
    toast(root, `已暂存 ${addr} 的公式映射（保存报表时落库）`, 'success')
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
    const sheetName = currentSheetCode(st)
    const cm = st.cellMap[cellKey(st, addr, sheetName)] = st.cellMap[cellKey(st, addr, sheetName)] || {}
    cm[field] = formula
    st.wizard = null
    markDirty(st, true)
    // 协同 B 档：向导插入的公式即时走 Op 通道（field=calcFormula/checkFormula）
    enqueueOp(st, field === 'checkFormula' ? 'setCheckFormula' : 'setCellFormula',
      { sheet: sheetName, cell: addr }, { formula })
    rerender()
    toast(root, `已插入公式到 ${addr}：${formula}`, 'success')
  })

  // —— 元素绑定 / 填入 ——
  root.querySelectorAll('[data-cell-bind],[data-el-bind]').forEach((b) => b.addEventListener('click', () => {
    bindElementToCell(st, root, b.getAttribute('data-cell-bind') || b.getAttribute('data-el-bind'))
  }))
  root.querySelector('[data-cell-unbind]')?.addEventListener('click', () => {
    const addr = st.selectedCell || 'A1'
    const sheetName = currentSheetCode(st)
    const key = cellKey(st, addr, sheetName)
    if (st.cellMap[key]) { delete st.cellMap[key].elementCode; delete st.cellMap[key].valueType; delete st.cellMap[key].dataSource }
    markDirty(st, true)
    // 协同 B 档：解绑即时走 Op 通道
    enqueueOp(st, 'unbindElement', { sheet: sheetName, cell: addr }, {})
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
  const sheetName = currentSheetCode(st)
  const cm = st.cellMap[cellKey(st, addr, sheetName)] = st.cellMap[cellKey(st, addr, sheetName)] || {}
  cm.elementCode = el.code
  cm.valueType = el.data_type || ''
  cm.dataSource = el.value_source || ''
  cm.calcFormula = cm.calcFormula || el.calc_formula || ''
  cm.checkFormula = cm.checkFormula || el.check_formula || ''
  st.selectedElementCode = el.code
  markDirty(st, true)
  // 协同 B 档：绑定即时走 Op 通道
  enqueueOp(st, 'bindElement', { sheet: sheetName, cell: addr }, { elementCode: el.code })
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
  const sheetName = currentSheetCode(st)
  const key = cellKey(st, addr, sheetName)
  const existed = !!st.cellMap[key]?.elementCode
  const cm = st.cellMap[key] = st.cellMap[key] || {}
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
  // 协同 B 档：拖拽绑定即时走 Op 通道
  enqueueOp(st, 'bindElement', { sheet: sheetName, cell: addr }, { elementCode: code })
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

/** 读当前选区的 formatter 字符串（getSelectionState().format，空=常规）。 */
function currentFormatter (sheet, st) {
  try {
    const live = liveSheetOf(st) || sheet
    const s = live?.getSelectionState?.() || {}
    return String(s.format || '')
  } catch (_) { return '' }
}

/** 把 formatter 字符串直接套到选区（旁路 NUMBER_FORMATS 枚举，供货币/百分比/小数位增删等专业操作）。 */
function applyRawFormatter (sheet, st, formatter) {
  const live = liveSheetOf(st) || sheet
  if (!live || typeof live.applySelectionStyle !== 'function') return
  live.applySelectionStyle({ formatter: formatter || '' })
  // 同步 st.sheetUi.format 回显（能对上枚举则用枚举名，否则置 general 占位）
  const hit = Object.entries(NUMBER_FORMATS).find(([, p]) => p === formatter)
  st.sheetUi.format = hit ? hit[0] : (formatter ? 'general' : 'general')
  if (!st.__loading) markDirty(st, true)
  updateToolbarControlsAll(st)
}

/**
 * 在 formatter 上增/减一位小数（step=+1/-1）。纯字符串操作，对齐 Excel「增加/减少小数位」。
 * 无 formatter 时以 '0' 起步；保留千分位/货币/百分号等前后缀。
 */
function stepDecimals (formatter, step) {
  let fmt = String(formatter || '').trim()
  if (!fmt) fmt = '0' // 常规起步：视作整数
  // 已有小数部分（.000…）→ 直接增删其位数。定位「整数末位数字后紧跟的小数段」。
  const decMatch = /\.([0#]+)/.exec(fmt)
  if (decMatch) {
    let zeros = decMatch[1].length
    zeros = Math.max(0, Math.min(9, zeros + step))
    if (zeros === 0) return fmt.replace(/\.[0#]+/, '') // 去掉小数点
    return fmt.replace(/\.[0#]+/, '.' + '0'.repeat(zeros))
  }
  // 无小数部分：增位→在「最后一个整数数字占位符(0/#)」后插入 .0…；减位→无操作（已 0 位）
  if (step <= 0) return fmt
  const zeros = Math.max(1, Math.min(9, step))
  // 在最后一个 0/# 之后插入小数段（保留其后的后缀，如 % 或货币尾符）
  const lastDigit = Math.max(fmt.lastIndexOf('0'), fmt.lastIndexOf('#'))
  if (lastDigit < 0) return fmt
  return fmt.slice(0, lastDigit + 1) + '.' + '0'.repeat(zeros) + fmt.slice(lastDigit + 1)
}

/** 合并/取消合并「二合一」：探测在屏选区有无合并 span → 有则取消、无则合并居中。纯前端，免改组件。 */
function toggleMergeSelection (sheet, st, root) {
  const ws = liveSheetOf(st)?.getWorkbook?.()?.getActiveSheet?.() || sheet?.getWorkbook?.()?.getActiveSheet?.()
  let hasSpan = false
  try {
    const sels = ws?.getSelections?.() || []
    for (const s of sels) {
      const r0 = s.row < 0 ? 0 : s.row
      const c0 = s.col < 0 ? 0 : s.col
      // 选区内任一格落在合并区 → 视为已合并
      const span = ws?.getSpan?.(r0, c0)
      if (span) { hasSpan = true; break }
      // 多格选区：扫描少量格兜底（大选区只看左上足够，合并居中总从左上起）
      const spans = ws?.getSpans?.(ws.getRange(r0, c0, Math.max(1, s.rowCount || 1), Math.max(1, s.colCount || 1)))
      if (spans && spans.length) { hasSpan = true; break }
    }
  } catch (_) {}
  if (hasSpan) sheet.unmergeSelection?.()
  else sheet.mergeSelection?.()
  if (!st.__loading) markDirty(st, true)
  updateToolbarControlsAll(st)
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
  // Excel 公式栏回填：名称框显当前选区、公式框显当前格公式/值。
  // 正在聚焦编辑的字段不覆盖（避免打断用户输入）。
  const focused = deepActiveElement()
  const nb = root.querySelector('[data-rd-namebox]')
  if (nb && nb !== focused) nb.value = st.selectedRange || st.selectedCell || 'A1'
  const fx = root.querySelector('[data-rd-fxinput]')
  if (fx && fx !== focused) fxSyncFromSelection(root, st)
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
    if (el === focused) return // 用户正在操作该下拉，别打断
    if (el.tagName === 'SELECT') {
      el.value = String(ui[field])
      // 数字格式仍是「透明 select + 自定义 label」形态（rd-menu-tool），同步其显示文本
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
  // 颜色按钮的当前色条回显
  root.querySelectorAll('[data-color-toggle]').forEach((btn) => {
    const field = btn.getAttribute('data-color-toggle')
    const val = ui[field] || (field === 'fillColor' ? '#ffffff' : '#1d2d3e')
    btn.style.setProperty('--rd-swatch', val)
    const custom = root.querySelector(`[data-color-custom="${field}"]`)
    if (custom && custom !== focused) { try { custom.value = /^#([0-9a-f]{6})$/i.test(val) ? val : (field === 'fillColor' ? '#ffffff' : '#1d2d3e') } catch (_) {} }
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
        const bind = st.cellMap[`${sheetCode}!${cellRef}`] || null
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
    // ★ 有绑定/公式但单元格本身为空的格子也要产出 cellMap，否则保存后映射丢失：
    //   - 拖拽绑定数据元素不写值（elementCode）
    //   - 属性页只填了取数/校验公式（calcFormula/checkFormula）——空格上定义 QM(...) 是常态
    //   st.cellMap 按 `sheet!cellRef` 复合键存，这里只取属于本 sheet 的键（每个 sheet 都要产出）。
    const sheetPrefix = `${sheetCode}!`
    for (const [mapKey, bind] of Object.entries(st.cellMap || {})) {
      if (!bind || !mapKey.startsWith(sheetPrefix)) continue
      const cellRef = mapKey.slice(sheetPrefix.length)
      if (seenRefs.has(cellRef)) continue
      const hasContent = bind.elementCode || (bind.calcFormula || '').trim() || (bind.checkFormula || '').trim()
      if (!hasContent) continue
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
/** 报表存储层物理表名 → 中文层名（presentDocError 展示 violation 前缀用）。 */
const RPT_TABLE_NAMES = {
  cr_report_fmt: '报表版式',
  cr_report_sheet: '报表页签',
  cr_report_region: '区域',
  cr_report_row: '行',
  cr_report_col: '列',
  cr_cell_element_map: '单元格映射',
  cr_cell_data: '单元格数据',
}

/**
 * 统一展示报表存储错误——对齐 DOC 业务单据机制（专业信息对话框，分 conflict/validation/generic 三态）。
 * 复用门户已导出的 `__cmxDataComp.presentDocError`（无需前端构建）；未加载（老 bundle）时回退 toast。
 * 返回 presentDocError 结果（`{kind,choice}` 或 null），调用方可据 `kind==='conflict'` 重载最新版。
 */
async function presentRptError (root, err, action = 'save') {
  const C = (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}
  if (typeof C.presentDocError === 'function') {
    try {
      return await C.presentDocError(err, { action, tableNames: RPT_TABLE_NAMES })
    } catch { /* 对话框自身异常 → 落到 toast 兜底 */ }
  }
  const msg = String(err?.message || err)
  toast(root, err?.conflict || msg.includes('409') || msg.includes('他人')
    ? '版式已被他人更新，请刷新后重试'
    : `保存失败：${msg}`, 'error')
  return null
}

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
    // 对齐 DOC：专业对话框展示（唯一键冲突/校验/乐观锁），冲突确认后重载最新版。
    const r = await presentRptError(root, err, 'save')
    if (r?.kind === 'conflict') await loadLayout(sheet, st, root)
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

// ============================================================================
// 协同编辑 B 档（方案 docs/报表协同编辑方案-B档.html §5/§8）：
// 属性页/向导的公式编辑 → 语义 Op → 去抖队列 → 批量 POST /ops；打开对齐 seq；轮询追平别人的改动。
// 整簿 saveLayout 仍在（快照物化 + 单人兜底），Op 通道让「同表不同格」的多人编辑互不覆盖。
// ============================================================================

/** 入队一个语义操作（同目标同类型做末值合并——连续键入压成一条），800ms 静默后 flush。 */
function enqueueOp (st, type, target, payload) {
  if (st.__loading) return // 装载/重放回填不产生 Op
  const key = `${type}|${target.sheet || ''}|${target.cell || target.region || target.at || ''}`
  const existed = st.opQueue.find((o) => o.__key === key)
  if (existed) { existed.payload = payload } else {
    st.opClientSerial += 1
    st.opQueue.push({
      __key: key, type, target, payload,
      clientOpId: `${designerSid(st)}-${Date.now().toString(36)}-${st.opClientSerial}`,
    })
  }
  clearTimeout(st.opFlushTimer)
  st.opFlushTimer = setTimeout(() => { flushOps(st).catch(() => {}) }, 800)
}

/** 批量提交队列中的操作；冲突/拒绝结果回传处理。 */
async function flushOps (st) {
  if (!st.opQueue.length) return
  const ops = st.opQueue.splice(0).map((o) => ({
    type: o.type, target: o.target, payload: o.payload,
    baseSeq: st.opSeq, clientOpId: o.clientOpId,
  }))
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/ops`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version: st.props.version || '', ops }),
    })
    if (typeof res?.curSeq === 'number') st.opSeq = res.curSeq
    const conflicts = (res?.results || []).filter((r) => r.conflict)
    if (conflicts.length) {
      // 覆盖了他人改动：last-writer 不阻塞，控制台可回查（op_log 存 prev_value）
      console.warn('[rpt-ops] 覆盖了他人改动', conflicts)
    }
    if ((res?.results || []).some((r) => r.rejected)) {
      // 结构操作被拒：先追平（B 档拒绝-rebase 语义），用户基于新状态重做
      await pollOps(st)
    }
  } catch (err) {
    // 提交失败不丢操作：塞回队列，下次 flush 重试（clientOpId 幂等去重保证不重复应用）
    st.opQueue.unshift(...ops.map((o) => ({ __key: `${o.type}|retry|${o.clientOpId}`, ...o })))
  }
}

/** 追平：拉 seq>opSeq 的别人操作，重放到本地投影态（cellMap），刷新属性页。 */
async function pollOps (st) {
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/ops?version=${enc(st.props.version || '')}&since=${st.opSeq}`)
    const ops = res?.ops || []
    if (typeof res?.curSeq === 'number') st.opSeq = res.curSeq
    if (!ops.length) return
    let touched = false
    for (const op of ops) touched = replayOp(st, op) || touched
    if (touched) refreshInstance(st, (v) => v === 'propertyCell')
  } catch (_) { /* 轮询失败静默，下轮再试 */ }
}

/** 重放一条远端操作到本地状态（公式/绑定类）。返回是否有可见变化。 */
function replayOp (st, op) {
  const t = op.target || ''
  const sheetName = typeof t === 'string' ? (t.split('!')[0] || '') : (t.sheet || '')
  const cell = typeof t === 'string' ? (t.split('!')[1] || '') : (t.cell || '')
  if (!cell) return false
  // 保留 target 里的 sheet，按复合键重放——否则别人在 Sheet2 的改动会串到本地 Sheet1 同位置。
  const key = cellKey(st, cell, sheetName)
  const cm = st.cellMap[key] = st.cellMap[key] || {}
  switch (op.type) {
    case 'setCellFormula': cm.calcFormula = op.payload?.formula || ''; return true
    case 'setCheckFormula': cm.checkFormula = op.payload?.formula || op.payload?.checkFormula || ''; return true
    case 'bindElement': cm.elementCode = op.payload?.elementCode || ''; return true
    case 'unbindElement': delete cm.elementCode; return true
    default: return false
  }
}

/** 打开时对齐 seq（不重放——layout 快照已含全部已应用改动，只取游标）；并启动追平轮询。 */
async function initOpsSync (st) {
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/ops?version=${enc(st.props.version || '')}&since=0&limit=1`)
    if (typeof res?.curSeq === 'number') st.opSeq = res.curSeq
  } catch (_) { /* op_log 表未部署时静默降级为单人模式 */ }
  if (!st.opPollTimer) {
    st.opPollTimer = setInterval(() => { pollOps(st).catch(() => {}) }, 30000)
  }
}

/** 把「暂存映射/向导插入」的公式/绑定编辑接入 Op 通道（属性页动作调用）。 */
function emitCellMapOps (st, addr, cm) {
  const sheet = st.activeSheet || 'Sheet1'
  if (cm.calcFormula !== undefined) {
    enqueueOp(st, 'setCellFormula', { sheet, cell: addr }, { formula: cm.calcFormula || '' })
  }
  if (cm.checkFormula !== undefined) {
    enqueueOp(st, 'setCheckFormula', { sheet, cell: addr }, { formula: cm.checkFormula || '' })
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
    // 按 `sheet!cellRef` 复合键回填，令各 sheet 同位置单元格映射互不覆盖。
    const sc = m.sheet_code || st.activeSheet || 'Sheet1'
    cm[`${sc}!${String(ref).toUpperCase()}`] = {
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
    // 对齐 DOC：数据落库错误同样走专业对话框（唯一键/校验/乐观锁）。
    await presentRptError(root, err, 'save')
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
    case 'export-json':
      exportReportJson(sheet, st)
      break
    case 'import-json':
      root.querySelector('[data-rd-import-json-file]')?.click()
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
    case 'merge-toggle': toggleMergeSelection(sheet, st, root); break
    case 'fmt-currency': applyRawFormatter(sheet, st, '¥#,##0.00'); break
    case 'fmt-percent': applyRawFormatter(sheet, st, '0.00%'); break
    case 'fmt-comma': applyRawFormatter(sheet, st, '#,##0.00'); break
    case 'fmt-dec-inc': applyRawFormatter(sheet, st, stepDecimals(currentFormatter(sheet, st), +1)); break
    case 'fmt-dec-dec': applyRawFormatter(sheet, st, stepDecimals(currentFormatter(sheet, st), -1)); break
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
    'wrap', 'border-all', 'border-outline', 'border-none', 'merge', 'unmerge', 'merge-toggle',
    'insert-row', 'delete-row', 'insert-col', 'delete-col',
    'fmt-currency', 'fmt-percent', 'fmt-comma', 'fmt-dec-inc', 'fmt-dec-dec',
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
  const jfile = root.querySelector('[data-rd-import-json-file]')
  jfile?.addEventListener('change', () => {
    const f = jfile.files?.[0]
    if (!f) return
    const reader = new FileReader()
    reader.onload = () => {
      try {
        const json = JSON.parse(String(reader.result || '{}'))
        if (sheet.setWorkbookJson) { sheet.setWorkbookJson(json); if (!st.__loading) markDirty(st, true); toast(root, '已导入报表版式(JSON)', 'success') }
      } catch (err) { toast(root, `导入失败：${String(err?.message || err)}`, 'error') }
      jfile.value = ''
    }
    reader.onerror = () => { toast(root, '文件读取失败', 'error'); jfile.value = '' }
    reader.readAsText(f)
  })
  bindFormulaBar(root, st)
  syncSheetUiFromSelection(sheet, st)
  updateToolbarControls(root, st)
  setupHistoryMenus(root, st, sheet)
  setupBorderMenu(root, st, sheet)
  setupColorMenus(root, st, sheet)
  setupReportMenu(root, st, sheet)
  setupRibbonOverflow(root)
}

/**
 * Excel 样式公式栏交互（名称框跳转 + 公式输入框提交 + fx 按钮）。每 root 只绑一次。
 * 名称框：输入 B4 / A1:C5 回车 → 跳转/选中在屏 sheet 的对应单元格/区域。
 * 公式框：输入内容或 =公式 回车/失焦 → 复用 applyCellInput 写入当前格（带撤销）；Esc 还原。
 * fx 按钮：聚焦公式框并确保以 = 起头，进入公式编辑（Excel fx 入口的轻量等价）。
 */
function bindFormulaBar (root, st) {
  if (root.__rdFxbarBound) return
  root.__rdFxbarBound = true
  const nb = root.querySelector('[data-rd-namebox]')
  const fx = root.querySelector('[data-rd-fxinput]')
  const fxBtn = root.querySelector('[data-rd-fxbtn]')

  // —— 名称框：回车/失焦跳转 ——
  const gotoFromNamebox = () => {
    const v = String(nb?.value || '').trim()
    if (!gotoCellOrRange(st, v)) {
      // 非法输入 → 还原为当前选区
      if (nb) nb.value = st.selectedRange || st.selectedCell || 'A1'
      toast(root, '无效的单元格/区域地址（示例：B4 或 A1:C5）', 'error')
      return
    }
    // 跳转成功后 onSelect 轮询会回填名称框/公式框；这里让焦点离开便于继续操作
    nb?.blur()
  }
  nb?.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); gotoFromNamebox() }
    else if (ev.key === 'Escape') { ev.preventDefault(); nb.value = st.selectedRange || st.selectedCell || 'A1'; nb.blur() }
  })
  nb?.addEventListener('focus', () => { try { nb.select() } catch {} })
  nb?.addEventListener('change', gotoFromNamebox)

  // —— 公式输入框：回车/失焦提交，Esc 还原 ——
  const submitFx = () => {
    const v = String(fx?.value ?? '')
    applyCellInput(st, root, /^\s*=/.test(v) ? 'formula' : 'value', v)
  }
  fx?.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); submitFx(); fx.blur() }
    else if (ev.key === 'Escape') { ev.preventDefault(); fxSyncFromSelection(root, st); fx.blur() }
  })
  fx?.addEventListener('change', submitFx)

  // —— fx 按钮：打开 Excel 样式函数定义面板 ——
  fxBtn?.addEventListener('click', () => openFxPanel(root, st))
}

/** 跳转/选中在屏 sheet 的单元格或区域（B4 / A1:C5）。成功返回 true。 */
function gotoCellOrRange (st, addr) {
  const box = expandRange(addr) // 单格与区域统一 → {r1,c1,r2,c2}
  if (!box) return false
  const live = liveSheetOf(st)
  const ws = live?.getWorkbook?.()?.getActiveSheet?.()
  if (!ws) return false
  const rows = box.r2 - box.r1 + 1
  const cols = box.c2 - box.c1 + 1
  try {
    if (typeof ws.setActiveCell === 'function') ws.setActiveCell(box.r1, box.c1)
    if (typeof ws.setSelection === 'function') ws.setSelection(box.r1, box.c1, rows, cols)
    // 滚动到可见（showCell 位置参数：3=左上，1=居中；用字面量避免依赖组件私有的 GC 全局）
    if (typeof ws.showCell === 'function') {
      try { ws.showCell(box.r1, box.c1, 3, 3) } catch { try { ws.showCell(box.r1, box.c1) } catch {} }
    }
  } catch { return false }
  // 立即回填本地状态（轮询也会跟上）
  st.selectedCell = `${indexToCol(box.c1)}${box.r1 + 1}`
  st.selectedRange = rows === 1 && cols === 1
    ? st.selectedCell
    : `${indexToCol(box.c1)}${box.r1 + 1}:${indexToCol(box.c2)}${box.r2 + 1}`
  updateToolbarControlsAll(st)
  updateFxPanelTarget(st) // fx 面板打开时目标格跟随（名称框跳转路径）
  return true
}

/** 用当前选中格的公式/值刷新公式输入框（不动名称框）。 */
function fxSyncFromSelection (root, st) {
  const fx = root.querySelector('[data-rd-fxinput]')
  if (!fx) return
  const ws = liveSheetOf(st)?.getWorkbook?.()?.getActiveSheet?.()
  const p = parseA1(st.selectedCell || 'A1')
  if (!ws || !p) { fx.value = ''; return }
  let formula = null
  try { formula = ws.getFormula ? ws.getFormula(p.row, p.col) : null } catch {}
  if (formula) { fx.value = `=${formula}`; return }
  let val = ''
  try { val = ws.getValue ? ws.getValue(p.row, p.col) : '' } catch {}
  fx.value = val == null ? '' : String(val)
}

// ============================================================================
// Excel 样式函数定义面板（fx 按钮弹出）——独立于属性页旧向导(st.wizard)，互不影响。
// 自管理 overlay：只重绘自己的 innerHTML，绝不走 refreshInstance（那会重挂 SpreadJS）。
// ============================================================================

/** fx 面板参数控件（按 kind 渲染；复刻旧 wizardControl 的 kind→控件映射，用面板自己的 class/attr）。 */
function fxpControl (st, p, i, val) {
  const A = `data-fxp-arg="${i}"`
  if (p.kind === 'period') {
    const opts = [['0', '本期(0)'], ['-1', '上期(-1)'], ['-2', '上两期(-2)'], ['-12', '上年同期(-12)']]
    const isAbs = !opts.some(([v]) => v === String(val)) && !!val
    const list = opts.map(([v, l]) => `<option value="${v}" ${String(val) === v ? 'selected' : ''}>${l}</option>`).join('')
    return `<select ${A}>${list}<option value="__abs" ${isAbs ? 'selected' : ''}>绝对期间…</option></select>
      <input ${A}-abs placeholder="或输入 2026-06" value="${esc(isAbs ? val : '')}" class="rd-fxp-abs">`
  }
  if (p.kind === 'org') {
    const isCode = val && val[0] !== '@'
    return `<select ${A}><option value="@current" ${val === '@current' || !val ? 'selected' : ''}>@当前组织</option>
      <option value="@parent" ${val === '@parent' ? 'selected' : ''}>@上级组织</option>
      <option value="__code" ${isCode ? 'selected' : ''}>指定组织码…</option></select>
      <input ${A}-code placeholder="组织码" value="${esc(isCode ? val : '')}" class="rd-fxp-abs">`
  }
  if (p.kind === 'object') {
    const els = (st.elements || []).slice(0, 300)
    const opts = els.map((e) => `<option value="${esc(e.code)}">${esc(e.code)} ${esc(e.name || '')}</option>`).join('')
    return `<input ${A} list="rd-fxp-obj-${i}" placeholder="科目码/元素码" value="${esc(val)}">
      <datalist id="rd-fxp-obj-${i}">${opts}</datalist>`
  }
  if (p.kind === 'direction') {
    return `<select ${A}><option value="net" ${val === 'net' || !val ? 'selected' : ''}>净额</option>
      <option value="debit" ${val === 'debit' ? 'selected' : ''}>借方</option>
      <option value="credit" ${val === 'credit' ? 'selected' : ''}>贷方</option></select>`
  }
  return `<input ${A} placeholder="${esc(p.hint || '')}" value="${esc(val)}">`
}

/** 内置(可拼)函数：SpreadJS 原生 + formula-eval 都支持，点击插入 NAME() 并把光标停括号内。 */
const FX_BUILTIN = ['SUM', 'IF', 'ABS', 'MAX', 'MIN', 'ROUND']
/** 取数函数（需参数，点击展开逐参子面板）。 */
const FX_FETCH = ['QM', 'QC', 'JE', 'FS', 'REF']
/** 运算符/括号快捷插入。 */
const FX_OPS = [
  { t: '+', ins: '+' }, { t: '−', ins: '-' }, { t: '×', ins: '*' }, { t: '÷', ins: '/' },
  { t: '( )', ins: '()', inside: true }, { t: ',', ins: ',' },
  { t: '>', ins: '>' }, { t: '<', ins: '<' }, { t: '=', ins: '=' },
]

/** 面板 HTML：非模态浮层 = 标题(可拖) + 公式编辑区 + 分区函数/运算符 + 取数参数子面板 + 底部动作。 */
function fxPanelHtml (st) {
  const fp = st.fxPanel
  if (!fp) return ''
  const head = `<div class="rd-fxp-head" data-fxp-drag>
      <b><span class="rd-fxp-badge">fx</span> 函数 / 公式编辑器</b>
      <button class="rd-fxp-x" type="button" data-fxp-close aria-label="关闭"><ui5-icon name="decline"></ui5-icon></button>
    </div>`
  const editor = `<div class="rd-fxp-editrow">
      <span class="rd-fxp-eq">=</span>
      <textarea class="rd-fxp-expr" data-fxp-expr spellcheck="false" placeholder="点下方函数/运算符插入，或直接键入表达式，如 FS(0,@current,'1001')+SUM(D3:D7)">${esc(fp.expr || '')}</textarea>
    </div>`
  const body = fp.sub ? fxSubPanelHtml(st, fp.sub) : fxPaletteHtml(st)
  return `<div class="rd-fxp-panel" role="dialog">
    ${head}
    ${editor}
    ${body}
    <div class="rd-fxp-foot">
      <span class="rd-fxp-tgt">写入到 <b>${esc(fp.target || '')}</b></span>
      <div class="rd-fxp-btns">
        <button class="rd-sbtn" type="button" data-fxp-close><ui5-icon name="decline"></ui5-icon>取消</button>
        <button class="rd-sbtn primary" type="button" data-fxp-insert><ui5-icon name="accept"></ui5-icon>写入单元格</button>
      </div>
    </div>
  </div>`
}

/** 调色板区：搜索 + 运算符区 + 内置函数区 + 取数函数区（三区彩色分明）。 */
function fxPaletteHtml (st) {
  const fp = st.fxPanel
  const q = String(fp.search || '').trim().toLowerCase()
  const opBtns = FX_OPS.map((o) => `<button class="rd-fx-op" type="button" data-fx-op="${esc(o.ins)}" data-fx-inside="${o.inside ? '1' : ''}" title="${esc(o.t)}">${esc(o.t)}</button>`).join('')
  const fnMeta = (name) => (st.functions || []).find((f) => f.name === name) || { name, help: '' }
  const match = (name) => { if (!q) return true; const m = fnMeta(name); return name.toLowerCase().includes(q) || String(m.help || '').toLowerCase().includes(q) }
  const builtinBtns = FX_BUILTIN.filter(match).map((name) => { const m = fnMeta(name); return `<button class="rd-fx-fn rd-fx-fn-builtin" type="button" data-fx-builtin="${esc(name)}" title="${esc(m.help || name)}"><span class="rd-fx-fnname">${esc(name)}</span><span class="rd-fx-fnhelp">${esc(m.help || '')}</span></button>` }).join('') || '<div class="rd-fxp-empty">无匹配</div>'
  const fetchBtns = FX_FETCH.filter(match).map((name) => { const m = fnMeta(name); return `<button class="rd-fx-fn rd-fx-fn-fetch" type="button" data-fx-fetch="${esc(name)}" title="${esc(m.help || name)}"><span class="rd-fx-fnname">${esc(name)}</span><span class="rd-fx-fnhelp">${esc(m.help || '')}</span></button>` }).join('') || '<div class="rd-fxp-empty">无匹配</div>'
  const cellBtn = `<button class="rd-fx-op rd-fx-op-cell" type="button" data-fx-cell title="插入当前选中单元格地址">插入单元格 ${esc(st.selectedCell || 'A1')}</button>`
  return `<div class="rd-fxp-pal">
      <div class="rd-fxp-search"><ui5-icon name="search"></ui5-icon><input data-fxp-search placeholder="搜索函数…" value="${esc(fp.search || '')}"></div>
      <div class="rd-fxp-zone rd-fxp-zone-op">
        <div class="rd-fxp-zt">运算符 · 括号</div>
        <div class="rd-fxp-ops">${opBtns}${cellBtn}</div>
      </div>
      <div class="rd-fxp-zone rd-fxp-zone-builtin">
        <div class="rd-fxp-zt">内置函数</div>
        <div class="rd-fxp-fns">${builtinBtns}</div>
      </div>
      <div class="rd-fxp-zone rd-fxp-zone-fetch">
        <div class="rd-fxp-zt">取数函数（点开填参数）</div>
        <div class="rd-fxp-fns">${fetchBtns}</div>
      </div>
    </div>`
}

/** 取数函数参数子面板：逐参控件（复用 fxpControl）+ 实时函数串 + 插入/返回。 */
function fxSubPanelHtml (st, sub) {
  const fn = sub.fn
  const params = wizardParamList(fn)
  const rows = params.map((p, i) => {
    const val = sub.args[i] != null ? sub.args[i] : (p.default || '')
    return `<div class="rd-fxp-prow">
      <div class="rd-fxp-plabel">${esc(p.name)}${p.required ? '<span class="req">*</span>' : ''}</div>
      <div class="rd-fxp-pctl">${fxpControl(st, p, i, val)}<div class="rd-fxp-phint">${esc(p.hint || '')}</div></div>
    </div>`
  }).join('') || '<div class="rd-fxp-phint">该函数无固定参数</div>'
  const built = buildFormula(fn, sub.args)
  return `<div class="rd-fxp-sub">
      <div class="rd-fxp-subhead"><button class="rd-fxp-subback" type="button" data-fxp-subback><ui5-icon name="nav-back"></ui5-icon></button>
        <span class="rd-fxp-subttl"><b>${esc(fn.name)}</b> · ${esc(fn.help || '')}</span></div>
      <div class="rd-fxp-subgrid">${rows}</div>
      <div class="rd-fxp-subout"><label>函数</label><code data-fxp-subout>${esc(built || fn.name + '()')}</code>
        <button class="rd-sbtn primary" type="button" data-fxp-subinsert><ui5-icon name="add"></ui5-icon>插入到表达式</button></div>
    </div>`
}

/** 打开面板：建 overlay + 载函数目录 + 渲染（非模态浮层）。 */
function openFxPanel (root, st) {
  const initAddr = st.selectedCell || 'A1'
  st.fxPanel = { open: true, expr: '', target: initAddr, search: '', sub: null, pos: null, caret: null, edited: false }
  st.fxPanel.expr = readCellExpr(st, initAddr) // 打开即回显当前格已有公式（双向联动）
  let host = root.querySelector('.rd-fxp-mask')
  if (!host) { host = document.createElement('div'); host.className = 'rd-fxp-mask'; root.appendChild(host) }
  renderFxPanel(root, st)
  loadFunctions(st).then(() => { if (st.fxPanel) renderFxPanel(root, st) })
}

/** 关闭面板：移除 overlay + 清状态。 */
function closeFxPanel (root, st) {
  st.fxPanel = null
  const host = root.querySelector('.rd-fxp-mask')
  if (host) host.remove()
}

/** 只重绘 overlay 的 innerHTML + 重绑（不碰 content root 其余部分）。 */
function renderFxPanel (root, st) {
  const host = root.querySelector('.rd-fxp-mask')
  if (!host || !st.fxPanel) return
  host.innerHTML = fxPanelHtml(st)
  const panel = host.querySelector('.rd-fxp-panel')
  if (panel) fxPlacePanel(st, panel)
  bindFxPanel(root, st)
}

/**
 * fx 面板目标格跟随：面板打开时把 st.fxPanel.target 同步为当前选中格，并原地更新底部「写入到 X」文本。
 * 不重渲染面板（那会清空公式框内容与焦点）——只改一处 <b>。跨所有 content 宿主找在屏面板。
 */
/**
 * 读某单元格已存的报表表达式（跨 sheet）。权威源 = st.cellMap[sheet!cell].calcFormula（原始 DSL 裸串，
 * 含 @current/单引号），无则回退画布原生公式 getFormula（sanitized 双引号版）。空格返回空串。
 */
function readCellExpr (st, addr) {
  const sheetName = currentSheetCode(st) // 当前在屏活动 sheet；切 sheet 后自然取对表映射
  const cm = st.cellMap && st.cellMap[cellKey(st, addr, sheetName)]
  if (cm && cm.calcFormula) return String(cm.calcFormula).replace(/^=+/, '')
  const ws = liveSheetOf(st)?.getWorkbook?.()?.getActiveSheet?.()
  const p = parseA1(addr)
  if (ws && p) { try { const fx = ws.getFormula(p.row, p.col); if (fx) return String(fx).replace(/^=+/, '') } catch {} }
  return ''
}

/**
 * fx 面板目标格跟随（打开时）：①同步 target + 原地改「写入到 X」文本 + 「插入单元格 X」按钮文本；
 * ②双向回显——未手动编辑过（fp.edited=false）且公式框未聚焦时，把选中格已有公式载入公式框（跨 sheet；空格清空）。
 * 全程不重渲染面板（否则丢公式与焦点）。
 */
function updateFxPanelTarget (st) {
  const fp = st.fxPanel
  if (!fp) return
  const addr = st.selectedCell || 'A1'
  const sameCell = fp.target === addr
  fp.target = addr
  // 载入该格已存公式（未编辑才载、避免覆盖用户手打）
  let loadExpr = null
  if (!fp.edited) {
    loadExpr = readCellExpr(st, addr)
    fp.expr = loadExpr
  }
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected || host.__rptDesignerNativeView !== 'content') continue
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    const mask = root?.querySelector?.('.rd-fxp-mask')
    if (!mask) continue
    const b = mask.querySelector('.rd-fxp-tgt b')
    if (b) b.textContent = addr
    const cellBtn = mask.querySelector('[data-fx-cell]')
    if (cellBtn) cellBtn.textContent = `插入单元格 ${addr}`
    if (loadExpr != null) {
      const ta = mask.querySelector('[data-fxp-expr]')
      if (ta && deepActiveElement() !== ta) { ta.value = loadExpr; fp.edited = false } // 载入不算用户编辑
    }
  }
}

/** 浮层定位：优先用 st.fxPanel.pos（拖过），否则首次停在内容区右上/选中格附近。 */
function fxPlacePanel (st, panel) {
  const fp = st.fxPanel
  if (fp.pos) { panel.style.left = fp.pos.left + 'px'; panel.style.top = fp.pos.top + 'px'; return }
  // 首次：量取尺寸，停在视口右上偏中
  panel.style.visibility = 'hidden'
  const pr = panel.getBoundingClientRect()
  const pw = pr.width || 520
  let left = Math.max(12, window.innerWidth - pw - 40)
  let top = 130
  panel.style.left = left + 'px'
  panel.style.top = top + 'px'
  panel.style.visibility = ''
  fp.pos = { left, top }
}

/** 在 expr textarea 光标处插入文本；inside=true 则把光标停到刚插入的 () 内。 */
function fxInsertAtCursor (root, st, text, inside) {
  const ta = root.querySelector('.rd-fxp-mask [data-fxp-expr]')
  const cur = String(st.fxPanel.expr || '')
  if (!ta) { st.fxPanel.expr = cur + text; renderFxPanel(root, st); return }
  // 光标位置：优先 textarea 当前（若聚焦），否则用记录的 caret，再否则末尾。
  // 点按钮会让 textarea 失焦，selectionStart 可能回 0，故以 st.fxPanel.caret 为准。
  const focused = (root.getRootNode?.().activeElement === ta) || (document.activeElement === ta)
  let s, e
  if (focused && ta.selectionStart != null) { s = ta.selectionStart; e = ta.selectionEnd }
  else if (typeof st.fxPanel.caret === 'number') { s = e = Math.min(st.fxPanel.caret, cur.length) }
  else { s = e = cur.length }
  const before = cur.slice(0, s)
  const after = cur.slice(e)
  const next = before + text + after
  st.fxPanel.expr = next
  st.fxPanel.edited = true // 运算符/函数插入=用户主动构建，标记已编辑（防切格覆盖）
  ta.value = next
  // 光标：inside(如 SUM()/())停到左右括号中间；否则停到插入串末尾
  let caret = s + text.length
  if (inside) { const open = text.indexOf('('); if (open >= 0) caret = s + open + 1 }
  st.fxPanel.caret = caret
  ta.focus()
  try { ta.setSelectionRange(caret, caret) } catch {}
}

/** 面板交互（每次 renderFxPanel 后重绑，只在 overlay 内 query）。非模态：不点外部关。 */
function bindFxPanel (root, st) {
  const host = root.querySelector('.rd-fxp-mask')
  if (!host || !st.fxPanel) return
  const fp = st.fxPanel
  const panel = host.querySelector('.rd-fxp-panel')
  // 关闭：× / 取消 / Esc（焦点在面板内时）
  host.querySelectorAll('[data-fxp-close]').forEach((b) => b.addEventListener('click', () => closeFxPanel(root, st)))
  panel?.addEventListener('keydown', (ev) => { if (ev.key === 'Escape') { ev.preventDefault(); closeFxPanel(root, st) } })

  // 拖动（标题栏）
  const handle = host.querySelector('[data-fxp-drag]')
  if (handle && panel) {
    handle.addEventListener('mousedown', (ev) => {
      if (ev.target.closest('[data-fxp-close]')) return
      ev.preventDefault()
      const pr = panel.getBoundingClientRect()
      const ox = ev.clientX - pr.left; const oy = ev.clientY - pr.top
      document.body.style.userSelect = 'none'
      const move = (e) => {
        let left = e.clientX - ox; let top = e.clientY - oy
        left = Math.max(4, Math.min(left, window.innerWidth - 80))
        top = Math.max(4, Math.min(top, window.innerHeight - 40))
        panel.style.left = left + 'px'; panel.style.top = top + 'px'
        fp.pos = { left, top }
      }
      const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); document.body.style.userSelect = '' }
      document.addEventListener('mousemove', move); document.addEventListener('mouseup', up)
    })
  }

  // 公式 textarea：编辑同步 expr + 记光标
  const ta = host.querySelector('[data-fxp-expr]')
  ta?.addEventListener('input', () => { fp.expr = ta.value; fp.caret = ta.selectionStart; fp.edited = true })
  ta?.addEventListener('keyup', () => { fp.caret = ta.selectionStart })
  ta?.addEventListener('click', () => { fp.caret = ta.selectionStart })
  // 失焦（点按钮前）记住光标，供 fxInsertAtCursor 在 textarea 失焦时定位
  ta?.addEventListener('blur', () => { if (ta.selectionStart != null) fp.caret = ta.selectionStart })

  // 写入单元格
  host.querySelector('[data-fxp-insert]')?.addEventListener('click', () => insertFxFormula(root, st))

  if (fp.sub) {
    // 参数子面板
    host.querySelector('[data-fxp-subback]')?.addEventListener('click', () => { fp.sub = null; renderFxPanel(root, st) })
    host.querySelector('[data-fxp-subinsert]')?.addEventListener('click', () => {
      const built = buildFormula(fp.sub.fn, fp.sub.args)
      fp.sub = null
      renderFxPanel(root, st) // 回主编辑区
      fxInsertAtCursor(root, st, built)
    })
    host.querySelectorAll('[data-fxp-arg]').forEach((el) => el.addEventListener('input', () => {
      const i = Number(el.getAttribute('data-fxp-arg'))
      let v = el.value
      if (v === '__abs' || v === '__code') { const sib = el.parentElement.querySelector('.rd-fxp-abs'); v = sib ? sib.value : '' } else if (el.classList.contains('rd-fxp-abs')) {
        const sel = el.parentElement.querySelector('[data-fxp-arg]')
        if (sel && (sel.value === '__abs' || sel.value === '__code')) { fp.sub.args[i] = v; refreshFxSubOut(root, st); return }
      }
      fp.sub.args[i] = v
      refreshFxSubOut(root, st)
    }))
    return
  }

  // 调色板：搜索
  const search = host.querySelector('[data-fxp-search]')
  if (search) {
    search.addEventListener('input', () => {
      fp.search = search.value
      renderFxPanel(root, st)
      requestAnimationFrame(() => { const nx = root.querySelector('.rd-fxp-mask [data-fxp-search]'); if (nx) { nx.focus(); const n = nx.value.length; try { nx.setSelectionRange(n, n) } catch {} } })
    })
  }
  // 运算符/括号
  host.querySelectorAll('[data-fx-op]').forEach((b) => b.addEventListener('click', () => {
    fxInsertAtCursor(root, st, b.getAttribute('data-fx-op') || '', b.getAttribute('data-fx-inside') === '1')
  }))
  // 插入单元格地址
  host.querySelector('[data-fx-cell]')?.addEventListener('click', () => fxInsertAtCursor(root, st, st.selectedCell || 'A1'))
  // 内置函数：插入 NAME() 光标停括号内
  host.querySelectorAll('[data-fx-builtin]').forEach((b) => b.addEventListener('click', () => {
    fxInsertAtCursor(root, st, `${b.getAttribute('data-fx-builtin')}()`, true)
  }))
  // 取数函数：展开参数子面板
  host.querySelectorAll('[data-fx-fetch]').forEach((b) => b.addEventListener('click', () => {
    const fn = (st.functions || []).find((f) => f.name === b.getAttribute('data-fx-fetch'))
    if (!fn) { fxInsertAtCursor(root, st, `${b.getAttribute('data-fx-fetch')}()`, true); return }
    fp.sub = { fn, args: wizardParamList(fn).map((p) => p.default || '') }
    renderFxPanel(root, st)
  }))
}

/** 只更新子面板的函数串预览。 */
function refreshFxSubOut (root, st) {
  if (!st.fxPanel || !st.fxPanel.sub) return
  const out = root.querySelector('.rd-fxp-mask [data-fxp-subout]')
  if (out) { const f = buildFormula(st.fxPanel.sub.fn, st.fxPanel.sub.args); out.textContent = f || (st.fxPanel.sub.fn.name + '()') }
}

/** 插入：写画布单元格(带撤销) + 写报表取数层 cellMap.calcFormula + 协同 Op。 */
/**
 * 把报表 DSL 公式转成 SpreadJS **语法安全**的等价串（仅供画布 setFormula 用）。
 * 取数自定义函数(QM/FS/…)按单元格位置取值、**不解析参数**，故参数只需语法合法即可：
 * 把 @current/@parent、绝对期间(2026-06)、组织码等**裸标识**转成字符串字面量(双引号)，
 * 避免 SpreadJS 把 @current 当无效名、2026-06 当减法而拒绝整条公式。
 * 已带引号的('...')、纯数字、单元格引用(A1)、区间(A1:B2)保持原样。
 */
function toSpreadjsFormula (fn, args) {
  const params = wizardParamList(fn)
  const parts = []
  for (let i = 0; i < params.length; i++) {
    const p = params[i]
    let v = args[i]
    if (v == null || v === '') { if (p.required && p.kind !== 'expr') v = p.default || ''; else continue }
    if (v == null || v === '') continue
    const s = String(v)
    if (p.kind === 'number' && /^-?\d+(?:\.\d+)?$/.test(s)) { parts.push(s); continue }
    if (p.kind === 'period' && /^-?\d+$/.test(s)) { parts.push(s); continue } // 相对期间整数
    if (p.kind === 'cellref' || p.kind === 'expr') { parts.push(s); continue } // 单元格/子表达式原样
    // 其余(org/object/direction/report/version/绝对期间/文本)→ 双引号字符串字面量
    parts.push('"' + s.replace(/"/g, '') + '"')
  }
  while (parts.length && (parts[parts.length - 1] === '' || parts[parts.length - 1] == null)) parts.pop()
  return `${fn.name}(${parts.join(',')})`
}

function insertFxFormula (root, st) {
  const fp = st.fxPanel
  if (!fp) return
  const expr = String(fp.expr || '').trim().replace(/^=+/, '') // 用户拼的整条表达式，去前导 =
  if (!expr) { toast(root, '表达式为空', 'error'); return }
  const addr = fp.target || st.selectedCell || 'A1'
  const sheetName = currentSheetCode(st)
  // 目标格可能与当前选中不同 → 先跳过去，令后续写入命中对的格
  if (addr !== st.selectedCell) { gotoCellOrRange(st, addr); st.selectedCell = addr }
  // ① 画布单元格：整条表达式（含取数函数 QM/FS/… 已注册为 SpreadJS 自定义函数 + 原生 SUM/IF/… + 运算符）。
  //    报表 DSL 的 @current/绝对期间等裸标识 SpreadJS 解析不了，用 sanitizeExprForSpreadjs 把它们转字符串字面量。
  const canvasExpr = sanitizeExprForSpreadjs(expr)
  applyCellInput(st, root, 'formula', '=' + canvasExpr)
  // ② 报表取数层 + 协同 Op（calcFormula 存原始表达式裸串，供后端/前端 formula-eval 解析）
  const key = cellKey(st, addr, sheetName)
  const cm = st.cellMap[key] = st.cellMap[key] || {}
  cm.calcFormula = expr
  markDirty(st, true)
  enqueueOp(st, 'setCellFormula', { sheet: sheetName, cell: addr }, { formula: expr })
  closeFxPanel(root, st)
  updateToolbarControls(root, st)
  toast(root, `已写入 ${addr}：=${expr}`, 'success')
}

/**
 * 把整条报表表达式转成 SpreadJS 语法安全版：@current/@parent → "@current"、绝对期间 2026-06 → "2026-06"。
 * 只处理这两类裸标识，不动函数名/数字/单元格引用/运算符/已带引号串。函数按格取值不解析参数，仅需语法合法。
 */
function sanitizeExprForSpreadjs (expr) {
  let s = String(expr || '')
  // 单引号字符串字面量 '...' → 双引号 "..."（SpreadJS 公式的字符串必须双引号；单引号是 sheet 名语义）。
  s = s.replace(/'([^']*)'/g, (mm, inner) => `"${inner.replace(/"/g, '')}"`)
  // @current / @parent / @root / @self 等 @ 记号（未被引号包裹）→ 双引号字符串
  s = s.replace(/@[a-zA-Z]+/g, (m) => `"${m}"`)
  // 绝对期间 YYYY-MM（未被引号包裹的）→ 双引号；简单处理：孤立的 4 位-2 位
  s = s.replace(/(^|[(,\s])(\d{4}-\d{2})(?=[),\s]|$)/g, (mm, pre, ym) => `${pre}"${ym}"`)
  return s
}

/** 边框下拉：点按钮开/关菜单，点菜单项应用对应线位。 */
function setupBorderMenu (root, st, sheet) {
  if (root.__rdBorderBound) return
  root.__rdBorderBound = true
  const wrap = root.querySelector('[data-rd-border]')
  const toggle = root.querySelector('[data-border-toggle]')
  const menu = root.querySelector('[data-rd-border-menu]')
  const close = () => { wrap?.classList.remove('open'); toggle?.setAttribute('aria-expanded', 'false') }
  // 菜单用 position:fixed，按钮下方定位（逃逸 .rd-ribbon-main 的 overflow:hidden 裁剪）。
  const place = () => {
    if (!toggle || !menu) return
    const b = toggle.getBoundingClientRect()
    // 先显示以量取真实尺寸（离屏避免闪烁），再定位
    menu.style.visibility = 'hidden'
    const mr = menu.getBoundingClientRect()
    const mw = mr.width || 190
    const mh = mr.height || 300
    let left = b.left
    if (left + mw > window.innerWidth - 8) left = window.innerWidth - mw - 8
    let top = b.bottom + 4
    if (top + mh > window.innerHeight - 8) top = Math.max(8, b.top - mh - 4) // 下方放不下则向上翻
    menu.style.left = Math.max(8, left) + 'px'
    menu.style.top = top + 'px'
    menu.style.visibility = ''
  }
  toggle?.addEventListener('click', (ev) => {
    ev.stopPropagation()
    const willOpen = !wrap?.classList.contains('open')
    close()
    if (willOpen && wrap) { wrap.classList.add('open'); toggle.setAttribute('aria-expanded', 'true'); place() }
  })
  root.addEventListener('click', (ev) => {
    // 颜色选择：更新当前边框色 + 高亮，不关菜单
    const colorBtn = ev.target.closest?.('[data-border-color]')
    if (colorBtn) {
      ev.stopPropagation()
      st.borderColor = colorBtn.getAttribute('data-border-color') || '#8a8f94'
      menu?.querySelectorAll('[data-border-color]').forEach((b) => b.classList.toggle('on', b === colorBtn))
      return
    }
    // 线型选择：更新当前线型 + 高亮，不关菜单
    const lineBtn = ev.target.closest?.('[data-border-line]')
    if (lineBtn) {
      ev.stopPropagation()
      st.borderLineStyle = lineBtn.getAttribute('data-border-line') || 'thin'
      menu?.querySelectorAll('[data-border-line]').forEach((b) => b.classList.toggle('on', b === lineBtn))
      return
    }
    // 位置选择：按当前颜色 + 线型套用，然后关菜单
    const item = ev.target.closest?.('[data-border-kind]')
    if (!item) return
    const kind = item.getAttribute('data-border-kind') || 'all'
    const live = liveSheetOf(st) || sheet
    if (!live) { toast(root, '请先在设计区打开电子表格', 'error'); return }
    live.applySelectionBorder?.(kind, st.borderColor || '#8a8f94', st.borderLineStyle || 'thin')
    if (!st.__loading) markDirty(st, true)
    close()
  })
  document.addEventListener('click', close)
  window.addEventListener('resize', close)
}

/**
 * 颜色下拉（前景/背景）交互——照抄 setupBorderMenu 的 fixed 弹层范式。
 * 按钮 toggle + place() 逃逸 ribbon overflow；点色块/无填充/自定义色 → applySheetUiStyle 落色 + 关闭。
 * 弹层是纯 DOM 层，不碰 SpreadJS 焦点/选区（规避原生 <input type=color> 丢选区的老问题）。
 */
function setupColorMenus (root, st, sheet) {
  if (root.__rdColorBound) return
  root.__rdColorBound = true
  const wraps = Array.from(root.querySelectorAll('[data-rd-colorwrap]'))
  const closeAll = () => wraps.forEach((w) => {
    w.classList.remove('open')
    w.querySelector('[data-color-toggle]')?.setAttribute('aria-expanded', 'false')
  })
  const place = (toggle, menu) => {
    if (!toggle || !menu) return
    const b = toggle.getBoundingClientRect()
    menu.style.visibility = 'hidden'
    const mr = menu.getBoundingClientRect()
    const mw = mr.width || 206
    const mh = mr.height || 220
    let left = b.left
    if (left + mw > window.innerWidth - 8) left = window.innerWidth - mw - 8
    let top = b.bottom + 4
    if (top + mh > window.innerHeight - 8) top = Math.max(8, b.top - mh - 4)
    menu.style.left = Math.max(8, left) + 'px'
    menu.style.top = top + 'px'
    menu.style.visibility = ''
  }
  const apply = (field, color) => {
    const live = liveSheetOf(st) || sheet
    if (!live) { toast(root, '请先在设计区打开电子表格', 'error'); return }
    applySheetUiStyle(live, st, { [field]: color })
    if (!st.__loading) markDirty(st, true)
    updateToolbarControlsAll(st)
  }
  wraps.forEach((wrap) => {
    const field = wrap.getAttribute('data-rd-colorwrap')
    const toggle = wrap.querySelector('[data-color-toggle]')
    const menu = wrap.querySelector('[data-rd-colormenu]')
    toggle?.addEventListener('click', (ev) => {
      ev.stopPropagation()
      const willOpen = !wrap.classList.contains('open')
      closeAll()
      // 也关掉边框菜单（互斥）
      root.querySelector('[data-rd-border]')?.classList.remove('open')
      if (willOpen) { wrap.classList.add('open'); toggle.setAttribute('aria-expanded', 'true'); place(toggle, menu) }
    })
    // 预设色块
    menu?.querySelectorAll('[data-color]').forEach((sw) => sw.addEventListener('click', (ev) => {
      ev.stopPropagation()
      apply(field, sw.getAttribute('data-color'))
      closeAll()
    }))
    // 无填充/自动
    menu?.querySelector('[data-color-none]')?.addEventListener('click', (ev) => {
      ev.stopPropagation()
      apply(field, '') // 空串 → 组件 foreColor/backColor(undefined) 清除
      closeAll()
    })
    // 更多颜色（自定义）
    const custom = menu?.querySelector('[data-color-custom]')
    custom?.addEventListener('click', (ev) => ev.stopPropagation())
    custom?.addEventListener('change', (ev) => {
      ev.stopPropagation()
      apply(field, custom.value)
      closeAll()
    })
    menu?.addEventListener('click', (ev) => ev.stopPropagation())
  })
  document.addEventListener('click', closeAll)
  window.addEventListener('resize', closeAll)
}

/** 导出报表版式为 JSON 文件下载（getWorkbookJson → Blob → a[download]）。 */
function exportReportJson (sheet, st) {
  try {
    const json = sheet.getWorkbookJson ? sheet.getWorkbookJson() : (sheet.getWorkbook?.()?.toJSON?.() || null)
    if (!json) return
    const name = `${st.props.reportCode || 'report'}-${st.props.version || 'default'}.json`
    const blob = new Blob([JSON.stringify(json)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url; a.download = name
    document.body.appendChild(a); a.click()
    setTimeout(() => { document.body.removeChild(a); URL.revokeObjectURL(url) }, 0)
  } catch (_) {}
}

/** 顶栏「报表 ▾」split button 下拉交互（照抄 setupBorderMenu 的 fixed 弹层范式）。 */
function setupReportMenu (root, st, sheet) {
  if (root.__rdRptBound) return
  root.__rdRptBound = true
  const wrap = root.querySelector('[data-rd-rpt]')
  const toggle = root.querySelector('[data-rpt-toggle]')
  const menu = root.querySelector('[data-rd-rpt-menu]')
  if (!wrap || !toggle || !menu) return
  const close = () => { wrap.classList.remove('open'); toggle.setAttribute('aria-expanded', 'false') }
  const place = () => {
    const b = toggle.getBoundingClientRect()
    menu.style.visibility = 'hidden'
    const mr = menu.getBoundingClientRect()
    const mw = mr.width || 190
    let left = b.right - mw
    if (left < 8) left = 8
    let top = b.bottom + 4
    if (top + (mr.height || 240) > window.innerHeight - 8) top = Math.max(8, b.top - (mr.height || 240) - 4)
    menu.style.left = Math.max(8, left) + 'px'
    menu.style.top = top + 'px'
    menu.style.visibility = ''
  }
  toggle.addEventListener('click', (ev) => {
    ev.stopPropagation()
    const willOpen = !wrap.classList.contains('open')
    close()
    if (willOpen) { wrap.classList.add('open'); toggle.setAttribute('aria-expanded', 'true'); place() }
  })
  // 下拉项点击走 runSheetCommand（各项自带 data-sheet-cmd），点后关闭
  menu.querySelectorAll('[data-sheet-cmd]').forEach((btn) => btn.addEventListener('click', () => {
    setTimeout(close, 0)
  }))
  document.addEventListener('click', close)
  window.addEventListener('resize', close)
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
    if (!addr) return
    // ★ 拖拽多选：活动格不变但选区在变（A1→C5，活动格恒 A1）。故活动格 + 选区任一变化都要刷新，
    //   否则名称框/坐标显示会「冻结」在活动格。
    const range = (typeof live.readSelection === 'function') ? live.readSelection() : addr
    if (addr === st.selectedCell && range === st.selectedRange) return
    st.selectedCell = addr
    st.selectedRange = range
    syncSheetUiFromSelection(live, st)
    // property 单元格页跨宿主刷新（联动核心）+ content 工具栏回显
    refreshInstance(st, (view) => view === 'propertyCell')
    updateToolbarControlsAll(st)
    // fx 公式面板打开时：目标格跟随当前选中单元格/表变化（原地更新"写入到 X"，不重渲染以免丢公式与焦点）
    updateFxPanelTarget(st)
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
      // 隐藏组件自带的极简公式栏（名称框 span + 裸 input）——报表设计器改用自建 Excel 样式 .rd-fxbar。
      if (typeof sheet.showFormulaBar === 'function') sheet.showFormulaBar(false)
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
        initOpsSync(st) // 协同 B 档：对齐 op_log 游标 + 启动 30s 追平轮询
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
    updateFxPanelTarget(st) // fx 面板打开时目标格跟随（组件选中事件路径）
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
    // fx 面板打开时切 sheet：目标格跟随新表的活动格
    const live = liveSheetOf(st) || sheet
    const a = (typeof live.getActiveAddr === 'function') ? live.getActiveAddr() : null
    if (a) { st.selectedCell = a; updateFxPanelTarget(st) }
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
