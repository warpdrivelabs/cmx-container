/**
 * doc-loader — 通用业务单据加载页（元数据驱动，native_pages）。
 *
 * 纯通用：**不认识任何具体单据**。层数 N、各层列、主从关系全部来自 `/api/doc/meta`，
 * 动态渲染 N 个 grid、动态建 CmxMasterSlave schema 与列模型。会计凭证只是"传入凭证的
 * 单据定义坐标"的一组 props —— 换任何 L1..LN 单据定义即可加载，本页零改动。
 *
 * 数据通道由 props 决定（一套页面覆盖全部出口）：
 *   props = { domain, application, module, file, dbId?, apiPath?, binary? }
 *     - apiPath 缺省 /api/doc/data/sqlx-dataset-json（老 DataSet+JSON）
 *     - binary:true → 走 msgpack 二进制通道（/api/doc/data/tokio-zmc-msgpack 等），页面用 arrayBuffer+decode
 *     - 其它 apiPath（/api/doc/data/sqlx-zmc-msgpack, /api/doc/data/tokio-zmc-json ...）按需
 *
 * 契约：export default { defaultView:'content', views:{ content(ctx) } }；ctx.props 来自菜单。
 * CMX 类 + 通用助手（buildMasterSlaveSchema/layerPaths/buildColumnModel/loadDocData）经
 * globalThis.__cmxDataComp 取用。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

/* 每个页面实例一份状态（同一模块可能被多区域复用，state 在 content(ctx) 里重置）。 */
const state = {
  def: null, meta: null, ms: null, collector: null, paths: [], loading: false,
  // 每层 UI 查询状态：layerId → { conds:[{col,op,value}], sorts:[{col,desc}], limit, offset, cursor, nextCursor }
  layerState: {},
  grids: {},
  pageSize: 50,
}

/* ── 响应归一（兼容门户已拆信封 / 原始信封两种形态） ─────────────────────── */
const { apiGet, apiPost } = globalThis.__cmxDataComp // 共享 fetch 封装（cmx-data-comp/lib/cmx-page-helpers.js；信封解包+结构化错误）

/** GET JSON（带 db_id 头），归一取业务数据。 */

/** POST JSON。 */

function qs (def, extra = {}) {
  return new URLSearchParams({
    domain: def.domain, application: def.application, module: def.module, file: def.file, ...extra,
  }).toString()
}

/* ── 样式 + 骨架（grid 容器在装载后按层数动态填充） ─────────────────────── */
function styleHtml () {
  return `<style>
.dl-root{display:flex;flex-direction:column;height:100%;gap:6px;padding:8px;box-sizing:border-box;min-width:0}
.dl-bar{flex:0 0 auto}
.dl-title{font-weight:600}
.dl-msg{margin-left:12px;color:#0854a0;font-size:12px}
.dl-grids{flex:1;display:flex;flex-direction:column;gap:6px;min-height:0;min-width:0;overflow:auto}
.dl-pane{flex:1 1 0;display:flex;flex-direction:column;min-height:0;min-width:0;overflow:hidden;border:1px solid #e5e5e5;border-radius:4px}
.dl-pane-head{display:flex;align-items:center;gap:8px;padding:2px 6px;background:#f7f7f7;flex-wrap:wrap;font-size:12px}
.dl-pane-title{font-weight:600}
.dl-grid{display:block;width:100%;flex:1;min-height:120px;min-width:0}
.dl-filter{display:flex;gap:4px;align-items:center;flex-wrap:wrap;padding:2px 6px;background:#fcfcfc;border-top:1px dashed #eee;font-size:12px}
.dl-cond{display:flex;gap:2px;align-items:center;background:#eef4ff;border-radius:3px;padding:1px 3px}
.dl-cond select,.dl-cond input{font-size:12px;padding:1px 3px;max-width:130px}
.dl-btn{font-size:12px;padding:1px 8px;cursor:pointer;border:1px solid #bbb;border-radius:3px;background:#fff}
.dl-btn:hover{background:#f0f0f0}
.dl-pg{display:flex;gap:4px;align-items:center;margin-left:auto}
.dl-sort{cursor:pointer;color:#0854a0;text-decoration:underline;font-size:11px}
</style>`
}

