// M6 门户级 E2E（消费方零改验证）：用真实 designer.js + 真实后端版式 API + 自研 mega 内核跑通打开报表。
//
// 门户 UI 需登录才populate __cmxDataComp，故不走登录 UI；改为「等价装配」：
//   - 起本地代理服务器：本地静态服务 harness.html + 自研 mega ESM；/api/* 与 /portal/native-pages/* 反代到 :8080（真后端）。
//   - harness 自己装 globalThis.__cmxDataComp = { CmxSpreadjsSheet: mega类 }（designer.js 只需这一项；presentDocError/Spread 均 guarded 可选）。
//   - import 真实 designer.js（从 :8080 原样取，字节未改），挂 content 视图打开 STAT_01_D@V2。
//   - 该报表 BLOB 是存量 SpreadJS SSJSON → loadLayout→setWorkbookJson→importSSJSON 双读硬门。
// designer.js / report-applier.js **一字未改**（消费方零改）；只是内核换成 <cmx-megasheet>。
import { chromium } from 'playwright'
import { fileURLToPath } from 'node:url'
import { dirname, join, normalize } from 'node:path'
import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { request as httpRequest } from 'node:http'

const here = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(here, '..')
const UPSTREAM = 'http://127.0.0.1:8080'
const REPORT = 'STAT_01_D'
const VERSION = 'V2'

// 反代到真后端（全方法：GET/POST/… + 请求体透传）
function proxy (req, res) {
  const u = new URL(req.url, UPSTREAM)
  const preq = httpRequest({ hostname: '127.0.0.1', port: 8080, path: u.pathname + u.search, method: req.method, headers: { ...req.headers, host: '127.0.0.1:8080' } }, (pres) => {
    res.writeHead(pres.statusCode || 502, pres.headers)
    pres.pipe(res)
  })
  preq.on('error', () => { res.writeHead(502); res.end('proxy error') })
  req.pipe(preq)
}

const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.css': 'text/css', '.png': 'image/png' }
const HARNESS = `<!DOCTYPE html><html><head><meta charset="utf-8"><title>M6 portal E2E</title>
<style>html,body{height:100%;margin:0;background:#0f1117}</style></head><body>
<script type="module">
  window.__CMX_SHEET_KERNEL__ = 'mega'
  import('/vendor/cmx-spreadjs-sheet-mega.js').then((mod) => {
    // designer.js 只需 __cmxDataComp.CmxSpreadjsSheet；Spread/presentDocError 皆 guarded 可选。
    globalThis.__cmxDataComp = { CmxSpreadjsSheet: mod.CmxSpreadjsSheetMega, resolveSheetKernel: () => 'mega' }
    if (!customElements.get('cmx-spreadjs-sheet')) customElements.define('cmx-spreadjs-sheet', mod.CmxSpreadjsSheetMega)
    window.__ready = true
  }).catch((e) => { window.__err = String(e) })
</script></body></html>`

// 自研 mega 源（wrapper + adapter + vendored ESM）在 packages/cmx-data-comp 下，映射到 /vendor/*
const MEGA_DIR = join(repoRoot, 'packages/cmx-data-comp/src/components/spreadjs')

const server = createServer(async (req, res) => {
  const path = decodeURIComponent((req.url || '/').split('?')[0])
  if (path === '/favicon.ico') { res.writeHead(204); res.end(); return }
  if (path.startsWith('/api/') || path.startsWith('/portal/native-pages/')) return proxy(req, res)
  if (path === '/' || path === '/harness.html') { res.writeHead(200, { 'Content-Type': 'text/html' }); res.end(HARNESS); return }
  if (path.startsWith('/vendor/')) {
    try {
      const fp = normalize(join(MEGA_DIR, path.slice('/vendor/'.length)))
      if (!fp.startsWith(MEGA_DIR)) { res.writeHead(403); res.end(); return }
      const body = await readFile(fp)
      res.writeHead(200, { 'Content-Type': MIME[fp.slice(fp.lastIndexOf('.'))] || 'application/octet-stream' })
      res.end(body); return
    } catch { res.writeHead(404); res.end('nf'); return }
  }
  res.writeHead(404); res.end('nf')
})
await new Promise((r) => server.listen(0, '127.0.0.1', r))
const port = server.address().port
const ORIGIN = `http://127.0.0.1:${port}`

const browser = await chromium.launch({ channel: 'chrome' })
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } })
const errors = []
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })
page.on('pageerror', (e) => errors.push(String(e)))

await page.goto(`${ORIGIN}/harness.html`, { waitUntil: 'domcontentloaded' })
await page.waitForFunction(() => window.__ready || window.__err, { timeout: 20000 })

