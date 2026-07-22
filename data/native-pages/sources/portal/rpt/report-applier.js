/**
 * 报表应用器 —— native_pages 多实例页面（从报表应用工作台打开，数据消费侧）。
 *
 * 与报表设计器（portal.rpt.designer）互不影响：设计器只设计版式，应用器只跑数据。
 * 复用同一个 cmx-spreadjs-sheet 组件 + 同一批后端端点（layout 读 / data 读写），但不 import designer.js。
 *
 * props: { reportCode, reportName, version, orgCode, periodCode }
 * content ：SpreadJS 画布 + 顶部数据条（组织/期间徽标 + 取数 / 存数 / 导出）。
 * property：报表属性（只读）+ 数据状态。
 *
 * 打开即按版式端点渲染报表格式（BLOB→无损复原，无则初始骨架），用户点「取数」按 org+period
 * 装载单元格值（cr_cell_data），「存数」回写。多实例：每 (报表+版本+组织+期间) 一套实例。
 */

const instances = new Map()
const DEFAULT_REGION = '__default__'

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;')

const enc = (s) => encodeURIComponent(String(s ?? ''))

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
    orgCode: String(p.orgCode || '').trim(),
    periodCode: String(p.periodCode || '').trim(),
  }
}

function instanceKey (props) {
  // 每 (报表+版本+组织+期间) 一套实例，与 cr_cell_data 的键一致。
  return `${props.reportCode || 'UNKNOWN'}@@${props.version || ''}@@${props.orgCode || ''}@@${props.periodCode || ''}`
}

function getState (ctx) {
  const props = propsOf(ctx)
  const key = instanceKey(props)
  if (!instances.has(key)) {
    instances.set(key, {
      props,
      hosts: new Set(),
      report: null,
      reportLoading: false,
      contentHash: null,
      loadedCells: 0,
      dataLoaded: false,
      periods: [], // cr_acct_calendar（explorer 期间下拉）
      org: null, // 当前组织详情行（cr_consol_org）
      explorerLoading: false,
      explorerLoaded: false,
      // 当前选中期间：默认取传入的 periodCode，可在 explorer 下拉换
      curPeriod: props.periodCode || '',
    })
  }
  const st = instances.get(key)
  st.props = props
  if (!st.curPeriod) st.curPeriod = props.periodCode || ''
  return st
}

function reportTitle (st) {
  const code = st.props.reportCode || ''
  const name = st.props.reportName || ''
  return name ? `${code}-${name}` : code || '未指定报表'
}

/** content tab 标签：报表名｜组织/期间（随期间切换更新）。 */
function tabLabelOf (st) {
  const code = String(st.props.reportCode || '').trim()
  const name = String(st.props.reportName || '').trim()
  const base = name ? `${code}-${name}` : code || '报表'
  const ctx = [st.props.orgCode, st.curPeriod || st.props.periodCode].filter(Boolean).join('/')
  return ctx ? `${base}｜${ctx}` : base
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

/** 从 native-page 宿主向上穿 shadow host 链，找到 PORTAL-CONTENT-AREA 组件（失败则全局兜底）。 */
function findContentArea (host) {
  let node = host
  for (let i = 0; i < 40 && node; i++) {
    const tag = node.tagName || ''
    if (tag === 'PORTAL-CONTENT-AREA' || (node._tabs && typeof node.getActiveTabId === 'function')) return node
    node = node.parentElement || (node.parentNode instanceof ShadowRoot ? node.parentNode.host : node.getRootNode?.()?.host) || null
  }
  return deepFindContentArea()
}

/** 本宿主所在 tab 的 id：向上找 dataset.cmxWorkspaceId="tab:<id>" 的挂载根，剥前缀。 */
function ownTabId (host) {
  let node = host
  for (let i = 0; i < 40 && node; i++) {
    const wsId = node.dataset?.cmxWorkspaceId || node.getAttribute?.('data-cmx-workspace-id')
    if (wsId && String(wsId).startsWith('tab:')) return String(wsId).slice(4)
    node = node.parentElement || (node.parentNode instanceof ShadowRoot ? node.parentNode.host : node.getRootNode?.()?.host) || null
  }
  return null
}

/** 设置/清除本报表 tab 的 dirty 标记（关闭时门户据此弹「是否保存」对话框）。 */
function markDirty (st, dirty) {
  st.dirty = !!dirty
  let done = false
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) continue
    const ca = findContentArea(host)
    if (!ca || typeof ca.setTabDirty !== 'function') continue
    const tabId = ownTabId(host) || (ca.getActiveTabId ? ca.getActiveTabId() : ca._activeTab)
    if (tabId) { try { ca.setTabDirty(tabId, !!dirty); done = true } catch (_) {} }
  }
  return done
}

/**
 * 期间切换后更新 content 区当前 tab 的显示标签。
 * 直接改 active .tab-item 的文本 span + 同步 _tabs[].text（不触发 renderTabs，避免销毁画布 CE）。
 */
