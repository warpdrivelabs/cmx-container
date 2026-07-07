// 端到端前端侧基准 —— 配合 e2e-server(Rust)跑三链路对比。
//
// 用 Node(V8,与 Chrome 同引擎)驱动**真实前端模块**:
//   - packages/cmx-data-comp/src/lib/cmx-msgpack-decode.js  (二进制解码,将上浏览器的实现)
//   - packages/cmx-data-comp/src/lib/cmx-data-set.js        (CmxDataSet.fromJSON → 展示模型)
//
// 每链路测:下载耗时/体积 → 解析(JSON.parse | decodeMsgpack) → 展示构建(fromJSON,
// 组件读的 dataset.rows 即此产物)。内存用 V8 heapUsed 差值(GC 后对齐)。
//
// 用法:
//   node --expose-gc e2e_bench.mjs [serverBase] [rounds]
//   默认 http://127.0.0.1:18099 、3 轮取中位。
//
// 输出:终端表格 + E2E_RESULTS.md(与服务端响应头指标合并)。

import { writeFileSync } from 'fs'
import { decodeMsgpack } from '../../../../packages/cmx-data-comp/src/lib/cmx-msgpack-decode.js'
import { CmxDataSet } from '../../../../packages/cmx-data-comp/src/lib/cmx-data-set.js'

const BASE = process.argv[2] || 'http://127.0.0.1:18099'
const ROUNDS = Number(process.argv[3] || 3)

function gcNow () {
  if (globalThis.gc) { globalThis.gc(); globalThis.gc() }
}
function heap () {
  gcNow()
  return process.memoryUsage().heapUsed
}
const MB = (b) => (b / 1048576)
const med = (a) => { const s = [...a].sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2 }

async function measureOnce (path, binary) {
  const url = `${BASE}${path}`

  // ── 下载 ──
  const h0 = heap()
  const t0 = performance.now()
  const res = await fetch(url, { headers: { Accept: binary ? 'application/x-msgpack' : 'application/json' } })
  const buf = new Uint8Array(await res.arrayBuffer())
  const tDownload = performance.now() - t0
  const bytes = buf.length
  const server = {
    fetchMs: Number(res.headers.get('x-t-fetch-ms')),
    encodeMs: Number(res.headers.get('x-t-encode-ms')),
    memTotalB: Number(res.headers.get('x-mem-total-b')),
    memPeakB: Number(res.headers.get('x-mem-peak-b')),
    rows: Number(res.headers.get('x-rows')),
  }
  const hAfterDownload = heap()

  // ── 解析(传输格式 → JS 对象) ──
  const t1 = performance.now()
  let body
  if (binary) {
    body = decodeMsgpack(buf)
  } else {
    body = JSON.parse(new TextDecoder().decode(buf))
  }
  const tParse = performance.now() - t1
  if (body.code !== 0) throw new Error(`bad envelope: ${path}`)
  const hAfterParse = heap()

  // ── 展示构建(列式包 → CmxDataSet,组件读 dataset.rows 的产物) ──
  const t2 = performance.now()
  const ds = CmxDataSet.fromJSON(body.data)
  const tBuild = performance.now() - t2
  const hAfterBuild = heap()

  // 触碰展示模型,防优化 + 校验行数
  const rows = ds.rows
  if (rows.length !== server.rows) throw new Error(`行数不符 ${rows.length} != ${server.rows} (${path})`)
  let sink = 0
  for (let i = 0; i < rows.length; i += 9973) sink += Number(rows[i].id) || 0
  if (sink < 0) console.log(sink)

  return {
    bytes,
    tDownload,
    tParse,
    tBuild,
    memDownload: hAfterDownload - h0,
    memParse: hAfterParse - hAfterDownload,
    memBuild: hAfterBuild - hAfterParse,
    memTotal: hAfterBuild - h0,
    server,
  }
}

async function measure (name, path, binary) {
  const runs = []
  for (let i = 0; i < ROUNDS; i++) {
    runs.push(await measureOnce(path, binary))
    gcNow()
  }
  const m = (f) => med(runs.map(f))
  return {
    name,
    bytes: runs[0].bytes,
    rows: runs[0].server.rows,
    svrFetchMs: m((r) => r.server.fetchMs),
    svrEncodeMs: m((r) => r.server.encodeMs),
    svrMemTotalMB: MB(m((r) => r.server.memTotalB)),
    svrMemPeakMB: MB(m((r) => r.server.memPeakB)),
    tDownload: m((r) => r.tDownload),
    tParse: m((r) => r.tParse),
    tBuild: m((r) => r.tBuild),
    feMemTotalMB: MB(m((r) => r.memTotal)),
    feMemParseMB: MB(m((r) => r.memParse)),
    feMemBuildMB: MB(m((r) => r.memBuild)),
    totalMs: m((r) => r.server.fetchMs + r.server.encodeMs + r.tDownload + r.tParse + r.tBuild),
  }
}

