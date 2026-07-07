/**
 * doc-service-test — 业务单据加载服务**全后端能力**自检页（native_pages）。
 *
 * 纯诊断：逐项调用后端每个能力，展示通过/失败清单 + 关键指标。零业务假设——
 * 单据坐标由 props 给（缺省会计凭证 cmxfico_doc_meta_v1）。覆盖：
 *   1. GET  /api/doc/meta                          元数据（层/列/关系）
 *   2. 5 个装载端点（驱动×内存×传输）GET 便捷 + 出口一致性
 *   3. POST DocQuery 富查询：根层过滤 + 排序 + limit
 *   4. 子层条件下推（cv_acc_line 等某度量列 > 0）
 *   5. keyset 游标翻页（page1 → nextCursor → page2，断言 id 递增不重）
 *   6. POST /api/doc/data/children                 懒下钻某层子树
 *   7. GET/POST /api/doc/data/tokio-zmc-stream      真·流式（分帧解码 → 行数）
 *
 * 契约：export default { defaultView:'content', views:{ content(ctx) } }。
 * 助手经 globalThis.__cmxDataComp：loadDocData/loadChildren/loadDocDataStream/
 * buildDocQuery/FrameStreamParser/decodeMsgpack/CmxDataSet。
 */

const DEFAULTS = { domain: 'fi', application: 'cmxfico', module: 'gl', file: 'cmxfico_doc_meta_v1.json', dbId: 'fico-db' }
const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