function updateApplierTab (st) {
  const label = tabLabelOf(st)
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) continue
    if (host.__raView !== 'content') continue
    const ca = findContentArea(host)
    if (!ca) continue
    try {
      const activeId = ca.getActiveTabId ? ca.getActiveTabId() : ca._activeTab
      const items = ca.shadowRoot ? [...ca.shadowRoot.querySelectorAll('.tab-item')] : []
      const item = items.find((el) => el.dataset?.id === activeId)
      // .tab-item 内部：<span.tab-icon-stack>…</span><span>{text}</span><span.tab-close>…
      const textSpan = item ? [...item.querySelectorAll(':scope > span')].find((s) => !s.className) : null
      if (textSpan) textSpan.textContent = label
      const rec = ca._tabs?.find((t) => t.id === activeId)
      if (rec) rec.text = label
    } catch (_) {}
  }
}

function versionLabel (v) {
  return v || '默认版本'
}

function indexToCol (idx) {
  let n = Number(idx) + 1
  let s = ''
  while (n > 0) { const r = (n - 1) % 26; s = String.fromCharCode(65 + r) + s; n = Math.floor((n - 1) / 26) }
  return s || 'A'
}

function parseAddr (addr) {
  const m = /^([A-Z]+)(\d+)$/.exec(String(addr || '').toUpperCase())
  if (!m) return null
  let col = 0
  for (let i = 0; i < m[1].length; i++) col = col * 26 + (m[1].charCodeAt(i) - 64)
  return { col: col - 1, row: Number(m[2]) - 1 }
}

/** base64 SSJSON → 对象（UTF-8 安全） */
function decodeDoc (b64) {
  if (!b64) return null
  try {
    const bin = atob(b64)
    const bytes = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
    return JSON.parse(new TextDecoder().decode(bytes))
  } catch (_) { return null }
}

/** 初始骨架（无 BLOB 版式时兜底渲染）。 */
function skeletonModel (st) {
  return {
    meta: { reportCode: st.props.reportCode, reportName: st.props.reportName, version: st.props.version },
    sheets: [{
      id: 'sheet1',
      name: st.props.reportCode || 'Sheet1',
      grid: { rows: 60, cols: 18, colWidths: { A: 56, B: 160, C: 130 } },
      cells: {
        B1: { type: 'text', value: reportTitle(st), class: 'title' },
        B2: { type: 'text', value: '组织' }, C2: { type: 'text', value: st.props.orgCode || '' },
        B3: { type: 'text', value: '期间' }, C3: { type: 'text', value: st.props.periodCode || '' },
      },
    }],
  }
}

// ============================================================================
// 视图
// ============================================================================

function orgIcon (t) {
  return ({ group: 'company-view', subgroup: 'org-chart', entity: 'building', branch: 'building' })[t] || 'building'
}

/** 加载 explorer 所需：会计日历（期间下拉）+ 当前组织详情行。仅一次。 */
async function loadExplorer (st) {
  if (st.explorerLoading || st.explorerLoaded) return
  st.explorerLoading = true
  refreshInstance(st, (v) => v === 'explorer')
  try {
    const [cal, org] = await Promise.all([
      apiJson('/api/report-design/calendar'),
      apiJson('/api/report-design/consol-org'),
    ])
    st.periods = Array.isArray(cal?.periods) ? cal.periods : []
    const orgs = Array.isArray(org?.orgs) ? org.orgs : []
    st.org = orgs.find((o) => String(o.code) === String(st.props.orgCode)) || null
    st.explorerLoaded = true
  } catch (_) {
    // 静默：explorer 是辅助信息，取数仍可用传入的 period
  } finally {
    st.explorerLoading = false
    refreshInstance(st, (v) => v === 'explorer')
  }
}