function fmtRow (r) {
  return `| ${r.name} | ${MB(r.bytes).toFixed(1)} MB | ${r.svrFetchMs.toFixed(0)} | ${r.svrEncodeMs.toFixed(0)} | ${r.svrMemPeakMB.toFixed(0)} MB | ${r.tDownload.toFixed(0)} | ${r.tParse.toFixed(0)} | ${r.tBuild.toFixed(0)} | ${r.feMemTotalMB.toFixed(0)} MB | **${r.totalMs.toFixed(0)}** |`
}

const old = await measure('老链路 sqlx/DataSet→JSON', '/old/json', false)
const szmc = await measure('sqlx/Zmc流式→msgpack', '/sqlx/zmc.bin', true)
const tzmc = await measure('tokio/Zmc流式→msgpack', '/tokio/zmc.bin', true)

let md = `# 端到端三链路对比:DB → HTTP → 前端展示模型

> 表:50 列宽表 · 行数:${old.rows} · ${ROUNDS} 轮取中位 · 服务端计数分配器 + 前端 V8 heapUsed(--expose-gc)
> 前端侧运行**真实前端模块**(cmx-msgpack-decode.js / CmxDataSet.fromJSON);「展示构建」产物即组件读的 dataset.rows

## 全链路分环节(用时 ms / 内存 MB)

| 链路 | 传输体积 | 服<br>取数 | 服<br>编码 | 服端<br>峰值内存 | 前<br>下载 | 前<br>解析 | 前<br>展示构建 | 前端<br>堆增量 | 端到端<br>总用时 |
|------|---------|-----|-----|--------|-----|-----|--------|--------|---------|
${fmtRow(old)}
${fmtRow(szmc)}
${fmtRow(tzmc)}

## 前端内存分解(MB,GC 后 heapUsed 差)

| 链路 | 下载(字节缓冲) | 解析(JS对象) | 展示构建(CmxDataSet) | 合计 |
|------|--------------|-------------|--------------------|------|
| ${old.name} | ${MB(old.bytes).toFixed(0)} | ${old.feMemParseMB.toFixed(0)} | ${old.feMemBuildMB.toFixed(0)} | ${old.feMemTotalMB.toFixed(0)} |
| ${szmc.name} | ${MB(szmc.bytes).toFixed(0)} | ${szmc.feMemParseMB.toFixed(0)} | ${szmc.feMemBuildMB.toFixed(0)} | ${szmc.feMemTotalMB.toFixed(0)} |
| ${tzmc.name} | ${MB(tzmc.bytes).toFixed(0)} | ${tzmc.feMemParseMB.toFixed(0)} | ${tzmc.feMemBuildMB.toFixed(0)} | ${tzmc.feMemTotalMB.toFixed(0)} |

## 与老链路的差异

| 指标 | 老链路 | sqlx/Zmc流式 | tokio/Zmc流式 |
|------|--------|--------------|----------------|
| 传输体积 | ${MB(old.bytes).toFixed(1)} MB | ${MB(szmc.bytes).toFixed(1)} MB (${(szmc.bytes / old.bytes * 100).toFixed(0)}%) | ${MB(tzmc.bytes).toFixed(1)} MB (${(tzmc.bytes / old.bytes * 100).toFixed(0)}%) |
| 服端峰值内存 | ${old.svrMemPeakMB.toFixed(0)} MB | ${szmc.svrMemPeakMB.toFixed(0)} MB (省 ${(100 - szmc.svrMemPeakMB / old.svrMemPeakMB * 100).toFixed(0)}%) | ${tzmc.svrMemPeakMB.toFixed(0)} MB (省 ${(100 - tzmc.svrMemPeakMB / old.svrMemPeakMB * 100).toFixed(0)}%) |
| 端到端总用时 | ${old.totalMs.toFixed(0)} ms | ${szmc.totalMs.toFixed(0)} ms | ${tzmc.totalMs.toFixed(0)} ms |
| 前端解析用时 | ${old.tParse.toFixed(0)} ms (JSON.parse) | ${szmc.tParse.toFixed(0)} ms (decodeMsgpack) | ${tzmc.tParse.toFixed(0)} ms (decodeMsgpack) |
| 前端展示构建 | ${old.tBuild.toFixed(0)} ms | ${szmc.tBuild.toFixed(0)} ms | ${tzmc.tBuild.toFixed(0)} ms(同一 fromJSON) |

---
注:本机回环(127.0.0.1)下载耗时不含真实网络 RTT/带宽;真实网络下「传输体积小 ${(100 - szmc.bytes / old.bytes * 100).toFixed(0)}%」直接折算为下载时间优势。前端解析:JSON.parse 是 V8 原生 C++,手写 JS msgpack 解码器与之对比见数据。
`

console.log(md)
writeFileSync(new URL('./E2E_RESULTS.md', import.meta.url), md)
console.log('已写入 E2E_RESULTS.md')