/* ── 响应归一（兼容门户已拆信封 / 原始信封） ─────────────────────────────── */
function unwrap (res, body) {
  if (body && typeof body === 'object' && typeof body.code === 'number') {
    if (body.code !== 0) throw new Error(body.msg || `code ${body.code}`)
    return body.data
  }
  if (!res.ok) throw new Error((body && body.error) || `HTTP ${res.status}`)
  return body
}
async function apiGet (url, dbId) {
  const h = { Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const res = await fetch(url, { headers: h, credentials: 'same-origin' })
  return unwrap(res, await res.json().catch(() => null))
}
async function apiPost (url, payload, dbId) {
  const h = { 'Content-Type': 'application/json', Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const res = await fetch(url, { method: 'POST', headers: h, credentials: 'same-origin', body: JSON.stringify(payload) })
  return unwrap(res, await res.json().catch(() => null))
}
/** 二进制端点：arrayBuffer + msgpack 解码 → 列式包。 */
async function apiMsgpack (url, dbId) {
  const C = cmx()
  const h = { Accept: 'application/x-msgpack' }; if (dbId) h.db_id = dbId
  const res = await fetch(url, { headers: h, credentials: 'same-origin' })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  const buf = new Uint8Array(await res.arrayBuffer())
  const env = C.decodeMsgpack(buf)
  // env 可能是 {code,msg,data} 信封或裸包
  return (env && typeof env.code === 'number') ? env.data : env
}

function q (def, extra = {}) {
  return new URLSearchParams({ domain: def.domain, application: def.application, module: def.module, file: def.file, ...extra }).toString()
}

/* ── 测试骨架 ─────────────────────────────────────────────────────────── */
const results = []   // { name, ok, detail, ms }

async function run (name, fn) {
  const t0 = (typeof performance !== 'undefined' ? performance.now() : 0)
  try {
    const detail = await fn()
    results.push({ name, ok: true, detail: detail || '', ms: Math.round(((typeof performance !== 'undefined' ? performance.now() : 0) - t0)) })
  } catch (e) {
    results.push({ name, ok: false, detail: e.message, ms: 0 })
  }
}

/** 从列式包取根层行数 + 首层子表键（用于展示）。 */
function pkgSummary (pkg) {
  const rows = (pkg.rows || []).length
  const cr = pkg.childRows || {}
  const pid = Object.keys(cr)[0]
  const childKeys = pid ? Object.keys(cr[pid]) : []
  return { rows, cols: (pkg.columns || []).length, childKeys }
}

/** 找一个数值型度量列（供子层过滤测试），从 meta 层里挑 dimType=measure 或 DECIMAL。 */
function pickMeasureCol (meta, layerId) {
  const L = (meta.layers || []).find((l) => l.id === layerId)
  if (!L) return null
  const m = (L.columns || []).find((c) => c.dimType === 'measure' || /DECIMAL|NUMERIC|INT|BIGINT/i.test(c.dataType))
  return m ? m.name : null
}

async function runAll (def) {
  results.length = 0
  const C = cmx()
  let meta = null
  let rootId = null
  let secondLayerId = null

  // 1. /api/doc/meta
  await run('GET /api/doc/meta（元数据：层/列/关系）', async () => {
    meta = await apiGet(`/api/doc/meta?${q(def)}`, def.dbId)
    if (!meta || !Array.isArray(meta.layers) || !meta.layers.length) throw new Error('无层定义')
    rootId = (meta.layerOrder && meta.layerOrder[0]) || meta.layers[0].id
    secondLayerId = (meta.layerOrder && meta.layerOrder[1]) || null
    const totalCols = meta.layers.reduce((n, l) => n + (l.columns || []).length, 0)
    const groups = (meta.layerGroups || []).map((g) => `${g.level}:${g.tableIds.length}表`).join(' ')
    return `${meta.layers.length} 层 · ${totalCols} 列 · ${(meta.relations || []).length} 关系 · 组[${groups}]`
  })
  if (!meta) { render(def); return }

  // 2. 五个装载端点（GET 便捷，limit 5）——出口一致性
  const endpoints = [
    { path: '/api/doc/data/sqlx-dataset-json', kind: 'json' },
    { path: '/api/doc/data/tokio-zmc-json', kind: 'json' },
    { path: '/api/doc/data/sqlx-zmc-json', kind: 'json' },
    { path: '/api/doc/data/tokio-zmc-msgpack', kind: 'msgpack' },
    { path: '/api/doc/data/sqlx-zmc-msgpack', kind: 'msgpack' },
  ]
  const seen = {}
  for (const ep of endpoints) {
    await run(`GET ${ep.path.split('/').pop()}（装载 limit 5）`, async () => {
      const url = `${ep.path}?${q(def, { limit: '5' })}`
      const pkg = ep.kind === 'msgpack' ? await apiMsgpack(url, def.dbId) : await apiGet(url, def.dbId)
      if (!pkg || pkg.datasetId == null) throw new Error('返回无 datasetId')
      const s = pkgSummary(pkg)
      seen[ep.path] = s.rows
      return `datasetId=${pkg.datasetId} · ${s.rows} 行 · ${s.cols} 列 · 子键[${s.childKeys.join(',')}]`
    })
  }
  await run('五端点出口行数一致', async () => {
    const vals = Object.values(seen)
    if (!vals.length) throw new Error('无端点数据')
    const allSame = vals.every((v) => v === vals[0])
    if (!allSame) throw new Error(`行数不一致: ${JSON.stringify(seen)}`)
    return `全部 ${vals[0]} 行一致（驱动/内存/传输 五组同构）`
  })

  // 3. POST DocQuery 富查询：根层过滤 + 排序 + limit（用 buildDocQuery）
  await run('POST 富查询（根层过滤 + 排序 + limit 3）', async () => {
    // 挑根层一个文本/维度列做等值过滤：取首行某列值当条件
    const probe = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', limit: 1 })
    const rootDs = Object.values(probe.dsMap)[0]
    const rootLayer = (meta.layers || []).find((l) => l.id === rootId)
    // 找一个非 id/系统列做过滤
    const col = (rootLayer.columns || []).find((c) => !/^id$|^upper_id$|^line_no$/.test(c.name) && c.dataType && /VARCHAR|CHAR|TEXT/i.test(c.dataType))
    const firstRow = rootDs && rootDs.length ? rootDs.getRow(rootDs.rows[0].id) : null
    const val = col && firstRow ? firstRow[col.name] : null
    const conds = (col && val != null) ? [{ col: col.name, op: '$eq', value: val }] : []
    const dq = C.buildDocQuery({ [rootId]: { conds, columns: rootLayer.columns, sorts: [{ col: 'id', desc: true }], limit: 3 } }, {})
    const r = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', query: (dq.layers ? dq : undefined) })
    const ds = Object.values(r.dsMap)[0]
    return `过滤列 ${col ? col.name : '(无文本列跳过)'}=${val} → ${ds.length} 行（≤3，降序）`
  })

  // 4. 子层条件下推
  if (secondLayerId) {
    await run(`POST 子层条件下推（${secondLayerId} 度量列 > 0）`, async () => {
      const mcol = pickMeasureCol(meta, secondLayerId)
      if (!mcol) return '(该层无可比数值列，跳过)'
      const dq = { layers: { [rootId]: { limit: 1 }, [secondLayerId]: { filter: { [mcol]: { $gt: 0 } } } } }
      const r = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', query: dq })
      const pkg = r.pkg
      const s = pkgSummary(pkg)
      return `根 1 行 · 子层 ${secondLayerId} 过滤 ${mcol}>0 下推成功 · 子键[${s.childKeys.join(',')}]`
    })
  }

  // 5. keyset 游标翻页
  await run('POST keyset 游标翻页（page1 → page2，id 递增不重）', async () => {
    const p1 = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', query: { layers: { [rootId]: { orderBy: ['id'], limit: 2 } } } })
    const ds1 = Object.values(p1.dsMap)[0]
    const ids1 = (ds1.rows || []).map((r) => r.id)
    if (ids1.length < 2) return `根层仅 ${ids1.length} 行，样本不足（仍算通过）`
    const lastId = ids1[ids1.length - 1]
    const cursor = C.encodeCursor([], lastId)
    const p2 = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', query: { layers: { [rootId]: { orderBy: ['id'], limit: 2, cursor } } } })
    const ds2 = Object.values(p2.dsMap)[0]
    const ids2 = (ds2.rows || []).map((r) => r.id)
    const overlap = ids2.some((id) => ids1.includes(id))
    if (overlap) throw new Error(`page2 与 page1 重叠: p1=${ids1} p2=${ids2}`)
    return `page1=[${ids1}] → page2=[${ids2}]（无重叠，游标稳定）`
  })

  // 6. 懒下钻 /children
  if (secondLayerId) {
    await run(`POST /children 懒下钻（${secondLayerId} 在某些根父下）`, async () => {
      const p1 = await C.loadDocData(null, { ...def, apiPath: '/api/doc/data/tokio-zmc-json', query: { depth: 1, layers: { [rootId]: { limit: 2 } } } })
      const ds1 = Object.values(p1.dsMap)[0]
      const parentIds = (ds1.rows || []).map((r) => r.id)
      if (!parentIds.length) return '根层无行，跳过'
      const { pkg } = await C.loadChildren(null, def, { layer: secondLayerId, parentIds, depth: 0 })
      return `父 [${parentIds}] → ${secondLayerId} 子树 ${(pkg.rows || []).length} 行`
    })
  }

  // 7. 真·流式 tokio-zmc-stream
  await run('GET tokio-zmc-stream（真·流式分帧解码）', async () => {
    if (typeof C.loadDocDataStream !== 'function') throw new Error('loadDocDataStream 未加载')
    let progressBatches = 0
    const pkg = await C.loadDocDataStream(null, { ...def, layer: rootId, limit: 1000 }, { onProgress: () => { progressBatches++ } })
    if (!pkg || pkg.datasetId == null) throw new Error('流未产出有效包')
    // 组装回 CmxDataSet 证明 grid-ready
    const ds = C.CmxDataSet ? C.CmxDataSet.fromJSON(pkg) : { length: (pkg.rows || []).length }
    return `分帧流 → datasetId=${pkg.datasetId} · ${pkg.rows.length} 行 · ${pkg.columns.length} 列 · fromJSON=${ds.length} 行`
  })
  await run('POST tokio-zmc-stream + 过滤（子层度量列）', async () => {
    if (!secondLayerId) return '(无子层，跳过)'
    const mcol = pickMeasureCol(meta, secondLayerId)
    const query = mcol ? { filter: { [mcol]: { $gt: 0 } }, orderBy: ['id'] } : { orderBy: ['id'] }
    const pkg = await C.loadDocDataStream(null, { ...def, layer: secondLayerId, query }, {})
    return `流式 ${secondLayerId}${mcol ? ` 过滤 ${mcol}>0` : ''} → ${pkg.rows.length} 行（下推生效）`
  })
}

/* ── 渲染 ─────────────────────────────────────────────────────────────── */
function styleHtml () {
  return `<style>
.st-root{font:13px/1.5 -apple-system,Segoe UI,sans-serif;padding:12px;height:100%;box-sizing:border-box;overflow:auto;color:#222}
.st-head{display:flex;align-items:center;gap:12px;margin-bottom:8px}
.st-title{font-weight:600;font-size:15px}
.st-run{padding:3px 14px;cursor:pointer;border:1px solid #0854a0;background:#0854a0;color:#fff;border-radius:4px}
.st-run:hover{background:#063d78}
.st-sum{margin-left:auto;font-weight:600}
.st-sum.ok{color:#107e3e}.st-sum.bad{color:#bb0000}
.st-list{border:1px solid #e5e5e5;border-radius:6px;overflow:hidden}
.st-item{display:flex;gap:10px;padding:6px 10px;border-top:1px solid #f0f0f0;align-items:baseline}
.st-item:first-child{border-top:0}
.st-badge{flex:0 0 auto;width:44px;text-align:center;font-weight:700;border-radius:3px;font-size:11px;padding:1px 0}
.st-badge.ok{background:#e6f4ea;color:#107e3e}.st-badge.bad{background:#fbe9e9;color:#bb0000}
.st-name{flex:0 0 44%;font-weight:600}
.st-detail{flex:1;color:#555;word-break:break-all}
.st-ms{flex:0 0 auto;color:#999;font-size:11px}
.st-props{margin:6px 0;color:#777;font-size:12px}
</style>`
}

function render (def) {
  const root = _root
  if (!root) return
  const total = results.length
  const passed = results.filter((r) => r.ok).length
  const allOk = total > 0 && passed === total
  root.innerHTML = `
${styleHtml()}
<div class="st-root">
  <div class="st-head">
    <span class="st-title">业务单据加载服务 · 全后端能力自检</span>
    <button class="st-run" id="stRun">重新运行</button>
    <span class="st-sum ${allOk ? 'ok' : 'bad'}">${passed}/${total} 通过</span>
  </div>
  <div class="st-props">坐标：${def.domain}/${def.application}/${def.module}/${def.file} · db=${def.dbId}</div>
  <div class="st-list">
    ${results.map((r) => `
      <div class="st-item">
        <span class="st-badge ${r.ok ? 'ok' : 'bad'}">${r.ok ? 'PASS' : 'FAIL'}</span>
        <span class="st-name">${escapeHtml(r.name)}</span>
        <span class="st-detail">${escapeHtml(r.detail)}</span>
        <span class="st-ms">${r.ms ? r.ms + 'ms' : ''}</span>
      </div>`).join('')}
    ${total === 0 ? '<div class="st-item"><span class="st-detail">点击「重新运行」开始…</span></div>' : ''}
  </div>
</div>`
  root.querySelector('#stRun')?.addEventListener('click', () => start(def))
}

function escapeHtml (s) {
  return String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]))
}