function styleCss () {
  return `
    .ra{--ra-blue:#0a6ed1;--ra-cyan:#00a6c8;--ra-green:#10a760;--ra-amber:#d98200;--ra-border:var(--sapGroup_TitleBorderColor,#d9e2ec);
      height:100%;min-height:0;box-sizing:border-box;display:flex;flex-direction:column;overflow:hidden;background:var(--sapBackgroundColor,#f5f6f7);color:var(--sapTextColor,#1d2d3e);font:13px/1.45 var(--sapFontFamily,Arial,sans-serif)}
    .ra-head{height:46px;flex:0 0 auto;display:flex;align-items:center;gap:9px;padding:0 12px;border-bottom:1px solid var(--ra-border);background:var(--sapList_HeaderBackground,#f7f9fc)}
    .ra-head-ic{width:30px;height:30px;border-radius:8px;display:flex;align-items:center;justify-content:center;background:color-mix(in srgb,var(--ra-blue) 12%,transparent);color:var(--ra-blue)}.ra-head-ic ui5-icon{width:1rem;height:1rem}
    .ra-title{min-width:0}.ra-title b,.ra-title span{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.ra-title b{font-size:14px}.ra-title span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .ra-tools{margin-left:auto;display:flex;align-items:center;gap:8px;min-width:0}
    .ra-ctx{display:inline-flex;align-items:center;gap:5px}
    .ra-badge{display:inline-flex;align-items:center;gap:4px;height:26px;padding:0 9px;border-radius:6px;background:var(--sapField_Background,#fff);border:1px solid color-mix(in srgb,var(--ra-blue) 24%,var(--ra-border));color:var(--ra-blue);font-size:11.5px;font-weight:700}.ra-badge ui5-icon{width:.85rem;height:.85rem}
    .ra-hgroup{display:inline-flex;align-items:center;gap:2px;height:32px;padding:2px;border-radius:8px;background:color-mix(in srgb,var(--ra-border) 26%,transparent)}
    .ra-btn{height:28px;border:0;border-radius:6px;background:transparent;color:var(--sapContent_IconColor,#475059);display:inline-flex;align-items:center;justify-content:center;gap:6px;padding:0 10px;font:inherit;font-size:12px;font-weight:600;cursor:pointer;transition:background .12s,color .12s,box-shadow .12s;white-space:nowrap}
    .ra-btn ui5-icon{width:1rem;height:1rem}.ra-btn:hover{background:var(--sapTile_Background,#fff);color:var(--ra-blue);box-shadow:0 1px 4px rgba(10,31,68,.12)}
    .ra-btn.primary{background:linear-gradient(180deg,#1a7ee0,var(--ra-blue));color:#fff;box-shadow:0 1px 2px rgba(10,110,209,.36)}.ra-btn.primary:hover{background:linear-gradient(180deg,#248ceb,#0a63bd);color:#fff}
    .ra-btn:disabled{opacity:.4;cursor:not-allowed;background:transparent!important;color:var(--sapContent_IconColor,#475059)!important;box-shadow:none!important}
    .ra-stage{flex:1;min-height:0;overflow:hidden;padding:12px;background:linear-gradient(180deg,color-mix(in srgb,var(--ra-blue) 4%,var(--sapBackgroundColor,#f5f6f7)),var(--sapBackgroundColor,#f5f6f7))}
    .ra-host{height:100%;min-height:460px;border:1px solid var(--ra-border);border-radius:8px;background:var(--sapTile_Background,#fff);box-shadow:0 4px 18px rgba(10,31,68,.08);overflow:hidden}
    .ra-spread{display:block;width:100%;height:100%;min-height:460px}
    .ra-prop{flex:1;min-height:0;overflow:auto;padding:10px;display:flex;flex-direction:column;gap:10px}
    .ra-hero{display:flex;gap:10px;align-items:center;border:1px solid var(--ra-border);border-radius:8px;background:linear-gradient(135deg,color-mix(in srgb,var(--ra-blue) 12%,var(--sapTile_Background,#fff)),var(--sapTile_Background,#fff));padding:12px}.ra-hero-ic{width:40px;height:40px;border-radius:9px;display:flex;align-items:center;justify-content:center;background:var(--ra-blue);color:#fff}.ra-hero-ic ui5-icon{width:1.35rem;height:1.35rem}.ra-hero b{display:block;font-size:15px}.ra-hero span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .ra-grid{display:grid;grid-template-columns:1fr;gap:6px}.ra-kv{border:1px solid var(--ra-border);border-radius:7px;background:var(--sapTile_Background,#fff);padding:7px 9px}.ra-kv span{display:block;font-size:10px;color:var(--sapContent_LabelColor,#6a6d70)}.ra-kv b{display:block;font-size:12px;word-break:break-word}
    .ra-sec{border:1px solid var(--ra-border);border-radius:8px;background:var(--sapTile_Background,#fff);padding:10px}.ra-sec>b{display:block;margin-bottom:7px;color:var(--ra-blue)}.ra-sec p{margin:0;color:var(--sapContent_LabelColor,#6a6d70);font-size:12px}
    .ra-empty{padding:18px;border:1px dashed var(--ra-border);border-radius:8px;background:var(--sapTile_Background,#fff);color:var(--sapContent_LabelColor,#6a6d70);text-align:center}
    .ra-note{margin:10px;border:1px dashed var(--ra-border);border-radius:8px;padding:12px;background:var(--sapList_HeaderBackground,#f7f9fc);color:var(--sapContent_LabelColor,#6a6d70)}
    .ra-toast{position:absolute;left:50%;bottom:22px;transform:translate(-50%,14px);z-index:60;max-width:min(560px,88%);padding:10px 16px;border-radius:9px;background:#1d2d3e;color:#fff;font-size:12.5px;font-weight:600;box-shadow:0 12px 32px rgba(10,31,68,.34);opacity:0;pointer-events:none;transition:opacity .22s,transform .22s;display:flex;align-items:center;gap:8px}.ra-toast.show{opacity:1;transform:translate(-50%,0)}.ra-toast[data-kind="success"]{background:linear-gradient(180deg,#12b56b,#0f9d5c)}.ra-toast[data-kind="warn"]{background:linear-gradient(180deg,#e0a336,#d98200)}.ra-toast[data-kind="error"]{background:linear-gradient(180deg,#e5544b,#c0392b)}
    /* explorer：期间下拉（顶部标题区，高度与 content .ra-head 一致 46px）+ 组织详情 */
    .ra-explorer{overflow:hidden}
    .ra-period-row{height:46px;flex:0 0 auto;box-sizing:border-box;display:flex;align-items:center;gap:8px;padding:0 12px;border-bottom:1px solid var(--ra-border);background:var(--sapList_HeaderBackground,#f7f9fc)}
    .ra-period-lbl{flex:0 0 auto;display:inline-flex;align-items:center;gap:4px;font-size:12px;font-weight:700;color:var(--sapContent_LabelColor,#6a6d70)}.ra-period-lbl ui5-icon{width:.95rem;height:.95rem;color:var(--ra-cyan)}
    .ra-select-wrap{position:relative;flex:1;min-width:0}
    .ra-select-wrap select{width:100%;height:32px;border:1px solid color-mix(in srgb,var(--ra-blue) 20%,var(--ra-border));border-radius:8px;background:var(--sapField_Background,#fff);color:inherit;font:inherit;font-size:12.5px;font-weight:600;padding:0 30px 0 11px;cursor:pointer;-webkit-appearance:none;appearance:none;box-shadow:0 1px 2px rgba(10,31,68,.05);transition:border-color .15s,box-shadow .15s}
    .ra-select-wrap select:hover{border-color:color-mix(in srgb,var(--ra-blue) 42%,var(--ra-border))}
    .ra-select-wrap select:focus{outline:0;border-color:var(--ra-blue);box-shadow:0 0 0 3px color-mix(in srgb,var(--ra-blue) 14%,transparent)}
    .ra-select-caret{position:absolute;right:9px;top:50%;transform:translateY(-50%);width:.85rem;height:.85rem;color:var(--ra-blue);pointer-events:none}
    .ra-org-scroll{flex:1;min-height:0;overflow:auto}
    .ra-org-head{display:flex;align-items:center;gap:6px;padding:9px 10px 5px;font-size:11px;font-weight:700;color:var(--sapContent_LabelColor,#6a6d70);text-transform:uppercase;letter-spacing:.03em}.ra-org-head ui5-icon{width:.9rem;height:.9rem;color:var(--ra-blue)}
    .ra-org-hero{display:flex;gap:9px;align-items:center;margin:0 10px 8px;border:1px solid var(--ra-border);border-radius:8px;background:linear-gradient(135deg,color-mix(in srgb,var(--ra-blue) 12%,var(--sapTile_Background,#fff)),var(--sapTile_Background,#fff));padding:10px}.ra-org-ic{width:34px;height:34px;flex:0 0 auto;border-radius:8px;display:flex;align-items:center;justify-content:center;background:var(--ra-blue);color:#fff}.ra-org-ic ui5-icon{width:1.15rem;height:1.15rem}.ra-org-hero b{display:block;font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.ra-org-hero span{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .ra-org-grid{display:grid;grid-template-columns:1fr;gap:6px;padding:0 10px 10px}
  `
}