function pageHtml (title) {
  return `${styleHtml()}
<div class="dl-root">
  <ui5-bar design="Header" class="dl-bar">
    <span slot="startContent" class="dl-title">${title || '业务单据'}</span>
    <span slot="startContent" class="dl-msg" id="dlMsg"></span>
    <ui5-button slot="endContent" design="Emphasized" id="btnLoad">加载</ui5-button>
    <ui5-button slot="endContent" design="Default" id="btnSave">保存</ui5-button>
  </ui5-bar>
  <div class="dl-grids" id="dlGrids"></div>
</div>`
}

function setMsg (root, text) {
  const el = root.querySelector('#dlMsg')
  if (el) el.textContent = text || ''
}

/* ── 元数据 → 动态建 N 层 grid + schema + 列模型 + 每层筛选/排序/分页 UI ─── */

/** 取元数据（层序/各层列/关系）。 */
async function loadMeta (def) {
  return apiGet(`/api/doc/meta?${qs(def)}`, def.dbId)
}

/** 该层 UI 状态（懒建）。 */
function layerSt (layerId) {
  if (!state.layerState[layerId]) {
    state.layerState[layerId] = { conds: [], sorts: [], limit: state.pageSize, offset: 0, cursor: null, nextCursor: null }
  }
  return state.layerState[layerId]
}

/** 层 id → 该层列元数据（来自 meta.layers）。 */
function layerColumns (layerId) {
  const L = (state.meta.layers || []).find((l) => l.id === layerId)
  return (L && L.columns) || []
}

/**
 * 依据元数据动态渲染 N 个 pane（筛选条 + grid + 分页），建协调器 + 列模型 + 绑定。
 * 全程无任何具体单据假设——层数/列/关系/算子都来自 meta。
 */
function setupGrids (root, meta) {
  const C = cmx()
  if (!C.CmxMasterSlave || !C.buildMasterSlaveSchema) { setMsg(root, 'CMX 数据类/助手未加载'); return null }
  if (state.ms) return state.ms

  const schema = C.buildMasterSlaveSchema(meta)
  const paths = C.layerPaths(meta)
  state.paths = paths
  const ms = new C.CmxMasterSlave({ schema })

  const box = root.querySelector('#dlGrids')
  box.innerHTML = ''
  for (const p of paths) {
    const pane = document.createElement('div')
    pane.className = 'dl-pane'
    pane.innerHTML = paneHtml(p)
    box.appendChild(pane)

    const grid = pane.querySelector('cmx-revo-grid')
    grid.setColumnModel(C.buildColumnModel(C, p.path, p.columns))
    grid.setOptions({ selectionMode: 'single', fillHeight: true, showRowIndex: true, editable: true, stretch: false })
    ms.bindTable(p.path, grid)
    state.grids[p.path] = grid   // 存引用，reload 后强制刷新用

    wirePaneControls(root, pane, p)
  }

  state.ms = ms
  return ms
}

/** 一层 pane 的 HTML（标题 + 筛选条 + grid + 分页）。列/算子下拉由元数据生成。 */
function paneHtml (p) {
  return `
  <div class="dl-pane-head">
    <span class="dl-pane-title">${p.level ? p.level + ' ' : ''}${p.levelName}</span>
    <button class="dl-btn" data-act="add-cond">+ 条件</button>
    <button class="dl-btn" data-act="apply">应用筛选</button>
    <button class="dl-btn" data-act="clear">清空</button>
    <span class="dl-pg">
      <button class="dl-btn" data-act="prev">上一页</button>
      <button class="dl-btn" data-act="next">下一页 ▶</button>
    </span>
  </div>
  <div class="dl-filter" data-role="filter"></div>
  <cmx-revo-grid class="dl-grid" id="grid__${p.path.replace(/\./g, '__')}"></cmx-revo-grid>`
}

/** 一行条件的 HTML（列下拉 + 算子下拉 + 值输入 + 删除）。 */
function condRowHtml (columns, ops) {
  const colOpts = columns.map((c) => `<option value="${c.name}">${c.caption || c.name}</option>`).join('')
  const opOpts = ops.map((o) => `<option value="${o.op}">${o.label}</option>`).join('')
  return `<span class="dl-cond">
    <select data-f="col">${colOpts}</select>
    <select data-f="op">${opOpts}</select>
    <input data-f="value" placeholder="值(多值逗号分隔)" />
    <button class="dl-btn" data-act="del-cond">×</button>
  </span>`
}