// 两个真实消费方都跑：designer.js（设计态）+ report-applier.js（应用态/取数）。
async function driveConsumer (pageId, props) {
  return page.evaluate(async ({ pageId, props }) => {
    const r = { pageId, steps: [] }
    if (window.__err) { r.error = 'harness import failed: ' + window.__err; return r }

    const host = document.createElement('div')
    host.style.cssText = 'position:fixed;inset:0;width:1400px;height:900px;background:#0f1117'
    const shadow = host.attachShadow({ mode: 'open' })
    const rootEl = document.createElement('div')
    rootEl.className = 'native-page-root'
    rootEl.style.cssText = 'width:100%;height:100%'
    shadow.appendChild(rootEl)
    Object.defineProperty(host, 'renderRoot', { get () { return rootEl } })
    host.__props = props
    document.body.appendChild(host)

    let mod
    try {
      const resp = await fetch('/api/native-pages/' + pageId)
      const j = await resp.json()
      const src = (j.data && j.data.source) || j.source
      if (!src) { r.error = 'native-pages 未返回源'; return r }
      const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }))
      mod = (await import(url)).default
      r.steps.push('module imported (blob)')
    } catch (e) { r.error = 'import failed: ' + String(e); return r }

    try { await mod.views.content({ props, host }); r.steps.push('content mounted') }
    catch (e) { r.error = 'mount failed: ' + String(e); return r }

    const sheetEl = await new Promise((resolve) => {
      const t0 = Date.now()
      const tick = () => {
        const el = rootEl.querySelector('cmx-spreadjs-sheet')
        if (el && el.getWorkbook && el.getWorkbook()) return resolve(el)
        if (Date.now() - t0 > 20000) return resolve(el || null)
        setTimeout(tick, 100)
      }
      tick()
    })
    if (!sheetEl) { r.error = '<cmx-spreadjs-sheet> 未就绪'; return r }
    r.steps.push('sheet ready')
    r.hasInnerMegasheet = !!sheetEl.shadowRoot?.querySelector('cmx-megasheet')

    await new Promise((res) => setTimeout(res, 3000)) // 等 loadLayout 双读复原

    const wb = sheetEl.getWorkbook()
    r.sheetCount = wb.getSheetCount()
    const ws = wb.getActiveSheet()
    r.activeSheetName = ws ? ws.name() : null
    r.rowCount = ws ? ws.getRowCount() : 0

    let nonEmpty = 0; let sampleText = ''
    if (ws) for (let row = 0; row < Math.min(ws.getRowCount(), 40); row++) for (let col = 0; col < Math.min(ws.getColumnCount(), 12); col++) {
      const v = ws.getValue(row, col)
      if (v != null && String(v) !== '') { nonEmpty++; if (!sampleText && typeof v === 'string' && v.length > 1) sampleText = v }
    }
    r.nonEmptyCells = nonEmpty; r.sampleText = sampleText

    let edited = false
    try { edited = sheetEl._runUndoable('editCell', () => { ws.setValue(0, 0, ws.getValue(0, 0) ?? 'x') }) } catch (e) { r.editErr = String(e) }
    r.escapeHatchEditable = !!edited
    return r
  }, { pageId, props })
}

const out = await driveConsumer('portal.rpt.designer', { reportCode: REPORT, version: VERSION })
await page.waitForTimeout(300)
await page.screenshot({ path: join(here, 'preview-m6-portal.png') })

// 应用态：report-applier.js（同版式 SSJSON 双读 + 取数回填路径）
const outApplier = await driveConsumer('portal.rpt.report-applier', { reportCode: REPORT, version: VERSION, orgCode: '', periodCode: '' })
await page.waitForTimeout(300)
await page.screenshot({ path: join(here, 'preview-m6-portal-applier.png') })
await browser.close()
server.close()

console.log('=== M6 门户级 E2E（真 designer.js + report-applier.js + 真后端 + mega 内核）===')
console.log('--- designer (设计态) ---')
console.log(JSON.stringify(out, null, 2))
console.log('--- applier (应用态) ---')
console.log(JSON.stringify(outApplier, null, 2))

function checksFor (o, label) {
  return [
    [`${label}: content mounted`, o.steps?.includes('content mounted')],
    [`${label}: sheet + getWorkbook ready`, o.steps?.includes('sheet ready')],
    [`${label}: inner <cmx-megasheet> (自研内核)`, o.hasInnerMegasheet === true],
    [`${label}: layout cells restored (SSJSON 双读)`, o.nonEmptyCells > 0],
    [`${label}: escape-hatch editable`, o.escapeHatchEditable === true],
  ]
}
const checks = [...checksFor(out, 'designer'), ...checksFor(outApplier, 'applier')]
let allPass = true
for (const [name, ok] of checks) { console.log(`${ok ? '✅' : '❌'} ${name}`); if (!ok) allPass = false }
if (out.error) { allPass = false; console.log('❌ designer error:', out.error) }
if (outApplier.error) { allPass = false; console.log('❌ applier error:', outApplier.error) }
if (errors.length) {
  const real = errors.filter((e) => !/favicon|\.map\b|net::ERR_ABORTED/.test(e))
  if (real.length) { allPass = false; console.log('❌ page errors:'); real.forEach((e) => console.log('  ', e)) }
  else console.log('(ignored noise:', errors.length, ')')
}
console.log(allPass ? '\n=== M6 PORTAL E2E PASS (designer + applier) ===' : '\n=== M6 PORTAL E2E: FAILURES ===')
process.exit(allPass ? 0 : 1)