// ============================================================================
// explorer：期间下拉 + 当前组织详情
// ============================================================================

function explorerPeriodSelect (st) {
  const years = st.periods.filter((p) => Number(p.level_no) === 1)
  const groups = years.map((y) => {
    const months = st.periods.filter((p) => p.parent_code === y.code && Number(p.is_leaf) === 1)
    const opts = months.map((m) =>
      `<option value="${esc(m.code)}" ${st.curPeriod === m.code ? 'selected' : ''}>${esc(m.name)}</option>`).join('')
    return `<optgroup label="${esc(y.name)}">${opts}</optgroup>`
  }).join('')
  // 若日历未加载但有传入期间，至少给一个当前项
  const fallback = st.curPeriod ? `<option value="${esc(st.curPeriod)}" selected>${esc(st.curPeriod)}</option>` : '<option value="">（无期间）</option>'
  return `<div class="ra-period-row">
    <span class="ra-period-lbl"><ui5-icon name="calendar"></ui5-icon>期间</span>
    <div class="ra-select-wrap">
      <select data-ra-period>${groups || fallback}</select>
      <ui5-icon class="ra-select-caret" name="slim-arrow-down"></ui5-icon>
    </div>
  </div>`
}

function orgDetailHtml (st) {
  const o = st.org
  if (st.explorerLoading && !o) return '<div class="ra-empty" style="margin:10px">正在加载组织详情...</div>'
  if (!o) {
    return `<div class="ra-org-head"><ui5-icon name="tree"></ui5-icon><span>组织机构</span></div>
      <div class="ra-empty" style="margin:10px">未找到组织 ${esc(st.props.orgCode || '')} 的详情</div>`
  }
  return `<div class="ra-org-head"><ui5-icon name="${orgIcon(o.org_type)}"></ui5-icon><span>组织机构</span></div>
    <div class="ra-org-hero">
      <span class="ra-org-ic"><ui5-icon name="${orgIcon(o.org_type)}"></ui5-icon></span>
      <div><b>${esc(o.name)}</b><span>${esc(o.code)} · ${esc(o.org_type || '')}</span></div>
    </div>
    <div class="ra-org-grid">
      ${kv('组织编码', o.code)}
      ${kv('核算实体', o.entity_code)}
      ${kv('合并方案', o.consol_scheme)}
      ${kv('合并方法', o.consol_method)}
      ${kv('持股比例', o.ownership_pct != null ? `${o.ownership_pct}%` : '-')}
      ${kv('表决权比例', o.voting_pct != null ? `${o.voting_pct}%` : '-')}
      ${kv('合并币种', o.consol_currency)}
      ${kv('是否母公司', Number(o.is_parent) === 1 ? '是' : '否')}
      ${kv('内部抵消', Number(o.offset_flag) === 1 ? '参与抵消' : '不抵消')}
      ${kv('层级深度', o.level_no)}
      ${kv('全路径', o.full_path)}
    </div>
    ${o.remark ? `<div class="ra-sec" style="margin:0 10px 10px"><b>备注</b><p>${esc(o.remark)}</p></div>` : ''}`
}