/** 给一层 pane 的按钮/表头绑事件（筛选/排序/分页）。 */
function wirePaneControls (root, pane, p) {
  const C = cmx()
  const layerId = p.layerId
  const cols = p.columns
  const filterBox = pane.querySelector('[data-role="filter"]')

  const addCond = () => {
    const span = document.createElement('template')
    // 算子随首列类型；这里给全算子，apply 时按列类型交给后端校验
    span.innerHTML = condRowHtml(cols, C.OPERATORS || [])
    filterBox.appendChild(span.content.firstElementChild)
  }

  pane.querySelector('[data-act="add-cond"]')?.addEventListener('click', addCond)
  pane.querySelector('[data-act="clear"]')?.addEventListener('click', () => {
    filterBox.innerHTML = ''
    layerSt(layerId).conds = []
    layerSt(layerId).offset = 0; layerSt(layerId).cursor = null
    reload(root)
  })
  pane.querySelector('[data-act="apply"]')?.addEventListener('click', () => {
    layerSt(layerId).conds = readConds(filterBox)
    layerSt(layerId).offset = 0; layerSt(layerId).cursor = null
    reload(root)
  })
  pane.querySelector('[data-act="next"]')?.addEventListener('click', () => {
    const st = layerSt(layerId)
    if (st.nextCursor) { st.cursor = st.nextCursor } else { st.offset = (st.offset || 0) + (st.limit || state.pageSize) }
    reload(root)
  })
  pane.querySelector('[data-act="prev"]')?.addEventListener('click', () => {
    const st = layerSt(layerId)
    st.cursor = null
    st.offset = Math.max(0, (st.offset || 0) - (st.limit || state.pageSize))
    reload(root)
  })
  filterBox.addEventListener('click', (ev) => {
    if (ev.target && ev.target.dataset && ev.target.dataset.act === 'del-cond') {
      ev.target.closest('.dl-cond')?.remove()
    }
  })

  // 表头点击排序：grid 抛的选中列事件不统一，这里提供一个简单"排序"入口——
  // 用 grid 的 header 事件若无，则在 pane 头加一个"排序:列"快捷（保持通用，不依赖具体列）。
  const grid = pane.querySelector('cmx-revo-grid')
  grid?.addEventListener?.('header-click', (ev) => {
    const col = ev.detail && ev.detail.prop
    if (!col) return
    toggleSort(layerId, col)
    reload(root)
  })
  // 展开某父行 → 懒下钻该行子树（若 grid 支持展开事件）
  grid?.addEventListener?.('row-expand', (ev) => {
    const rowId = ev.detail && ev.detail.id
    if (rowId != null) lazyExpand(root, p, rowId)
  })
}

/** 读筛选条里的条件行。 */
function readConds (filterBox) {
  const out = []
  filterBox.querySelectorAll('.dl-cond').forEach((el) => {
    const col = el.querySelector('[data-f="col"]').value
    const op = el.querySelector('[data-f="op"]').value
    const value = el.querySelector('[data-f="value"]').value
    if (col && op) out.push({ col, op, value })
  })
  return out
}

/** 切换某层某列排序：无→asc→desc→无。 */
function toggleSort (layerId, col) {
  const st = layerSt(layerId)
  const i = st.sorts.findIndex((s) => s.col === col)
  if (i < 0) st.sorts = [{ col, desc: false }]
  else if (!st.sorts[i].desc) st.sorts[i].desc = true
  else st.sorts = st.sorts.filter((s) => s.col !== col)
  st.offset = 0; st.cursor = null
}

/* ── 数据装载（用 buildDocQuery 组每层查询，POST 富查询） ─────────────────── */

/** 用各层 UI 状态组 DocQuery（元数据驱动）。 */
function buildQuery () {
  const C = cmx()
  const perLayer = {}
  for (const p of state.paths) {
    const st = layerSt(p.layerId)
    perLayer[p.layerId] = {
      conds: st.conds, columns: p.columns, sorts: st.sorts,
      limit: st.limit, offset: st.cursor ? undefined : st.offset, cursor: st.cursor || undefined,
    }
  }
  return C.buildDocQuery ? C.buildDocQuery(perLayer, {}) : {}
}

