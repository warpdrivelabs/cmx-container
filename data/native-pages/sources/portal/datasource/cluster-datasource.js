/**
 * 集群数据源浏览（cluster datasource browser）—— native_pages 三区只读功能。
 *
 * explorer：顶部 domain/app（一行）+ module（单独一行）三个 DAM 下拉（含「全部」）；
 *           下方 <cmx-ignite-list> 展示 /api/sys-datasource/list 的数据库列表（按 db_type 设图标）；
 *           再下方 <cmx-ui5-form> 只读展示选中数据库的详细配置。
 * content：三个视图（数据字典 DCT / 业务单据 DOC / 上下文档案 profile），
 *           顶部标题区下拉「选具体档案 + 版本」，下方只读展示其详情（参考三大功能的展示，去掉编辑）。
 * property：展示 content 区当前选中项的只读详情。
 *
 * 跨区通信：workspace.context（set/on('change')）。数据组件（CmxDataSet / cmx-ignite-list /
 *           cmx-ui5-form / CmxColumnModel）经 globalThis.__cmxDataComp 取用——原生页由 Blob import
 *           加载，无法裸引 'cmx-data-comp'。
 *
 * 说明：数据源记录当前无 domain/app/module 字段（将来会加），故 DAM 下拉只过滤 content 区，
 *       数据库列表始终展示集群全部数据源（按用户要求"没有定义的按全部处理"）。
 */

// ─── 共享状态（模块级单例，三区共用） ─────────────────────────────────────
const state = {
  dam: { domains: [], apps: [], modules: [] },
  filter: { domain: '', app: '', module: '' }, // '' = 全部
  datasources: [],           // /api/sys-datasource/list 的 rows
  dsLoading: false,
  selectedDsId: '',          // 选中的数据源 id
  message: '',
  hosts: new Set(),
}

// db_type → 图标名 + 中文标签（cmx-ignite-list 的 row.icon / 展示用）
const DB_TYPE_META = {
  postgres:   { icon: 'database', label: 'PostgreSQL', short: 'PG' },
  postgresql: { icon: 'database', label: 'PostgreSQL', short: 'PG' },
  pg:         { icon: 'database', label: 'PostgreSQL', short: 'PG' },
  mysql:      { icon: 'database', label: 'MySQL', short: 'MySQL' },
  mariadb:    { icon: 'database', label: 'MariaDB', short: 'MariaDB' },
  oracle:     { icon: 'database', label: 'Oracle', short: 'Oracle' },
  sqlserver:  { icon: 'database', label: 'SQL Server', short: 'MSSQL' },
  mssql:      { icon: 'database', label: 'SQL Server', short: 'MSSQL' },
  sqlite:     { icon: 'database', label: 'SQLite', short: 'SQLite' },
  mongodb:    { icon: 'tree', label: 'MongoDB', short: 'Mongo' },
  redis:      { icon: 'multiselect-all', label: 'Redis', short: 'Redis' },
}
const dbTypeMeta = (t) => DB_TYPE_META[String(t || '').toLowerCase()] || { icon: 'database', label: String(t || '未知'), short: String(t || '?') }

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;')

const cmxClasses = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

async function apiJson (url, options = {}) {
  const res = await fetch(url, {
    ...options,
    headers: { Accept: 'application/json', ...(options.headers || {}) },
    credentials: 'same-origin',
  })
  if (!res.ok) {
    let msg = `HTTP ${res.status}`
    try { const j = await res.json(); if (j && j.error) msg = j.error } catch {}
    throw new Error(msg)
  }
  return res.status === 204 ? {} : res.json()
}

// ─── 数据加载 ──────────────────────────────────────────────────────────────
async function loadDam () {
  try {
    const dam = await apiJson('/api/registry/dam')
    state.dam = {
      domains: Array.isArray(dam.domains) ? dam.domains : [],
      apps: Array.isArray(dam.apps) ? dam.apps : (Array.isArray(dam.applications) ? dam.applications : []),
      modules: Array.isArray(dam.modules) ? dam.modules : [],
    }
  } catch (err) { state.message = 'DAM 加载失败：' + err.message }
}