function explorerHtml (st) {
  return `<section class="ra ra-explorer">
    ${explorerPeriodSelect(st)}
    <div class="ra-org-scroll">${orgDetailHtml(st)}</div>
  </section>`
}

function contextBadges (st) {
  return `<span class="ra-ctx">
    <span class="ra-badge" title="组织"><ui5-icon name="tree"></ui5-icon>${esc(st.props.orgCode || '未指定组织')}</span>
    <span class="ra-badge" title="会计期间"><ui5-icon name="calendar"></ui5-icon>${esc(st.curPeriod || st.props.periodCode || '未指定期间')}</span>
  </span>`
}

function contentHtml (st) {
  const model = skeletonModel(st)
  return `<section class="ra">
    <div class="ra-head">
      <span class="ra-head-ic"><ui5-icon name="table-chart"></ui5-icon></span>
      <span class="ra-title"><b>${esc(reportTitle(st))}</b><span>${esc(versionLabel(st.props.version))} · 报表应用（数据）</span></span>
      <span class="ra-tools">
        ${contextBadges(st)}
        <span class="ra-hgroup">
          <button class="ra-btn primary" type="button" data-ra-cmd="load" title="按组织+期间装载数据">${'<ui5-icon name="download-from-cloud"></ui5-icon>'}<span>取数</span></button>
          <button class="ra-btn" type="button" data-ra-cmd="compute" title="按公式后端真算（QM/QC/REF…），落库并刷新">${'<ui5-icon name="function"></ui5-icon>'}<span>计算</span></button>
          <button class="ra-btn" type="button" data-ra-cmd="save" title="保存数据到 cr_cell_data">${'<ui5-icon name="save"></ui5-icon>'}<span>存数</span></button>
          <button class="ra-btn" type="button" data-ra-cmd="export" title="导出 Excel">${'<ui5-icon name="excel-attachment"></ui5-icon>'}<span>导出</span></button>
        </span>
      </span>
    </div>
    <div class="ra-stage"><div class="ra-host"><cmx-spreadjs-sheet class="ra-spread" data-ra-spread data-cmx-report="${esc(JSON.stringify(model))}"></cmx-spreadjs-sheet></div></div>
  </section>`
}

function propertyHtml (st) {
  const r = st.report || {}
  return `<section class="ra ra-prop">
    <div class="ra-hero">
      <span class="ra-hero-ic"><ui5-icon name="detail-view"></ui5-icon></span>
      <div><b>${esc(r.name || st.props.reportName || st.props.reportCode)}</b><span>${esc(st.props.reportCode)} · ${esc(versionLabel(st.props.version))}</span></div>
    </div>
    <div class="ra-grid">
      ${kv('报表编码', r.code || st.props.reportCode)}
      ${kv('报表名称', r.name || st.props.reportName)}
      ${kv('报表类型', r.report_type)}
      ${kv('报表类别', r.report_category)}
      ${kv('期间类型', r.period_type)}
      ${kv('币种 / 单位', `${r.currency_code || '-'} / ${r.amount_unit || '-'}`)}
      ${kv('取数来源', r.data_source || '未指定')}
      ${kv('状态', r.status == null ? '-' : (Number(r.status) === 0 ? '停用' : '启用'))}
    </div>
    <div class="ra-sec"><b>说明</b><p>${esc(r.remark || '暂无备注')}</p></div>
  </section>`
}

function propertyStatusHtml (st) {
  return `<section class="ra ra-prop">
    <div class="ra-hero">
      <span class="ra-hero-ic"><ui5-icon name="status-positive"></ui5-icon></span>
      <div><b>数据状态</b><span>${esc(reportTitle(st))}</span></div>
    </div>
    <div class="ra-grid">
      ${kv('组织', st.props.orgCode || '未指定')}
      ${kv('会计期间', st.props.periodCode || '未指定')}
      ${kv('版式已加载', st.contentHash ? '是' : '首次/骨架')}
      ${kv('已装载单元格', st.dataLoaded ? String(st.loadedCells) : '尚未取数')}
    </div>
    <div class="ra-sec"><b>说明</b><p>「取数」按组织+期间从 cr_cell_data 装载单元格值并覆盖到版式画布（保留格式与公式）；「存数」把画布上的手工/非公式值回写 cr_cell_data。公式计算另案。</p></div>
  </section>`
}

function kv (label, value) {
  return `<div class="ra-kv"><span>${esc(label)}</span><b>${esc(value == null || value === '' ? '-' : value)}</b></div>`
}

