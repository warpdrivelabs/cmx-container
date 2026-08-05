/**
 * MDM 查重候选台（native-page · 企业级重设计 v4）。
 *
 * 设计要点（Neo 主题 + 换肤 + 克制视觉）：
 *  - 三区垂直（条件→候选→历史），`.neo-panel` 卡片分区，颜色全 `var(--sap*|--neo-*)` 派生，不硬编码。
 *  - cmx 组件不写 data-cmx-skin 即走门户默认 Neo；light/dark 自动跟随。
 *  - 单一签名：字段对比表差异行红底高亮 + 一致行弱化。
 *
 * 业务流程（查重预览不落库 → 用户确认合并才落库）：
 *  ① 选数据字典 → ② 选/编辑查重规则（内嵌维护，无独立管理页）→ ③ 选目标记录 → ④ 查重（不落库）
 *  ⑤ 候选列表 + 选中展开字段对比 → ⑥ 勾选 victim 执行合并（落库）→ ⑦ 历史区可还原。
 *
 * 契约：export default { defaultView:'content', views:{ async content(ctx) } }；CMX 类经 globalThis.__cmxDataComp。
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

// 中文状态映射（后端 status 全英文，前端统一中文展示）
const STATUS_CN = {
  pending: '待处理', reviewed: '已合并', rejected: '已驳回', unmerged: '已还原',
  automerge: '自动合并', review: '待评审', nomatch: '不匹配',
}
const DECISION_META = {
  AutoMerge: { name: '自动合并', tone: 'success' },
  Review: { name: '待评审', tone: 'warning' },
  NoMatch: { name: '不匹配', tone: 'neutral' },
}
const statusCn = (s) => STATUS_CN[s] || s || ''

// 全局状态
const state = {
  dbId: '', domain: '', application: '', module: '',
  dictCode: '', dictMeta: null,            // 选中的字典 + 其 meta（columns）
  rule: null,                              // 当前查重规则（来自 match-config 或用户新建）
  rules: [],                               // 该字典已有规则列表
  ruleDirty: false,                        // 规则编辑器有未保存改动
  targetId: null, targetRow: null,         // 目标记录
  result: null,                            // 查重结果 {targetFields,candidates,thresholds}
  selCand: null,                           // 当前选中对比的候选
  victimIds: [],                           // 勾选待合并的 victim id
  // 历史区
  histDict: '', histKw: '', histPage: 1, histPageSize: 10, histList: [], histTotal: 0,
}

function styleCss() {
  return `
  .pg { height:100%; overflow:auto; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor,#f7f7f7); color:var(--sapTextColor,#1d2d3e);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:12px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor,#1d2d3e); }
  .pg-sub { font-size:12px; color:var(--sapContent_LabelColor,#6a6d70); margin-top:2px; }
  .neo-panel { background:var(--sapList_Background,#fff); border:1px solid var(--neo-border,var(--sapGroup_ContentBorderColor,#d9d9d9));
    border-radius:6px; overflow:hidden; margin-bottom:12px; }
  .neo-panel-head { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:8px 14px;
    background:var(--sapList_HeaderBackground,#f5f6f7); border-bottom:1px solid var(--neo-border-subtle,#e9e9e9); }
  .neo-panel-head .pt { font-size:14px; font-weight:600; color:var(--sapTitleColor,#1d2d3e); display:flex; align-items:center; gap:6px; }
  .neo-panel-head .pt ui5-icon { color:var(--neo-cyan,#00b4d8); }
  .neo-panel-body { padding:12px 14px; }
  .muted { color:var(--sapContent_LabelColor,#6a6d70); }
  .bar { display:flex; gap:10px; align-items:flex-end; flex-wrap:wrap; }
  .bar .f-item { display:flex; flex-direction:column; gap:4px; min-width:240px; flex:1 1 240px; }
  .bar label { font-size:12px; color:var(--sapContent_LabelColor,#6a6d70); }
  cmx-dict-select { display:block; }
  .hint { font-size:12px; color:var(--sapContent_LabelColor,#6a6d70); margin-top:6px; }
  /* 规则编辑器 */
  .rule-bar { display:flex; gap:8px; align-items:center; flex-wrap:wrap; margin-bottom:10px; }
  .rule-fields { display:flex; flex-direction:column; gap:6px; margin-top:6px; }
  .rule-row { display:flex; gap:8px; align-items:center; padding:5px 8px; border-radius:4px;
    background:var(--sapList_Background,#fff); border:1px solid var(--sapGroup_ContentBorderColor,#e9d9d9); }
  .rule-row .rf-name { min-width:120px; font-size:13px; }
  .rule-row ui5-select { min-width:120px; }
  .survive-row { margin-top:8px; }
  .survive-row .chk-grid { display:flex; flex-wrap:wrap; gap:8px 18px; margin-top:4px; }
  /* 候选区 */
  .cand-wrap { min-height:200px; }
  .tbl { width:100%; border-collapse:collapse; font-size:13px; }
  .tbl th { text-align:left; padding:9px 12px; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor,#6a6d70);
    border-bottom:1px solid var(--sapList_BorderColor,#e5e5e5); background:var(--sapList_HeaderBackground,#f5f6f7); }
  .tbl td { padding:9px 12px; border-bottom:1px solid var(--sapList_BorderColor,#e5e5e5); vertical-align:middle; }
  .tbl tbody tr { cursor:pointer; }
  .tbl tbody tr:hover td { background:var(--sapList_Hover_Background,#f5f5f5); }
  .tbl tbody tr.sel td { background:var(--sapInformationBackground,#eaf4ff); }
  .tbl tbody tr.diff td { background:var(--sapErrorBackground,#ffebeb); }
  .tbl tbody tr.same td { color:var(--sapContent_LabelColor,#6a6d70); }
  .score { font-weight:600; }
  .empty { padding:30px 12px; text-align:center; color:var(--sapContent_LabelColor,#6a6d70); font-size:13px; }
  /* 对比表 */
  .cmp-tip { font-size:12px; color:var(--sapContent_LabelColor,#6a6d70); padding:8px 0; }
  cmx-toolbar { display:flex; gap:8px; }
  `
}

// ── 字典选择 ────────────────────────────────────────────────────────────
function dictSource() {
  // 走 /api/model/definitions/list?kind=DCT 取所有字典定义文件，再聚合 dictCode。
  // 简化：直接列几个已知 mdm 域字典（supplier），帮助弹窗用 dct/data/search。
  return {
    keyField: 'dictCode', labelField: 'dictName', pageSize: 50,
    search: async (query) => {
      const d = await apiGet('/api/model/definitions/list?kind=DCT&domain=basic&application=dataplatform&module=mdm', state.dbId)
      const items = (d && d.items) || []
      // 每个 file 再取 config 读 dictionaryTables（这里简化：本地维护可选字典）
      const known = [{ dictCode: 'supplier', dictName: '供应商', targetTable: 'cm_supplier' }]
      const q = (query || '').toLowerCase()
      return known.filter((x) => !q || x.dictName.toLowerCase().includes(q) || x.dictCode.toLowerCase().includes(q))
    },
    loadByKeys: async (keys) => {
      const known = [{ dictCode: 'supplier', dictName: '供应商', targetTable: 'cm_supplier' }]
      return known.filter((x) => keys.includes(x.dictCode))
    },
  }
}

function recordSource() {
  if (!state.dictCode) return null
  return {
    keyField: 'id', labelField: 'name', pageSize: 50,
    search: async (query, o) => {
      const q = new URLSearchParams({ domain: state.domain, application: state.application, module: state.module, dict: state.dictCode })
      const d = await apiPost('/api/dct/data/search?' + q.toString(), { page: (o && o.page) || 1, pageSize: (o && o.pageSize) || 50, q: query || '' }, state.dbId)
      return (d && d.rows) || []
    },
    loadByKeys: async (keys) => {
      if (!keys || !keys.length) return []
      const q = new URLSearchParams({ domain: state.domain, application: state.application, module: state.module, dict: state.dictCode })
      const d = await apiPost('/api/dct/data/search?' + q.toString(), { page: 1, pageSize: Math.max(20, keys.length), filters: { id: keys } }, state.dbId)
      return (d && d.rows) || []
    },
  }
}

async function loadDictMeta() {
  if (!state.dictCode) { state.dictMeta = null; return }
  const q = new URLSearchParams({ domain: state.domain, application: state.application, module: state.module, dict: state.dictCode, with_props: 'true' })
  state.dictMeta = await apiGet('/api/dct/meta?' + q.toString(), state.dbId)
}

async function loadRules() {
  if (!state.dictCode) { state.rules = []; return }
  state.rules = (await apiGet(`/api/mdm/match-configs?dictCode=${encodeURIComponent(state.dictCode)}`, state.dbId)) || []
  // 默认选第一条
  if (state.rules.length && !state.rule) state.rule = normalizeRule(state.rules[0])
  else if (!state.rules.length) state.rule = null
}

// 把后端规则或用户新建统一成编辑器内部结构
function normalizeRule(r) {
  if (!r) return null
  const specs = (r.specs || []).map((s) => ({ field: s.field, weight: s.weight ?? 0, kind: s.kind || 'Exact' }))
  return {
    id: r.id || '', ruleName: r.rule_name || r.ruleName || '',
    dictCode: r.dict_code || r.dictCode || state.dictCode,
    targetTable: r.target_table || r.targetTable || (state.dictMeta && state.dictMeta.tableName) || '',
    specs, clusterKeys: r.cluster_keys || r.clusterKeys || specs.map((s) => s.field),
    surviveFields: r.survive_fields || r.surviveFields || [],
    thresholds: r.thresholds || { auto_merge: 95, review: 80 },
  }
}

// ── 渲染：查重条件区 ────────────────────────────────────────────────────
function condHtml() {
  const C = cmx()
  const dictCfg = {
    dictCode: '_selector', idCol: 'dictCode', labelCol: 'dictName',
    helpLayout: 'grid', dataSource: dictSource(), dictTitle: '选择数据字典',
    columns: [
      C.CmxColumn && new C.CmxColumn({ id: 'dictCode', caption: '字典码', dataType: 'VARCHAR', width: '140px' }),
      C.CmxColumn && new C.CmxColumn({ id: 'dictName', caption: '字典名称', dataType: 'VARCHAR' }),
    ].filter(Boolean),
  }
  const recSel = state.dictCode ? `<div class="f-item">
      <label>目标记录</label>
      <cmx-dict-select id="dcRecord" ${state.targetRow ? `value="${state.targetRow.id}"` : ''}></cmx-dict-select>
    </div>` : ''
  return `<section class="neo-panel">
    <div class="neo-panel-head"><div class="pt"><ui5-icon name="filter"></ui5-icon>查重条件</div></div>
    <div class="neo-panel-body">
      <div class="bar">
        <div class="f-item">
          <label>数据字典</label>
          <cmx-dict-select id="dcDict" ${state.dictCode ? `value="${state.dictCode}"` : ''}></cmx-dict-select>
        </div>
        ${recSel}
        <ui5-button design="Emphasized" icon="search" id="dcFind" ?disabled=${!state.dictCode || !state.targetId || !ruleHasFields()}>查重</ui5-button>
      </div>
      ${state.dictCode ? ruleHtml() : '<div class="hint">请先选择数据字典</div>'}
    </div>
  </section>`
}

function ruleHtml() {
  if (!state.dictMeta) return '<div class="hint">加载字典字段中…</div>'
  const r = state.rule || newBlankRule()
  const ruleOpts = state.rules.map((x) => `<ui5-option value="${x.id}" ${String(r.id) === String(x.id) ? 'selected' : ''}>${x.rule_name}</ui5-option>`).join('')
  // 字段勾选列表
  const fields = pickableFields()
  const fieldRows = fields.map((f) => {
    const sel = r.specs.find((s) => s.field === f.name)
    const checked = !!sel
    const weight = sel ? sel.weight : ''
    const kind = sel ? sel.kind : 'Exact'
    return `<div class="rule-row">
      <ui5-checkbox ?checked=${checked} data-field="${f.name}" class="rf-chk"></ui5-checkbox>
      <span class="rf-name" title="${f.name}">${f.caption}</span>
      <ui5-select data-field="${f.name}" class="rf-kind" ?disabled=${!checked}>
        <ui5-option value="Exact" ${kind === 'Exact' ? 'selected' : ''}>精确匹配</ui5-option>
        <ui5-option value="EditDistance" ${kind === 'EditDistance' ? 'selected' : ''}>相似度</ui5-option>
      </ui5-select>
      <ui5-number-input data-field="${f.name}" class="rf-wt" value="${weight}" min="0" max="100" step="5" ?disabled=${!checked} style="width:90px;"></ui5-number-input>
    </div>`
  }).join('')
  // 存活字段多选
  const surviveChks = fields.map((f) => `<ui5-checkbox ?checked=${r.surviveFields.includes(f.name)} data-sv="${f.name}">${f.caption}</ui5-checkbox>`).join('')
  return `<div class="rule-bar">
    <label style="font-size:12px;color:var(--sapContent_LabelColor,#6a6d70);">查重规则</label>
    <ui5-select id="dcRule" style="min-width:200px;">${ruleOpts || '<ui5-option value="">（暂无规则）</ui5-option>'}</ui5-select>
    <ui5-button design="Transparent" icon="edit" id="dcRuleToggle">编辑</ui5-button>
    <ui5-button design="Transparent" icon="add" id="dcRuleNew">新建</ui5-button>
    <ui5-button design="Emphasized" icon="save" id="dcRuleSave" ?disabled=${!ruleHasFields()}>保存规则</ui5-button>
  </div>
  <div class="rule-fields">
    <div style="font-size:12px;color:var(--sapContent_LabelColor,#6a6d70);margin-bottom:4px;">查重字段（勾选参与比较的字段，配置权重与比较方式）</div>
    ${fieldRows || '<div class="hint">该字典无可选字段</div>'}
  </div>
  <div class="survive-row">
    <div style="font-size:12px;color:var(--sapContent_LabelColor,#6a6d70);">存活字段（合并时保留这些字段的值，参与对比展示）</div>
    <div class="chk-grid">${surviveChks}</div>
  </div>`
}

function newBlankRule() {
  return { id: '', ruleName: '新规则', dictCode: state.dictCode, targetTable: (state.dictMeta && state.dictMeta.tableName) || '', specs: [], clusterKeys: [], surviveFields: [], thresholds: { auto_merge: 95, review: 80 } }
}

function pickableFields() {
  if (!state.dictMeta || !state.dictMeta.columns) return []
  return state.dictMeta.columns.filter((c) => {
    if (c.isPrimaryKey) return false
    if (c.visible === false) return false
    // 过滤审计/治理列
    if (['create_by', 'create_time', 'update_by', 'update_time', 'lifecycle_status', 'published_version', 'sort_no', 'code'].includes(c.name)) return false
    return true
  })
}

function ruleHasFields() { return !!(state.rule && state.rule.specs && state.rule.specs.length) }

// ── 渲染：候选结果区 ────────────────────────────────────────────────────
function candHtml() {
  if (!state.result) {
    return `<section class="neo-panel"><div class="neo-panel-head"><div class="pt"><ui5-icon name="duplicate"></ui5-icon>查重候选</div></div>
      <div class="neo-panel-body"><div class="empty">${state.dictCode ? '选择目标记录后点击「查重」' : '请先选择数据字典'}</div></div></section>`
  }
  const cands = (state.result.candidates) || []
  const thr = state.result.thresholds || { auto_merge: 95, review: 80 }
  const rows = cands.map((c) => {
    const m = DECISION_META[c.decision] || { name: c.decision, tone: 'neutral' }
    const ck = state.victimIds.includes(c.recordId)
    const sel = state.selCand && String(state.selCand.recordId) === String(c.recordId)
    const rec = c.fields || {}
    return `<tr data-cand="${c.recordId}" class="${sel ? 'sel' : ''}">
      <td><ui5-checkbox ?checked=${ck} data-victim="${c.recordId}"></ui5-checkbox></td>
      <td>${rec.name || ''}</td><td class="muted">${rec.code || ''}</td>
      <td class="score">${c.score}</td>
      <td><cmx-status-tag tone="${m.tone}" variant="subtle" dot size="sm">${m.name}</cmx-status-tag></td>
    </tr>`
  }).join('')
  const candList = cands.length
    ? `<table class="tbl"><thead><tr><th></th><th>候选名称</th><th>代码</th><th>score</th><th>裁决</th></tr></thead><tbody>${rows}</tbody></table>`
    : `<div class="empty">未发现重复候选</div>`
  return `<section class="neo-panel">
    <div class="neo-panel-head">
      <div class="pt"><ui5-icon name="duplicate"></ui5-icon>查重候选（${cands.length}）</div>
      <cmx-toolbar>
        <ui5-button design="Emphasized" icon="combine" id="dcMerge" ?disabled=${state.victimIds.length === 0}>执行合并（${state.victimIds.length}）</ui5-button>
      </cmx-toolbar>
    </div>
    <div class="neo-panel-body">
      <div class="cand-wrap">${candList}</div>
      ${state.selCand ? cmpHtml() : ''}
    </div>
  </section>`
}

function cmpHtml() {
  const cand = state.selCand
  const targetF = (state.result && state.result.targetFields) || {}
  const candF = cand.fields || {}
  const r = state.rule || {}
  // 字段集 = specs 字段 ∪ surviveFields
  const specFields = (r.specs || []).map((s) => s.field)
  const fieldSet = Array.from(new Set([...specFields, ...(r.surviveFields || [])]))
  const allCols = (state.dictMeta && state.dictMeta.columns) || []
  const caption = (f) => (allCols.find((c) => c.name === f) || {}).caption || f
  const rows = fieldSet.map((f) => {
    const tv = fmt(targetF[f]); const cv = fmt(candF[f])
    const diff = !eqVal(tv, cv)
    return `<tr class="${diff ? 'diff' : 'same'}"><td>${caption(f)}</td><td>${tv}</td><td>${cv}</td>
      <td>${diff ? '<cmx-status-tag tone="negative" variant="subtle" size="sm">差异</cmx-status-tag>' : '<cmx-status-tag tone="positive" variant="subtle" size="sm">一致</cmx-status-tag>'}</td></tr>`
  }).join('')
  return `<div style="margin-top:14px;">
    <div class="cmp-tip">字段对比：当前目标记录 vs 候选记录「${candF.name || cand.recordId}」。目标记录默认为<b>主记录(master)</b>，勾选候选作<b>被合并方(victim)</b>，其值按存活规则并入主记录。</div>
    <table class="tbl"><thead><tr><th>字段</th><th>当前目标记录</th><th>候选记录</th><th>状态</th></tr></thead><tbody>${rows}</tbody></table>
  </div>`
}
const fmt = (v) => (v === null || v === undefined || v === '') ? '<span class="muted">—</span>' : String(v)
function eqVal(a, b) { return String(a) === String(b) }

// ── 渲染：合并历史区 ────────────────────────────────────────────────────
function histHtml() {
  const rows = state.histList.map((g) => {
    const members = g.memberNames || []
    const victims = members.filter((m) => String(m.id) !== String(g.master_id)).map((m) => m.name || m.code || m.id).join('、')
    const st = statusCn(g.status)
    const canUndo = g.status === 'reviewed'
    return `<tr><td>${g.masterName || g.master_id}</td><td>${victims}</td>
      <td><cmx-status-tag tone="${g.status === 'reviewed' ? 'success' : (g.status === 'unmerged' ? 'neutral' : 'negative')}" variant="subtle" dot size="sm">${st}</cmx-status-tag></td>
      <td>${g.score ?? ''}</td><td class="muted">${fmtTime(g.created_at)}</td>
      <td>${canUndo ? `<ui5-button design="Transparent" icon="reset" data-undo="${g.id}">还原</ui5-button>` : ''}</td></tr>`
  }).join('')
  const totalPages = Math.max(1, Math.ceil(state.histTotal / state.histPageSize))
  return `<section class="neo-panel">
    <div class="neo-panel-head"><div class="pt"><ui5-icon name="history"></ui5-icon>合并历史（共 ${state.histTotal} 条）</div></div>
    <div class="neo-panel-body">
      <div class="bar" style="margin-bottom:10px;">
        <ui5-select id="dcHistDict" style="min-width:160px;">
          <ui5-option value="" ${state.histDict === '' ? 'selected' : ''}>全部字典</ui5-option>
          <ui5-option value="supplier" ${state.histDict === 'supplier' ? 'selected' : ''}>供应商</ui5-option>
        </ui5-select>
        <ui5-input id="dcHistKw" placeholder="搜索主记录/被合并方名称" value="${state.histKw}" style="min-width:240px;flex:1 1 240px;"></ui5-input>
        <ui5-button design="Default" icon="search" id="dcHistSearch">查询</ui5-button>
      </div>
      ${state.histList.length
        ? `<table class="tbl"><thead><tr><th>主记录</th><th>被合并方</th><th>状态</th><th>score</th><th>合并时间</th><th>操作</th></tr></thead><tbody>${rows}</tbody></table>`
        : '<div class="empty">暂无合并记录</div>'}
      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:10px;">
        <span class="muted" style="font-size:12px;">第 ${state.histPage} / ${totalPages} 页</span>
        <div style="display:flex;gap:6px;">
          <ui5-button design="Transparent" icon="nav-left" id="dcHistPrev" ?disabled=${state.histPage <= 1}>上一页</ui5-button>
          <ui5-button design="Transparent" icon="nav-right" id="dcHistNext" ?disabled=${state.histPage >= totalPages}>下一页</ui5-button>
        </div>
      </div>
    </div>
  </section>`
}
const fmtTime = (s) => { if (!s) return ''; try { return new Date(s).toLocaleString('zh-CN', { hour12: false }) } catch { return s } }

function viewHtml() {
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">主数据查重</div>
      <div class="pg-sub">识别一物多码：选择字典与记录，比对字段后合并重复项</div></div>
    ${condHtml()}${candHtml()}${histHtml()}
  </div>`
}

// ── 事件绑定 ────────────────────────────────────────────────────────────
function bind(root) {
  const C = cmx()
  // 字典选择
  const dcDict = root.querySelector('#dcDict')
  if (dcDict && C.CmxColumn) {
    dcDict.configure({
      dictCode: '_selector', idCol: 'dictCode', labelCol: 'dictName',
      helpLayout: 'grid', dataSource: dictSource(), dictTitle: '选择数据字典',
      columns: [new C.CmxColumn({ id: 'dictCode', caption: '字典码', dataType: 'VARCHAR', width: '140px' }), new C.CmxColumn({ id: 'dictName', caption: '字典名称', dataType: 'VARCHAR' })],
    })
    dcDict.addEventListener('cmx-dict-change', (e) => { const d = e.detail || {}; onDictChange(d.id || '') })
  }
  // 目标记录选择
  const dcRecord = root.querySelector('#dcRecord')
  if (dcRecord && C.CmxColumn && state.dictCode) {
    const ds = recordSource()
    if (ds) {
      const cols = []
      const mc = (state.dictMeta && state.dictMeta.columns) || []
      ;['code', 'name', 'credit_code'].forEach((id) => { const c = mc.find((x) => x.name === id); if (c) cols.push(new C.CmxColumn({ id: c.name, caption: c.caption, dataType: 'VARCHAR', width: c.name === 'name' ? '200px' : '140px' })) })
      dcRecord.configure({ dictCode: state.dictCode, idCol: 'id', labelCol: 'name', helpLayout: 'grid', dataSource: ds, dictTitle: '选择目标记录', columns: cols })
      dcRecord.addEventListener('cmx-dict-change', (e) => { const d = e.detail || {}; onRecordChange(d) })
    }
  }
  // 查重按钮
  root.querySelector('#dcFind')?.addEventListener('click', () => runFind().catch((e) => cmx().cmxError?.(`查重失败：${e.message}`)))
  // 规则下拉切换
  root.querySelector('#dcRule')?.addEventListener('change', (e) => {
    const id = e.target.value; const r = state.rules.find((x) => String(x.id) === String(id))
    if (r) { state.rule = normalizeRule(r); refresh() }
  })
  // 编辑/新建/保存
  root.querySelector('#dcRuleNew')?.addEventListener('click', () => { state.rule = newBlankRule(); refresh() })
  root.querySelector('#dcRuleToggle')?.addEventListener('click', () => { /* 编辑模式：当前编辑器即编辑态 */ })
  // 字段勾选/权重/比较方式
  root.querySelectorAll('.rf-chk').forEach((ck) => ck.addEventListener('change', () => syncRuleFromUi(root)))
  root.querySelectorAll('.rf-kind').forEach((s) => s.addEventListener('change', () => syncRuleFromUi(root)))
  root.querySelectorAll('.rf-wt').forEach((w) => w.addEventListener('change', () => syncRuleFromUi(root)))
  root.querySelectorAll('[data-sv]').forEach((ck) => ck.addEventListener('change', () => syncRuleFromUi(root)))
  // 保存规则
  root.querySelector('#dcRuleSave')?.addEventListener('click', () => saveRule().catch((e) => cmx().cmxError?.(`保存规则失败：${e.message}`)))
  // 候选行点击对比 + 勾选 victim
  root.querySelectorAll('tr[data-cand]').forEach((tr) => {
    tr.addEventListener('click', (e) => {
      if (e.target.closest('ui5-checkbox')) return // 点 checkbox 不触发对比
      const id = tr.dataset.cand
      const c = (state.result.candidates || []).find((x) => String(x.recordId) === String(id))
      state.selCand = c || null; refresh()
    })
  })
  root.querySelectorAll('[data-victim]').forEach((ck) => ck.addEventListener('change', (e) => {
    const id = Number(ck.dataset.victim)
    if (ck.checked) { if (!state.victimIds.includes(id)) state.victimIds.push(id) }
    else { state.victimIds = state.victimIds.filter((x) => x !== id) }
    refresh()
  }))
  // 执行合并
  root.querySelector('#dcMerge')?.addEventListener('click', () => doMerge().catch((e) => cmx().cmxError?.(`合并失败：${e.message}`)))
  // 历史
  root.querySelector('#dcHistDict')?.addEventListener('change', (e) => { state.histDict = e.target.value; state.histPage = 1; loadHist().then(refresh) })
  const hk = root.querySelector('#dcHistKw')
  hk?.addEventListener('change', (e) => { state.histKw = e.target.value })
  hk?.addEventListener('keydown', (e) => { if (e.key === 'Enter') { state.histPage = 1; loadHist().then(refresh) } })
  root.querySelector('#dcHistSearch')?.addEventListener('click', () => { state.histPage = 1; loadHist().then(refresh) })
  root.querySelector('#dcHistPrev')?.addEventListener('click', () => { if (state.histPage > 1) { state.histPage--; loadHist().then(refresh) } })
  root.querySelector('#dcHistNext')?.addEventListener('click', () => { state.histPage++; loadHist().then(refresh) })
  root.querySelectorAll('[data-undo]').forEach((b) => b.addEventListener('click', () => doUndo(b.dataset.undo).catch((e) => cmx().cmxError?.(`还原失败：${e.message}`))))
}

// 从 UI 控件同步规则到 state.rule
function syncRuleFromUi(root) {
  if (!state.rule) state.rule = newBlankRule()
  const fields = pickableFields()
  const specs = []
  fields.forEach((f) => {
    const ck = root.querySelector(`.rf-chk[data-field="${f.name}"]`)
    if (ck && ck.checked) {
      const kindSel = root.querySelector(`.rf-kind[data-field="${f.name}"]`)
      const wtInput = root.querySelector(`.rf-wt[data-field="${f.name}"]`)
      specs.push({ field: f.name, weight: Number((wtInput && wtInput.value) || 0), kind: (kindSel && kindSel.value) || 'Exact' })
    }
  })
  const surviveFields = []
  root.querySelectorAll('[data-sv]').forEach((ck) => { if (ck.checked) surviveFields.push(ck.dataset.sv) })
  state.rule.specs = specs
  state.rule.clusterKeys = specs.map((s) => s.field)
  state.rule.surviveFields = surviveFields
  state.rule.targetTable = (state.dictMeta && state.dictMeta.tableName) || state.rule.targetTable
  state.ruleDirty = true
  // 只刷新查重按钮可用性（避免重渲染丢焦点）
  const findBtn = root.querySelector('#dcFind'); if (findBtn) findBtn.disabled = !state.dictCode || !state.targetId || !ruleHasFields()
}

async function onDictChange(dictCode) {
  state.dictCode = dictCode
  state.dictMeta = null; state.rule = null; state.rules = []
  state.targetId = null; state.targetRow = null; state.result = null; state.selCand = null; state.victimIds = []
  if (!dictCode) { refresh(); return }
  await loadDictMeta()
  await loadRules()
  refresh()
}

function onRecordChange(detail) {
  if (detail.id == null || detail.id === '') { state.targetId = null; state.targetRow = null }
  else { state.targetId = detail.id; state.targetRow = detail.plain || detail.row || null }
  const root = rootEl; if (root) { const b = root.querySelector('#dcFind'); if (b) b.disabled = !state.dictCode || !state.targetId || !ruleHasFields() }
}

// ── 业务动作 ────────────────────────────────────────────────────────────
async function runFind() {
  if (!state.dictCode || !state.targetId || !ruleHasFields()) { cmx().cmxWarn?.('请先选择字典、目标记录，并配置查重字段'); return }
  const r = state.rule
  const payload = {
    dictCode: state.dictCode, recordId: Number(state.targetId), targetTable: r.targetTable,
    specs: r.specs, clusterKeys: r.clusterKeys, surviveFields: r.surviveFields,
  }
  state.result = await apiPost('/api/mdm/records/find-duplicates', payload, state.dbId)
  state.selCand = null; state.victimIds = []
  refresh()
  cmx().cmxInfo?.(`查重完成，发现 ${((state.result && state.result.candidates) || []).length} 个候选`)
}

async function doMerge() {
  const M = cmx()
  if (!state.victimIds.length) { M.cmxWarn?.('请先勾选要合并的候选'); return }
  const r = state.rule; if (!r) return
  const targetName = (state.targetRow && (state.targetRow.name || state.targetRow.code)) || state.targetId
  const victims = state.victimIds.map((id) => {
    const c = (state.result.candidates || []).find((x) => String(x.recordId) === String(id))
    return (c && c.fields && (c.fields.name || c.fields.code)) || id
  })
  const ok = await M.cmxConfirm?.({
    title: '确认合并', danger: true,
    message: `确认执行合并？\n\n保留为主记录(master)：${targetName}\n将被废弃标记已合并(victim)：${victims.join('、')}\n\n说明：被合并方可完整还原；主记录被合并带过来的字段值不会回退，如需修正请走变更单。`,
  })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests', {
    dictCode: state.dictCode, masterId: Number(state.targetId), victimIds: state.victimIds,
    targetTable: r.targetTable, surviveFields: r.surviveFields,
  }, state.dbId)
  M.cmxInfo?.('合并成功')
  // 刷新候选（剔除已合并）+ 历史
  state.victimIds = []; state.selCand = null
  await runFind().catch(() => {})
  await loadHist()
  refresh()
}

async function doUndo(mergeId) {
  const M = cmx()
  const ok = await M.cmxConfirm?.({
    title: '确认还原', message: '还原会让被合并方完整恢复（状态、明细、交叉引用）；但主记录被合并带过来的字段值不会回退。是否继续？',
  })
  if (ok === false) return
  await apiPost('/api/mdm/merge-requests/undo', { mergeId: Number(mergeId) }, state.dbId)
  M.cmxInfo?.('已还原')
  await loadHist(); refresh()
}

async function saveRule() {
  const M = cmx()
  if (!ruleHasFields()) { M.cmxWarn?.('请至少勾选一个查重字段'); return }
  if (!state.rule.ruleName || state.rule.ruleName === '新规则') {
    const name = await M.cmxPrompt?.('请输入规则名称') // 若无 cmxPrompt 则用默认
    state.rule.ruleName = name || `规则_${Date.now()}`
  }
  const r = state.rule
  const payload = {
    id: r.id || '', ruleName: r.ruleName, dictCode: state.dictCode, targetTable: r.targetTable,
    specs: r.specs, clusterKeys: r.clusterKeys, surviveFields: r.surviveFields, thresholds: r.thresholds,
  }
  const saved = await apiPost('/api/mdm/match-configs', payload, state.dbId)
  if (saved && saved.id) state.rule.id = saved.id
  state.ruleDirty = false
  await loadRules()
  M.cmxInfo?.('规则已保存')
  refresh()
}

async function loadHist() {
  const q = new URLSearchParams({ page: String(state.histPage), pageSize: String(state.histPageSize) })
  if (state.histDict) q.set('dictCode', state.histDict)
  const d = await apiGet('/api/mdm/merge-requests?' + q.toString(), state.dbId)
  state.histList = (d && d.list) || []
  state.histTotal = (d && d.total) || 0
  // 关键字二次过滤（后端暂不支持名称搜索，前端按 masterName/memberNames 过滤）
  if (state.histKw) {
    const kw = state.histKw.toLowerCase()
    state.histList = state.histList.filter((g) => {
      const names = [g.masterName || '', ...((g.memberNames || []).map((m) => m.name || ''))].join(' ').toLowerCase()
      return names.includes(kw)
    })
  }
}

// ── 渲染循环 ────────────────────────────────────────────────────────────
let rootEl = null; let currentHost = null
function refresh() {
  const host = currentHost; if (!host) return
  const root = host.renderRoot || host.shadowRoot; if (!root) return
  root.innerHTML = `<style>${styleCss()}</style>${viewHtml()}`
  rootEl = root
  bind(root)
}
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
      const props = (ctx && ctx.props) || {}
      state.domain = props.domain || 'basic'
      state.application = props.application || 'dataplatform'
      state.module = props.module || 'mdm'
      state.dbId = props.dbId || props.db_id || ''
      // 初始加载历史（独立常驻）
      try { await loadHist() } catch (e) { console.error('[duplicate-check] loadHist fail', e) }
      if (host) whenRendered(host, '.pg', (r) => { rootEl = r; bind(r) })
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