async function loadDatasources () {
  state.dsLoading = true
  try {
    // 通用 CRUD list 是 POST；返回 { id, schema, rows:[...] }（经 fetch 拦截器拆 ApiResp）。
    const data = await apiJson('/api/sys-datasource/list', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({}),
    })
    const rows = Array.isArray(data?.rows) ? data.rows : (Array.isArray(data?.items) ? data.items : (Array.isArray(data) ? data : []))
    // 归一：补 id（cmx-ignite-list 需要）+ 展示字段。
    state.datasources = rows.map((r, i) => {
      const meta = dbTypeMeta(r.db_type)
      const name = r.db_id || r.id || `ds_${i}`
      return {
        ...r,
        id: String(r.id || r.db_id || `ds_${i}`),
        icon: meta.icon,
        title: name,
        subtitle: `${meta.label}${r.default_flag === 1 ? ' · 默认' : ''}${r.status === 0 ? ' · 已禁用' : ''}`,
        _typeLabel: meta.label,
      }
    })
    if (!state.selectedDsId && state.datasources.length) state.selectedDsId = state.datasources[0].id
  } catch (err) {
    state.datasources = []
    state.message = '数据源加载失败：' + err.message
  } finally { state.dsLoading = false }
}

// ─── 渲染工具 ──────────────────────────────────────────────────────────────
function refreshAll () {
  for (const host of Array.from(state.hosts)) {
    if (host && host.isConnected) renderInto(host)
    else state.hosts.delete(host)
  }
}

function mount (ctx, html, after) {
  const bindWhenReady = (tries = 0) => {
    if (ctx.host) state.hosts.add(ctx.host)
    const root = ctx.host?.renderRoot || ctx.host?.shadowRoot?.querySelector('.native-page-root')
    if (root && root.isConnected && typeof after === 'function') { after(root); return }
    if (tries < 20) requestAnimationFrame(() => bindWhenReady(tries + 1))
  }
  requestAnimationFrame(() => bindWhenReady())
  return `${styleHtml()}${html}`
}

function viewOf (host) {
  const v = host?.getAttribute?.('view') || ''
  if (v === 'explorer') return v
  if (v.startsWith('content') || v.startsWith('property')) return v
  return 'content-dct'
}

function renderInto (host) {
  const root = host?.renderRoot || host?.shadowRoot?.querySelector('.native-page-root')
  if (!root) return
  const view = viewOf(host)
  let html = ''
  if (view === 'explorer') html = explorerHtml()
  else if (view.startsWith('property')) html = propertyHtml(view.replace('property-', '') || 'dct')
  else html = contentHtml(view.replace('content-', '') || 'dct')
  root.innerHTML = `${styleHtml()}${html}`
  bindView(root, view)
}

// ─── explorer 区 ───────────────────────────────────────────────────────────
function damSelectsHtml () {
  const domains = state.dam.domains || []
  const apps = (state.dam.apps || []).filter((a) => !state.filter.domain || a.domain === state.filter.domain)
  const modules = (state.dam.modules || []).filter((m) =>
    (!state.filter.domain || m.domain === state.filter.domain) &&
    (!state.filter.app || (m.application || m.app) === state.filter.app))
  const opt = (val, label, sel) => `<option value="${esc(val)}" ${sel ? 'selected' : ''}>${esc(label)}</option>`
  return `
    <div class="cds-dam-row">
      <select class="cds-select" data-dam="domain" title="域">
        ${opt('', '全部域', !state.filter.domain)}
        ${domains.map((d) => opt(d.id, d.label || d.name || d.id, d.id === state.filter.domain)).join('')}
      </select>
      <select class="cds-select" data-dam="app" title="应用">
        ${opt('', '全部应用', !state.filter.app)}
        ${apps.map((a) => opt(a.id, a.label || a.name || a.id, a.id === state.filter.app)).join('')}
      </select>
    </div>
    <div class="cds-dam-row cds-dam-row-module">
      <select class="cds-select" data-dam="module" title="模块">
        ${opt('', '全部模块', !state.filter.module)}
        ${modules.map((m) => opt(m.id || m.module, m.label || m.name || m.id || m.module, (m.id || m.module) === state.filter.module)).join('')}
      </select>
    </div>`
}