function viewHtml (view, st) {
  if (view === 'explorer') return explorerHtml(st)
  if (view === 'property') return propertyHtml(st)
  if (view === 'propertyStatus') return propertyStatusHtml(st)
  return contentHtml(st)
}

// ============================================================================
// toast
// ============================================================================

function toast (root, message, kind = 'info') {
  requestAnimationFrame(() => {
    const host = root.querySelector('.ra') || root
    if (!host) return
    if (getComputedStyle(host).position === 'static') host.style.position = 'relative'
    let box = host.querySelector(':scope > .ra-toast')
    if (!box) { box = document.createElement('div'); box.className = 'ra-toast'; host.appendChild(box) }
    box.setAttribute('data-kind', kind)
    box.textContent = message
    box.classList.remove('show'); void box.offsetWidth; box.classList.add('show')
    clearTimeout(box.__t)
    box.__t = setTimeout(() => box.classList.remove('show'), 3200)
  })
}

// ============================================================================
// 版式加载 + 数据取/存（复用后端端点，逻辑本地实现，不 import designer.js）
// ============================================================================

/** 打开即加载版式：GET layout → 有 BLOB 用 setWorkbookJson 无损复原，无则初始骨架。 */
async function loadLayout (sheet, st, root) {
  try {
    const data = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/layout?version=${enc(st.props.version || '')}`)
    st.contentHash = data?.fmt?.contentHash || null
    const wbJson = decodeDoc(data?.fmt?.docContent)
    if (wbJson && sheet.setWorkbookJson) {
      await sheet.setWorkbookJson(wbJson)
    } else if (sheet.setReportModel) {
      sheet.setReportModel(skeletonModel(st))
    }
    refreshInstance(st, (v) => v === 'propertyStatus')
    return true
  } catch (_) {
    if (sheet.setReportModel) sheet.setReportModel(skeletonModel(st))
    return false
  }
}

/** 取数：POST data/query → setCellValues 覆盖画布值（保留版式与公式）。 */
async function loadData (sheet, st, root) {
  const { orgCode, periodCode } = st.props
  if (!orgCode || !periodCode) { toast(root, '缺少组织或期间上下文', 'error'); return }
  try {
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/data/query`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version: st.props.version || '', orgCode, periodCode }),
    })
    const cells = res?.cells || []
    // 取数值双路：
    //  ① 公式格(画布有 =FS(...) 等公式)——灌 setReportValueMap，函数按格取值显示，**不覆盖公式**。
    //  ② 非公式格(手工值)——仍 setCellValues 直填。
    const wb = sheet.getWorkbook && sheet.getWorkbook()
    const ws = wb && wb.getActiveSheet && wb.getActiveSheet()
    const sheetName = (ws && ws.name && ws.name()) || ''
    const parseRef = (ref) => { const m = /^([A-Z]+)(\d+)$/.exec(String(ref || '').toUpperCase()); if (!m) return null; let c = 0; for (let i = 0; i < m[1].length; i++) c = c * 26 + (m[1].charCodeAt(i) - 64); return { row: Number(m[2]) - 1, col: c - 1 } }
    const valueMap = {}     // sheetName!CELLREF -> value（供公式格取数）
    const plainValues = {}  // CELLREF -> value（非公式格直填）
    for (const r of cells) {
      if (!r.cellRef) continue
      const v = r.valueType === 'number' ? r.numValue : r.textValue
      valueMap[`${sheetName}!${String(r.cellRef).toUpperCase()}`] = v
      // 判断该格画布上是否是公式格
      let hasFormula = false
      const p = parseRef(r.cellRef)
      if (ws && p) { try { hasFormula = !!ws.getFormula(p.row, p.col) } catch (_) {} }
      if (!hasFormula) plainValues[r.cellRef] = v
    }
    st.__loading = true
    // 先灌 map（公式格显真值），再直填非公式格
    if (sheet.setReportValueMap) sheet.setReportValueMap(valueMap)
    if (sheet.setCellValues && Object.keys(plainValues).length) sheet.setCellValues(plainValues)
    setTimeout(() => { st.__loading = false }, 200)
    st.dataLoaded = true
    st.loadedCells = cells.length
    markDirty(st, false) // 取数=从DB装载，画布与DB一致，清除未保存标记
    toast(root, `已装载 ${cells.length} 个单元格数据`, 'success')
    refreshInstance(st, (v) => v === 'propertyStatus')
  } catch (err) {
    toast(root, `取数失败：${String(err?.message || err)}`, 'error')
  }
}

