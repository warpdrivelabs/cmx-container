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
const state = { def: null, meta: null, ms: null, collector: null, paths: [], loading: false }

/* ── 响应归一（兼容门户已拆信封 / 原始信封两种形态） ─────────────────────── */
function unwrap (res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) throw new Error(body.msg || `业务错误 code ${body.code}`)
    return body.data
  }
  if (!res.ok) throw new Error((body && body.error) || `HTTP ${res.status}`)
  return body
}

/** GET JSON（带 db_id 头），归一取业务数据。 */
async function apiGet (url, dbId) {
  const headers = { Accept: 'application/json' }
  if (dbId) headers.db_id = dbId
  const res = await fetch(url, { headers, credentials: 'same-origin' })
  const body = await res.json().catch(() => null)
  return unwrap(res, body)
}

/** POST JSON。 */
async function apiPost (url, payload, dbId) {
  const headers = { 'Content-Type': 'application/json', Accept: 'application/json' }
  if (dbId) headers.db_id = dbId
  const res = await fetch(url, { method: 'POST', headers, credentials: 'same-origin', body: JSON.stringify(payload) })
  const body = await res.json().catch(() => null)
  return unwrap(res, body)
}

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
.dl-grids{flex:1;display:flex;flex-direction:column;gap:6px;min-height:0;min-width:0;overflow:hidden}
.dl-pane{flex:1 1 0;display:flex;flex-direction:column;min-height:0;min-width:0;overflow:hidden}
.dl-grid{display:block;width:100%;flex:1;min-height:0;min-width:0}
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

/* ── 元数据 → 动态建 N 层 grid + schema + 列模型 ───────────────────────── */

/** 取元数据（层序/各层列/关系）。 */
async function loadMeta (def) {
  return apiGet(`/api/doc/meta?${qs(def)}`, def.dbId)
}

/**
 * 依据元数据动态渲染 N 个 pane+grid，建协调器 + 列模型 + 绑定。幂等（已建则复用）。
 * 全程无任何具体单据假设——层数/列/关系都来自 meta。
 */
function setupGrids (root, meta) {
  const C = cmx()
  if (!C.CmxMasterSlave || !C.buildMasterSlaveSchema) { setMsg(root, 'CMX 数据类/助手未加载'); return null }
  if (state.ms) return state.ms

  const schema = C.buildMasterSlaveSchema(meta)
  const paths = C.layerPaths(meta)
  state.paths = paths
  const ms = new C.CmxMasterSlave({ schema })

  // 动态生成 N 个 pane+grid（标题=levelName）
  const box = root.querySelector('#dlGrids')
  box.innerHTML = ''
  for (const p of paths) {
    const pane = document.createElement('div')
    pane.className = 'dl-pane'
    const title = document.createElement('ui5-title')
    title.setAttribute('level', 'H5'); title.setAttribute('size', 'H5')
    title.textContent = `${p.level ? p.level + ' ' : ''}${p.levelName}`
    const grid = document.createElement('cmx-revo-grid')
    grid.className = 'dl-grid'
    grid.id = `grid__${p.path.replace(/\./g, '__')}`
    pane.appendChild(title); pane.appendChild(grid)
    box.appendChild(pane)

    // 列模型（列头 caption/类型/宽度来自元数据）+ grid 选项
    grid.setColumnModel(C.buildColumnModel(C, p.path, p.columns))
    grid.setOptions({ selectionMode: 'single', fillHeight: true, showRowIndex: true, editable: true, stretch: false })
    ms.bindTable(p.path, grid)
  }

  state.ms = ms
  return ms
}

/* ── 数据装载（props 决定通道：JSON / msgpack 二进制） ──────────────────── */

async function loadData (def) {
  const C = cmx()
  // 统一用共享 loadDocData：apiPath/binary 由 props 决定，内部信封容错 + msgpack 解码
  const r = await C.loadDocData(null, {
    domain: def.domain, application: def.application, module: def.module, file: def.file,
    dbId: def.dbId, apiPath: def.apiPath, binary: def.binary === true, limit: def.limit || 50,
  })
  return r.dsMap
}

async function loadVoucher (root) {
  const C = cmx()
  const def = state.def
  setMsg(root, '装载元数据…')
  let meta = state.meta
  try {
    if (!meta) { meta = await loadMeta(def); state.meta = meta }
  } catch (e) { setMsg(root, `元数据装载失败：${e.message}`); return }
  if (!meta || !Array.isArray(meta.layers) || !meta.layers.length) { setMsg(root, '元数据无层定义'); return }

  const ms = setupGrids(root, meta)   // 动态建 N grid + schema（幂等）
  if (!ms) return

  setMsg(root, `装载数据…（${def.apiPath || '/api/doc/data/sqlx-dataset-json'}${def.binary ? ' · 二进制' : ''}）`)
  let dsMap
  try {
    dsMap = await loadData(def)
    if (!dsMap || !Object.keys(dsMap).length) throw new Error('返回数据为空')
  } catch (e) { setMsg(root, `数据装载失败：${e.message}`); return }

  ms.setDataSet(dsMap)
  state.collector = C.ChangeSetCollector ? new C.ChangeSetCollector(ms).attach() : null

  const rootPath = state.paths[0] && state.paths[0].path
  const rootDs = rootPath ? ms.getRootDataSet(rootPath) : null
  setMsg(root, `已装载 ${meta.layers.length} 层 · 根层 ${rootDs ? rootDs.length : 0} 行`)
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
  } catch (e) { setMsg(root, `保存失败：${e.message}`) }
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

/** 校验并归一 props → def；缺关键坐标则报错（页面通用，坐标必须由菜单给）。 */
function readDef (ctx) {
  const p = (ctx && ctx.props) || {}
  const def = {
    domain: p.domain, application: p.application, module: p.module, file: p.file,
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