function explorerHtml () {
  const selected = state.datasources.find((d) => d.id === state.selectedDsId) || null
  return `
    <div class="cds-neo cds-wrap">
      <div class="cds-banner"><ui5-icon name="database" class="cds-banner-ic"></ui5-icon><span class="cds-banner-title">集群数据源</span><span class="cds-kpi">${state.datasources.length}</span></div>
      <div class="cds-dam">${damSelectsHtml()}</div>
      <div class="cds-section-label">数据库列表<span class="cds-hint">（集群全部）</span></div>
      <div class="cds-list-region" data-ds-list-host>
        ${state.dsLoading ? '<div class="cds-empty">加载中…</div>'
          : (state.datasources.length ? '<cmx-ignite-list data-cmx-layout="card" data-cmx-density="compact" id="cds-list"></cmx-ignite-list>' : '<div class="cds-empty"><ui5-icon name="database"></ui5-icon>暂无已配置数据源</div>')}
      </div>
      <div class="cds-section-label">数据库属性<span class="cds-hint">（只读）</span></div>
      <div class="cds-detail-region">
        ${selected ? '<cmx-ui5-form id="cds-form" data-cmx-skin="neo"></cmx-ui5-form>' : '<div class="cds-empty">选择数据库查看配置</div>'}
      </div>
      ${state.message ? `<div class="cds-msg">${esc(state.message)}</div>` : ''}
    </div>`
}

// ─── content 区（三视图共用）：直接嵌入真实功能组件（只读） ───────────────────
// DAM 过滤经属性传入；同页 content↔property 用 data-bus-scope 共享一条私有总线，与真实功能页隔离。
const BUS_SCOPE = { dct: 'cds-dct', doc: 'cds-doc', profile: 'cds-profile' }

function damFilterAttrs () {
  const f = state.filter
  return `${f.domain ? ` data-filter-domain="${esc(f.domain)}"` : ''}${f.app ? ` data-filter-app="${esc(f.app)}"` : ''}${f.module ? ` data-filter-module="${esc(f.module)}"` : ''}`
}

/** content 视图：整块交给真实组件渲染（100% 原样，只读）。 */
function contentHtml (tab) {
  const scope = BUS_SCOPE[tab]
  const filters = damFilterAttrs()
  if (tab === 'profile') {
    return `<div class="cds-embed-host"><portal-context-profile-manager data-embed data-readonly data-bus-scope="${scope}"${filters}></portal-context-profile-manager></div>`
  }
  const kind = tab === 'doc' ? 'DOC' : 'DCT'
  return `<div class="cds-embed-host"><portal-definition-manager data-kind="${kind}" data-embed data-readonly data-bus-scope="${scope}"${filters}></portal-definition-manager></div>`
}

// ─── property 区：三 tab，各嵌真实检查器（与同名 content tab 同 scope 共享总线，只读） ──
function propertyHtml (tab) {
  const scope = BUS_SCOPE[tab] || BUS_SCOPE.dct
  let inspector
  if (tab === 'profile') inspector = `<portal-context-profile-inspector data-readonly data-bus-scope="${scope}"></portal-context-profile-inspector>`
  else inspector = `<portal-definition-inspector data-kind="${tab === 'doc' ? 'DOC' : 'DCT'}" data-readonly data-bus-scope="${scope}"></portal-definition-inspector>`
  return `<div class="cds-embed-host cds-prop-host">${inspector}</div>`
}

// ─── 绑定（事件 + 数据组件挂载） ────────────────────────────────────────────
function bindView (root, view) {
  if (view === 'explorer') return bindExplorer(root)
  // content / property：各自嵌入的真实组件（及其检查器）自管，无需页面级绑定。
}

function bindExplorer (root) {
  root.querySelectorAll('[data-dam]').forEach((sel) => {
    sel.addEventListener('change', () => {
      const kind = sel.getAttribute('data-dam')
      const val = sel.value
      if (kind === 'domain') { state.filter.domain = val; state.filter.app = ''; state.filter.module = '' }
      else if (kind === 'app') { state.filter.app = val; state.filter.module = '' }
      else state.filter.module = val
      // DAM 变化 → content 区各 embed 组件重挂（带新过滤），property 跟随。
      refreshAll()
    })
  })
  // 数据源列表 → cmx-ignite-list + CmxDataSet
  const listEl = root.querySelector('#cds-list')
  if (listEl) {
    const { CmxDataSet } = cmxClasses()
    if (CmxDataSet) {
      const ds = new CmxDataSet()
      ds.setRows(state.datasources)
      listEl.setDataSet(ds)
      if (state.selectedDsId && ds.moveToId) ds.moveToId(state.selectedDsId)
    } else {
      listEl.setDataSet(state.datasources) // 回退：直接给行数组
    }
    listEl.addEventListener('cmx-row-selected', (e) => {
      state.selectedDsId = e.detail?.id || ''
      bindDatasourceForm(root) // 只更新详情表单，不整块重渲染（保列表滚动/选中）
    })
  }
  bindDatasourceForm(root)
}