async function loadData (def) {
  const C = cmx()
  const query = buildQuery()
  const r = await C.loadDocData(null, {
    domain: def.domain, application: def.application, module: def.module, file: def.file,
    dbId: def.dbId, apiPath: def.apiPath, binary: def.binary === true,
    query: (query && query.layers) ? query : undefined,
  })
  return r
}

/** 懒下钻：展开父行时只拉该层在该父下的子树，回填协调器。 */
async function lazyExpand (root, p, parentId) {
  const C = cmx()
  if (typeof C.loadChildren !== 'function') return
  // 找 p 的下一层（子层）——用 paths 里紧跟 p 且 path 以 p.path 为前缀的层
  const child = state.paths.find((x) => x.path.startsWith(p.path + '.') && x.path.split('.').length === p.path.split('.').length + 1)
  if (!child) return
  try {
    const st = layerSt(child.layerId)
    const filter = C.buildLayerFilter ? C.buildLayerFilter(st.conds, child.columns) : undefined
    const q = {}
    if (filter) q.filter = filter
    if (st.sorts && st.sorts.length) q.orderBy = C.buildOrderBy(st.sorts)
    const { pkg } = await C.loadChildren(null, state.def, {
      layer: child.layerId, parentIds: [parentId], query: Object.keys(q).length ? q : undefined,
      exit: (state.def.apiPath || '').includes('sqlx') ? 'sqlx-zmc-json' : undefined,
    })
    // 回填：把子树挂到协调器对应父行（若 ms 支持 setChildDataSet；否则整体 reload）
    if (state.ms && typeof state.ms.setChildData === 'function') {
      state.ms.setChildData(child.path, parentId, C.CmxDataSet.fromJSON(pkg))
    }
    setMsg(root, `已按需装载 ${child.levelName} 子树（父 ${parentId}）：${(pkg.rows || []).length} 行`)
  } catch (e) { setMsg(root, `懒下钻失败：${e.message}`) }
}

/** 重新装载（筛选/排序/分页变化后调用）。 */
async function reload (root) {
  const def = state.def
  setMsg(root, '装载中…')
  try {
    const r = await loadData(def)
    if (!r.dsMap || !Object.keys(r.dsMap).length) throw new Error('返回数据为空')
    const C = cmx()
    state.ms.setDataSet(r.dsMap)
    // 强制各 grid 从当前 source 重渲染（防某些时序下 revo 未刷新）。协调器已把新 ds
    // 推给各 grid（_ds/_rows 已更新），refreshLayout→revo.refresh('all') 保证界面重画。
    for (const g of Object.values(state.grids)) {
      try { g.refreshLayout && g.refreshLayout() } catch (_) {}
    }
    state.collector = C.ChangeSetCollector ? new C.ChangeSetCollector(state.ms).attach() : null
    const rootPath = state.paths[0] && state.paths[0].path
    const rootDs = rootPath ? state.ms.getRootDataSet(rootPath) : null
    setMsg(root, `已装载 · 根层 ${rootDs ? rootDs.length : 0} 行`)
  } catch (e) { setMsg(root, `装载失败：${e.message}`) }
}

async function loadVoucher (root) {
  const def = state.def
  setMsg(root, '装载元数据…')
  let meta = state.meta
  try {
    if (!meta) { meta = await loadMeta(def); state.meta = meta }
  } catch (e) { setMsg(root, `元数据装载失败：${e.message}`); return }
  if (!meta || !Array.isArray(meta.layers) || !meta.layers.length) { setMsg(root, '元数据无层定义'); return }

  const ms = setupGrids(root, meta)   // 动态建 N grid + 筛选/分页 UI（幂等）
  if (!ms) return
  // 等一帧让动态建的 cmx-revo-grid 完成挂载（connectedCallback + 内部 revo 实例就绪），
  // 再装数据，确保首次 setDataSet 能真正渲染（否则 _revo 未就绪、后续刷新时序易错乱）。
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
  await reload(root)
}