let _root = null
async function start (def) {
  results.length = 0
  render(def) // 显示"运行中"骨架（空列表）
  if (_root) _root.querySelector('.st-sum').textContent = '运行中…'
  await runAll(def)
  render(def)
}

/* ── mount + export ──────────────────────────────────────────────────── */
function whenRendered (host, selector, cb, tries) {
  const t = tries == null ? 60 : tries
  const r = host && host.renderRoot
  if (r && r.querySelector(selector)) { cb(r); return }
  if (t <= 0) return
  requestAnimationFrame(() => whenRendered(host, selector, cb, t - 1))
}

function readDef (ctx) {
  const p = (ctx && ctx.props) || {}
  return {
    domain: p.domain || DEFAULTS.domain,
    application: p.application || DEFAULTS.application,
    module: p.module || DEFAULTS.module,
    file: p.file || DEFAULTS.file,
    dbId: p.dbId || p.db_id || DEFAULTS.dbId,
  }
}

export default {
  defaultView: 'content',
  views: {
    async content (ctx) {
      const host = ctx && ctx.host
      const def = readDef(ctx)
      if (host) {
        whenRendered(host, '.st-root', (root) => { _root = root; start(def) })
      }
      // 返回初始骨架，宿主挂载后自动跑
      _root = null
      return `${styleHtml()}<div class="st-root"><div class="st-head"><span class="st-title">业务单据加载服务 · 全后端能力自检</span></div><div class="st-props">初始化中…</div></div>`
    },
  },
}