function bindDatasourceForm (root) {
  const form = root.querySelector('#cds-form')
  if (!form) return
  const ds = state.datasources.find((d) => d.id === state.selectedDsId)
  if (!ds) return
  const { CmxColumnModel, CmxColumn } = cmxClasses()
  const fieldDefs = [
    { id: 'db_id', caption: '数据源标识' }, { id: '_typeLabel', caption: '数据库类型' },
    { id: 'db_url', caption: '连接URL' }, { id: 'db_schema', caption: '模式(schema)' },
    { id: 'description', caption: '描述' },
    { id: 'default_flag', caption: '默认数据源', map: (v) => (v === 1 ? '是' : '否') },
    { id: 'status', caption: '状态', map: (v) => (v === 0 ? '禁用' : '启用') },
    { id: 'source', caption: '来源' },
    { id: 'max_connections', caption: '最大连接数' }, { id: 'min_connections', caption: '最小空闲连接' },
    { id: 'connect_timeout', caption: '连接超时(s)' }, { id: 'idle_timeout', caption: '空闲超时(s)' },
    { id: 'max_lifetime', caption: '最大生命周期(s)' },
    { id: 'health_check_interval', caption: '健康检查间隔(s)' }, { id: 'health_check_timeout', caption: '健康检查超时(s)' },
  ]
  // 展示行（把枚举/布尔映射成中文；全部 readonly）
  const row = {}
  for (const f of fieldDefs) row[f.id] = f.map ? f.map(ds[f.id]) : ds[f.id]
  if (CmxColumnModel && CmxColumn && typeof form.setColumnModel === 'function') {
    try {
      // 每列建成只读 CmxColumn（edit.mode='readonly' → 适配器输出 form field.readonly=true）。
      const members = fieldDefs.map((f) => new CmxColumn({ id: f.id, caption: f.caption, edit: { mode: 'readonly' } }))
      const model = new CmxColumnModel({ caption: '数据源配置', members })
      if (typeof form.setLayout === 'function') form.setLayout('S1 M1 L2 XL2')
      form.setColumnModel(model)
      form.setDataSet(row)
      return
    } catch (err) { /* 回退到 KV 表 */ state.message = '表单渲染回退：' + (err?.message || err) }
  }
  // 回退：无 CmxColumnModel 时用只读 KV 表替换表单容器
  const region = form.parentElement
  if (region) region.innerHTML = kvTable(fieldDefs.map((f) => [f.caption, row[f.id]]))
}

/** 只读 KV 表（数据源属性回退用）。 */
function kvTable (rows) {
  return `<table class="cds-kv">${rows.filter((r) => r).map(([k, v]) => `<tr><th>${esc(k)}</th><td>${v == null || v === '' ? '<span class="cds-muted">—</span>' : esc(v)}</td></tr>`).join('')}</table>`
}