async function saveVoucher (root) {
  const C = cmx()
  const def = state.def
  if (!state.ms) { setMsg(root, '请先加载'); return }
  if (!state.collector) { setMsg(root, '变更收集器未就绪'); return }
  const changes = state.collector.export()
  if (!Object.keys(changes).length) { setMsg(root, '无变更可保存'); return }
  setMsg(root, '保存中…')
  try {
    let result
    if (typeof C.saveDocData === 'function') {
      result = await C.saveDocData(null, { ...def, dbId: def.dbId }, { saveMode: 'merge', changes })
    } else {
      result = await apiPost(`/api/doc/save?${qs(def)}`, { saveMode: 'merge', changes }, def.dbId)
    }
    state.collector.reset()
    setMsg(root, `保存成功：影响 ${result.affected} 行`)
  } catch (e) {
    // 保存失败 → 专业信息对话框（冲突/列校验/普通失败分级，violations 逐行，可跳帮助中心）。
    // presentDocError 未加载（老 bundle）时回退到内联消息，保证不崩。
    setMsg(root, `保存失败：${e.message}`)
    if (typeof C.presentDocError === 'function') {
      const r = await C.presentDocError(e, { action: 'save', tableNames: layerTableNames() })
      // 乐观锁冲突：用户确认后重新装载到最新版，避免覆盖他人改动。
      if (r && r.kind === 'conflict') { state.meta = null; await loadVoucher(root) }
    }
  }
}

/** 物理表名(层 id) → 中文层名 映射，供列校验 violations 前缀「中文名(表名)」显示。 */
function layerTableNames () {
  const layers = (state.meta && state.meta.layers) || []
  const map = {}
  for (const l of layers) { if (l && l.id) map[l.id] = l.levelName || l.level || l.id }
  return map
}

function bindPage (root) {
  root.querySelector('#btnLoad')?.addEventListener('click', () => loadVoucher(root))
  root.querySelector('#btnSave')?.addEventListener('click', () => saveVoucher(root))
  Promise.resolve().then(() => { setMsg(root, '就绪，正在装载…'); return loadVoucher(root) })
}

/* ── mount + export ──────────────────────────────────────────────────── */
function whenRendered (host, selector, cb, tries) {
  const t = tries == null ? 60 : tries
  const root = host && host.renderRoot
  if (root && root.querySelector(selector)) { cb(root); return }
  if (t <= 0) return
  requestAnimationFrame(() => whenRendered(host, selector, cb, t - 1))
}

/** 校验并归一 props → def；缺关键坐标则报错（页面通用，坐标必须由菜单给）。
 *  DAM（domain/application/module）优先从 workspace.context 读取（框架 openNode 时注入），
 *  fallback 到 view props（向后兼容）。file/dbId/apiPath 等仍在 props。 */
function readDef (ctx) {
  const p = (ctx && ctx.props) || {}
  // ctx.host.workspace.context：框架在 openNode 时注入 DAM（短名 domain/application/module）
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  const def = {
    domain: get('domain') || p.domain,
    application: get('application') || p.application,
    module: get('module') || p.module,
    file: p.file,
    dbId: p.dbId || p.db_id || '', apiPath: p.apiPath || '', binary: p.binary === true,
    limit: p.limit,
  }
  const ok = def.domain && def.application && def.module && def.file
  return ok ? def : null
}

export default {
  defaultView: 'content',
  views: {
    async content (ctx) {
      const host = ctx && ctx.host
      // 每次进入重置实例状态
      state.def = null; state.meta = null; state.ms = null; state.collector = null; state.paths = []
      state.layerState = {}
      state.grids = {}

      const def = readDef(ctx)
      if (!def) {
        return `<div style="padding:12px;color:#b00;font-size:13px">通用单据加载页缺少必要 props：需 { domain, application, module, file }（可选 dbId/apiPath/binary）。</div>`
      }
      state.def = def
      const title = `业务单据 · ${def.module}/${def.file}${def.binary ? ' · 二进制' : (def.apiPath ? ' · ' + def.apiPath.split('/').pop() : '')}`

      if (host) whenRendered(host, '.dl-root', (root) => bindPage(root))
      return pageHtml(title)
    },
  },
}