/** 计算：POST compute → 后端装载公式递归求值（QM/QC/REF…）落 cr_cell_data → 再取数刷新画布。 */
async function computeData (sheet, st, root) {
  const orgCode = st.props.orgCode
  const periodCode = st.curPeriod || st.props.periodCode
  if (!orgCode || !periodCode) { toast(root, '缺少组织或期间上下文', 'error'); return }
  try {
    toast(root, '正在按公式计算…', 'info')
    const res = await apiJson(`/api/report-design/reports/${enc(st.props.reportCode)}/compute`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version: st.props.version || '', orgCode, periodCode }),
    })
    const computed = res?.computed || 0
    const errs = res?.errorCount || 0
    if (errs > 0) {
      const detail = (res?.errors || []).slice(0, 3).join('；')
      toast(root, `计算完成：${computed} 格已算，${errs} 格异常（${detail}）`, 'warn')
    } else {
      toast(root, `计算完成：${computed} 个单元格已算并落库`, 'success')
    }
    // 计算已落 cr_cell_data，取数把算好的值刷回画布
    await loadData(sheet, st, root)
  } catch (err) {
    toast(root, `计算失败：${String(err?.message || err)}`, 'error')
  }
}

/** 存数：收集画布非公式有值单元格 → POST data（按 org+period UPSERT cr_cell_data）。 */
async function saveData (sheet, st, root) {
  const { orgCode, periodCode } = st.props
  if (!orgCode || !periodCode) { toast(root, '缺少组织或期间上下文', 'error'); return }
  const wb = sheet.getWorkbook?.()
  const ws = wb?.getActiveSheet?.()
  if (!ws) { toast(root, '工作簿未就绪', 'error'); return }
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
        // row_id/col_id 用画布网格位置(1基)作稳定唯一键——cr_cell_data 唯一键含 row_id+col_id，
        // 若全用 0 会让所有单元格撞同一键、互相覆盖。装载按 cellRef 回填，故此处只需保证每格唯一。
        rowId: r + 1, colId: c + 1, cellRef: `${indexToCol(c)}${r + 1}`,
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
    markDirty(st, false) // 存数成功 → 清除未保存标记
    toast(root, `已保存 ${cells.length} 个单元格数据`, 'success')
    return true
  } catch (err) {
    toast(root, `存数失败：${String(err?.message || err)}`, 'error')
    return false
  }
}

async function loadReportMeta (st) {
  if (st.report || st.reportLoading || !st.props.reportCode) return
  st.reportLoading = true
  try {
    const url = `/api/report-design/reports/${enc(st.props.reportCode)}${st.props.version ? `?version=${enc(st.props.version)}` : ''}`
    const data = await apiJson(url)
    st.report = data?.report || null
  } catch (_) {} finally {
    st.reportLoading = false
    refreshInstance(st, (v) => v === 'property')
  }
}

// ============================================================================
// 组件初始化 + 绑定
// ============================================================================

function ensureSpreadElementRegistered () {
  if (customElements.get('cmx-spreadjs-sheet')) return true
  const C = (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}
  if (C.CmxSpreadjsSheet) {
    try { customElements.define('cmx-spreadjs-sheet', C.CmxSpreadjsSheet); return true } catch {}
  }
  return false
}

/**
 * 监听门户「关闭含未保存修改的 tab → 点保存」派发的 portal-content-tab-save-request。
 * 仅当 tabId 命中本实例的 content 宿主时执行存数。每实例只挂一次。
 */
function setupSaveRequestListener (st) {
  if (st.__saveReqBound) return
  st.__saveReqBound = true
  document.addEventListener('portal-content-tab-save-request', (ev) => {
    const tabId = ev.detail?.tabId
    if (!tabId) return
    for (const host of Array.from(st.hosts)) {
      if (!host || !host.isConnected || host.__raView !== 'content') continue
      if (String(ownTabId(host)) !== String(tabId)) continue
      const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
      const sheet = root?.querySelector?.('[data-ra-spread]')
      if (sheet) saveData(sheet, st, root)
      break
    }
  })
}

function initSpread (root, st) {
  const sheet = root.querySelector('[data-ra-spread]')
  if (!sheet || sheet.__raBound) return
  sheet.__raBound = true
  // 组件派发的 cmx-cell-edited（编程改值时）→ 置 dirty
  sheet.addEventListener('cmx-cell-edited', () => {
    if (st.__loading) return
    markDirty(st, true)
  })
  setupSaveRequestListener(st)
  const apply = () => {
    try {
      if (typeof sheet.showFormulaBar === 'function') sheet.showFormulaBar(true)
      if (typeof sheet.showHeaders === 'function') sheet.showHeaders(true)
      if (typeof sheet.showGridlines === 'function') sheet.showGridlines(true)
      // 应用器只跑数据，画布默认只读（避免误改版式）；存数收集的是画布当前值。
      if (typeof sheet.setEditable === 'function') sheet.setEditable(true)
      st.__loading = true
      loadLayout(sheet, st, root).catch(() => {
        if (typeof sheet.setReportModel === 'function') sheet.setReportModel(skeletonModel(st))
      }).finally(() => {
        setTimeout(() => { st.__loading = false }, 300)
        bindWorkbookEditEvents(sheet, st) // 用户键盘编辑靠这个（组件只绑了 CellChanged，用户输入不触发）
      })
    } catch (err) {
      st.__loading = false
      sheet.insertAdjacentHTML('afterend', `<div class="ra-note">SpreadJS 初始化失败：${esc(err instanceof Error ? err.message : String(err))}</div>`)
    }
  }
  if (ensureSpreadElementRegistered()) { apply(); return }
  customElements.whenDefined('cmx-spreadjs-sheet').then(apply)
  setTimeout(() => {
    if (!customElements.get('cmx-spreadjs-sheet')) {
      sheet.insertAdjacentHTML('afterend', '<div class="ra-note">cmx-spreadjs-sheet 组件尚未注册，请确认 cmx-data-comp 已预加载。</div>')
    }
  }, 1200)
}