// ─── 皮肤 ──────────────────────────────────────────────────────────────────
function styleHtml () {
  return `<style>
    .cds-wrap{display:flex;flex-direction:column;height:100%;min-height:0;box-sizing:border-box;font:13px/1.45 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#f5f6f7);overflow:hidden}
    .cds-banner{display:flex;align-items:center;gap:8px;flex:0 0 40px;height:40px;padding:0 12px;box-sizing:border-box;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);background:var(--sapList_HeaderBackground,#eef2f6)}
    .cds-banner-ic{width:1.05rem;height:1.05rem;color:var(--sapInformationColor,#0a6ed1)}
    .cds-banner-title{font-weight:700;font-size:13px;flex:1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
    .cds-kpi{font-size:11px;font-weight:700;background:var(--sapInformationBackground,#eaf4ff);color:var(--sapInformationColor,#0a6ed1);border-radius:9px;padding:1px 9px}
    .cds-dam{padding:8px 10px;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#e5e5e5);display:flex;flex-direction:column;gap:6px;flex:0 0 auto}
    .cds-dam-row{display:grid;grid-template-columns:1fr 1fr;gap:6px}
    .cds-dam-row-module{grid-template-columns:1fr}
    .cds-select{height:28px;border:1px solid var(--sapField_BorderColor,#89919a);border-radius:4px;padding:0 6px;background:var(--sapField_Background,#fff);color:var(--sapField_TextColor,var(--sapTextColor,#1d2d3e));font-size:12px;box-sizing:border-box;width:100%}
    .cds-section-label{padding:6px 12px 2px;font-size:11px;font-weight:700;color:var(--sapContent_LabelColor,#6a6d70);text-transform:uppercase;letter-spacing:.03em}
    .cds-section-label .cds-hint{font-weight:400;text-transform:none;letter-spacing:0;margin-left:4px}
    .cds-list-region{flex:1 1 45%;min-height:80px;overflow:auto;padding:2px 6px;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#e5e5e5)}
    .cds-detail-region{flex:1 1 55%;min-height:80px;overflow:auto;padding:6px 10px}
    #cds-list{--cmx-list-item-gap:2px}
    .cds-list-region .cmx-list-item{border:1px solid transparent;border-radius:6px;padding:6px 8px;gap:8px}
    .cds-list-region .cmx-list-item:hover{background:var(--sapList_Hover_Background,#f5f6f7)}
    .cds-list-region .cmx-list-item.is-selected{border-color:var(--sapContent_FocusColor,#0a6ed1);background:color-mix(in srgb,var(--sapInformationColor,#0a6ed1) 10%,var(--sapList_Background,#fff))}
    .cds-list-region .cmx-list-item__ic ui5-icon{width:1.1rem;height:1.1rem;color:var(--sapInformationColor,#0a6ed1)}
    .cds-list-region .cmx-list-item__title{font-weight:600;font-size:12px}
    .cds-list-region .cmx-list-item__desc{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70)}
    .cds-embed-host{height:100%;min-height:0;display:flex;flex-direction:column;overflow:hidden}
    .cds-embed-host>portal-definition-manager,.cds-embed-host>portal-context-profile-manager,.cds-embed-host>portal-definition-inspector,.cds-embed-host>portal-context-profile-inspector{flex:1 1 auto;min-height:0;display:block}
    .cds-kv{width:100%;border-collapse:collapse;font-size:12px}
    .cds-kv th{text-align:left;width:120px;color:var(--sapContent_LabelColor,#6a6d70);font-weight:600;padding:5px 10px;vertical-align:top;background:color-mix(in srgb,var(--sapList_HeaderBackground,#f7f7f7) 40%,transparent)}
    .cds-kv td{padding:5px 10px;word-break:break-all}
    .cds-muted{color:var(--sapContent_LabelColor,#9a9d9f)}
    .cds-empty{padding:18px 12px;color:var(--sapContent_LabelColor,#6a6d70);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;text-align:center;min-height:60px}
    .cds-empty ui5-icon{width:1.4rem;height:1.4rem;opacity:.7}
    .cds-msg{padding:6px 12px;font-size:12px;color:var(--sapNegativeTextColor,#b00)}
  </style>`
}

// ─── 页面入口：一个 id 服务五个 view ───────────────────────────────────────
export default {
  defaultView: 'content-dct',
  views: {
    async explorer (ctx) {
      if (!state.dam.domains.length) await loadDam()
      await loadDatasources()
      return mount(ctx, explorerHtml(), (root) => bindView(root, 'explorer'))
    },
    // content 三视图：整块交给真实功能组件（只读自管列表/详情），此处只需 DAM 供过滤属性。
    async 'content-dct' (ctx) {
      if (!state.dam.domains.length) await loadDam()
      return mount(ctx, contentHtml('dct'), (root) => bindView(root, 'content-dct'))
    },
    async 'content-doc' (ctx) {
      if (!state.dam.domains.length) await loadDam()
      return mount(ctx, contentHtml('doc'), (root) => bindView(root, 'content-doc'))
    },
    async 'content-profile' (ctx) {
      if (!state.dam.domains.length) await loadDam()
      return mount(ctx, contentHtml('profile'), (root) => bindView(root, 'content-profile'))
    },
    // property 三视图：各嵌对应检查器（与同名 content tab 同 scope 联动）。
    async 'property-dct' (ctx) {
      return mount(ctx, propertyHtml('dct'), (root) => bindView(root, 'property-dct'))
    },
    async 'property-doc' (ctx) {
      return mount(ctx, propertyHtml('doc'), (root) => bindView(root, 'property-doc'))
    },
    async 'property-profile' (ctx) {
      return mount(ctx, propertyHtml('profile'), (root) => bindView(root, 'property-profile'))
    },
  },
}