/**
 * 直接绑 SpreadJS 的用户编辑事件 → markDirty。
 * ★ 组件内部只绑了 Events.CellChanged（编程改值触发），**用户键盘输入提交走 ValueChanged/EditEnded，
 * 不一定触发 CellChanged**，故在此补绑。SpreadJS 的编辑事件既可绑在 workbook 也可绑在 worksheet，
 * 为稳妥两者都绑。workbook.bind/sheet.bind 接受事件名字符串。工作簿未就绪则重试。
 */
function bindWorkbookEditEvents (sheet, st, tries = 0) {
  const wb = sheet.getWorkbook?.()
  if (!wb) {
    if (tries < 20) setTimeout(() => bindWorkbookEditEvents(sheet, st, tries + 1), 300)
    return
  }
  if (wb.__raEditBound) return
  wb.__raEditBound = true
  const onEdit = () => { if (!st.__loading) markDirty(st, true) }
  const EVENTS = ['ValueChanged', 'EditEnded', 'ClipboardPasted', 'RangeChanged', 'CellChanged', 'DragDropBlockCompleted', 'DragFillBlockCompleted']
  // workbook 级
  for (const name of EVENTS) { try { wb.bind(name, onEdit) } catch (_) {} }
  // worksheet 级（部分编辑事件只在 sheet 上派发）——绑当前 + 后续所有 sheet
  const bindSheet = (ws) => {
    if (!ws || ws.__raEditBound) return
    ws.__raEditBound = true
    for (const name of EVENTS) { try { ws.bind(name, onEdit) } catch (_) {} }
  }
  try { const cnt = wb.getSheetCount?.() || 1; for (let i = 0; i < cnt; i++) bindSheet(wb.getSheet?.(i)) } catch (_) { bindSheet(wb.getActiveSheet?.()) }
  try { wb.bind('ActiveSheetChanged', () => bindSheet(wb.getActiveSheet?.())) } catch (_) {}
}

function bind (root, st, view) {
  if (view === 'content') {
    const sheet = root.querySelector('[data-ra-spread]')
    root.querySelectorAll('[data-ra-cmd]').forEach((btn) => btn.addEventListener('click', () => {
      const cmd = btn.getAttribute('data-ra-cmd')
      if (!sheet) { toast(root, '画布未就绪', 'error'); return }
      if (cmd === 'load') loadData(sheet, st, root)
      else if (cmd === 'compute') computeData(sheet, st, root)
      else if (cmd === 'save') saveData(sheet, st, root)
      else if (cmd === 'export') sheet.exportXlsx?.(`${st.props.reportCode || 'report'}-${st.props.orgCode || ''}-${st.curPeriod || st.props.periodCode || ''}`)
    }))
    initSpread(root, st)
  } else if (view === 'explorer') {
    root.querySelector('[data-ra-period]')?.addEventListener('change', (ev) => {
      const val = ev.target.value || ''
      st.curPeriod = val
      st.props.periodCode = val // 后续取数/存数按新期间
      // content 页顶部徽标同步 + 更新 content 区 tab 标签
      refreshInstance(st, (v) => v === 'content' || v === 'propertyStatus')
      updateApplierTab(st)
      if (st.dataLoaded) toast(root, `期间已切到 ${val}，请在报表页点「取数」刷新数据`, 'info')
    })
  }
}

function mount (ctx, view) {
  const st = getState(ctx)
  const host = ctx.host
  st.hosts.add(host)
  if (host) host.__raView = view
  const render = () => {
    const root = host?.renderRoot || host?.shadowRoot?.querySelector('.native-page-root')
    if (!root || !root.isConnected) return
    root.innerHTML = `<style>${styleCss()}</style>${viewHtml(view, st)}`
    bind(root, st, view)
  }
  requestAnimationFrame(render)
  if (view === 'property') loadReportMeta(st)
  if (view === 'explorer') loadExplorer(st)
  return `<style>${styleCss()}</style>${viewHtml(view, st)}`
}

function refreshInstance (st, predicate) {
  for (const host of Array.from(st.hosts)) {
    if (!host || !host.isConnected) { st.hosts.delete(host); continue }
    const root = host.renderRoot || host.shadowRoot?.querySelector('.native-page-root')
    if (!root) continue
    const view = host.__raView || 'content'
    if (predicate && !predicate(view)) continue
    root.innerHTML = `<style>${styleCss()}</style>${viewHtml(view, st)}`
    bind(root, st, view)
  }
}

export default {
  defaultView: 'content',
  views: {
    async explorer (ctx) { return mount(ctx, 'explorer') },
    async content (ctx) { return mount(ctx, 'content') },
    async property (ctx) { return mount(ctx, 'property') },
    async propertyStatus (ctx) { return mount(ctx, 'propertyStatus') },
  },
}
