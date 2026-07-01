#!/usr/bin/env node
// build-help.mjs —— 帮助文档「源(.md) → 产物(.json)」构建器。
//
// 背景：后端(cmx-container Rust)读 data/help/<域>/<应用>/<模块>/<文件>.json，把 content 当
// markdown、examples[].code 当代码字符串直接用。JSON 里 content/code 满是 \n 转义，人手难编辑。
// 本脚本让你用「Markdown + YAML frontmatter」写源文件（正文真 markdown、代码用 ``` 围栏或 YAML |
// 块标量，零转义），构建时生成与后端逐字段等价的 JSON。**不改任何运行代码。**
//
// 源目录：  cmx-container/data/help-src/<域>/<应用>/<模块>/<id>.md
// 产物目录：cmx-container/data/help/<域>/<应用>/<模块>/<id>.json
//
// 用法：
//   node scripts/build-help.mjs           # 构建全部
//   node scripts/build-help.mjs --check   # 只校验「产物是否与源一致」，不写盘（CI/提交前用）
//   node scripts/build-help.mjs --clean   # 构建并删除「源已不存在」的孤儿 json
//
// 源文件结构（frontmatter 用 ---，正文是 markdown）：
//   ---
//   title: 录入凭证
//   summary: 一句话简介
//   keywords: [凭证, 录入]
//   path: 凭证管理            # 模块内分级目录，可空
//   order: 1
//   actions:                  # 可选：wsnode:#key / node: / menu: 的内联动作
//     damRegistry: { kind: node, id: portal-dam-registry }
//   examples:                 # 可选：property 区样例
//     - title: 请求体
//       lang: json
//       note: 说明（可选）
//       code: |
//         { "batchId": "B-1" }
//   ---
//   # 正文标题
//   markdown 正文……[站内跳转](help:other-id) [打开功能](wsnode:#damRegistry)

import { promises as fs } from 'node:fs'
import path from 'node:path'
import url from 'node:url'
import yaml from 'js-yaml'

const HERE = path.dirname(url.fileURLToPath(import.meta.url))
const ROOT = path.resolve(HERE, '..')              // cmx-container/
const SRC_DIR = path.join(ROOT, 'data', 'help-src')
const OUT_DIR = path.join(ROOT, 'data', 'help')

const args = new Set(process.argv.slice(2))
const CHECK = args.has('--check')
const CLEAN = args.has('--clean')

/** 递归列出某目录下全部指定后缀文件（相对 base 的 posix 路径）。 */
async function listFiles (base, ext) {
  const out = []
  async function walk (dir) {
    let entries
    try { entries = await fs.readdir(dir, { withFileTypes: true }) } catch (e) {
      if (e.code === 'ENOENT') return
      throw e
    }
    for (const ent of entries) {
      if (ent.name.startsWith('.')) continue
      const abs = path.join(dir, ent.name)
      if (ent.isDirectory()) await walk(abs)
      else if (ent.isFile() && ent.name.endsWith(ext)) out.push(abs)
    }
  }
  await walk(base)
  return out.sort()
}

/** 拆分 frontmatter 与正文。frontmatter 用首尾 --- 包裹。 */
function splitFrontmatter (text, relForErr) {
  const t = text.replace(/^﻿/, '') // 去 BOM
  const m = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/.exec(t)
  if (!m) {
    throw new Error(`[${relForErr}] 缺少 YAML frontmatter（文件须以 --- 开头、--- 结尾包裹元数据）`)
  }
  let meta
  try {
    meta = yaml.load(m[1]) || {}
  } catch (e) {
    throw new Error(`[${relForErr}] frontmatter YAML 解析失败：${e.message}`)
  }
  if (typeof meta !== 'object' || Array.isArray(meta)) {
    throw new Error(`[${relForErr}] frontmatter 必须是键值对象`)
  }
  return { meta, body: m[2].replace(/^\r?\n/, '').replace(/\s+$/, '') }
}

/** 从 src 相对路径推导 DAM 段：<域>/<应用>/<模块>/<id>.md → {domain,app,module,id,file}。 */
function damFromRel (rel) {
  const parts = rel.split('/')
  if (parts.length !== 4) {
    throw new Error(`[${rel}] 路径层级须为 域/应用/模块/<id>.md（4 段），实际 ${parts.length} 段`)
  }
  const [domain, app, module, fname] = parts
  if (!fname.endsWith('.md')) throw new Error(`[${rel}] 源文件须 .md 结尾`)
  const id = fname.slice(0, -3)
  const SEG = /^[a-zA-Z0-9_-]+$/
  for (const [k, v] of [['domain', domain], ['app', app], ['module', module], ['id', id]]) {
    if (!SEG.test(v)) throw new Error(`[${rel}] ${k} 段非法（仅允许字母数字 _-）："${v}"`)
  }
  return { domain, app, module, id, file: `${id}.json` }
}

/** 规范化 examples：保证字段顺序与类型与现有 JSON 一致（title/lang/note/code）。 */
function normExamples (examples, rel) {
  if (examples == null) return []
  if (!Array.isArray(examples)) throw new Error(`[${rel}] examples 必须是数组`)
  return examples.map((e, i) => {
    if (e == null || typeof e !== 'object') throw new Error(`[${rel}] examples[${i}] 必须是对象`)
    const out = {}
    if (e.title != null) out.title = String(e.title)
    if (e.lang != null) out.lang = String(e.lang)
    if (e.note != null) out.note = String(e.note)
    // code 用 YAML | 块标量写，原样保留；去掉块标量尾随的单个换行。
    out.code = e.code != null ? String(e.code).replace(/\n$/, '') : ''
    return out
  })
}

/** 把一个源文件编译成后端 JSON 对象（与现有字段逐一对齐）。 */
function compile (rel, text) {
  const { meta, body } = splitFrontmatter(text, rel)
  const dam = damFromRel(rel)
  const doc = {
    domain: dam.domain,
    app: dam.app,
    module: dam.module,
    file: dam.file,
    id: dam.id,
    path: meta.path != null ? String(meta.path).trim().replace(/^\/+|\/+$/g, '') : '',
    title: meta.title != null ? String(meta.title) : dam.id,
    summary: meta.summary != null ? String(meta.summary) : '',
    keywords: Array.isArray(meta.keywords) ? meta.keywords.map(String) : [],
    order: Number.isFinite(meta.order) ? Math.trunc(meta.order) : 0,
    content: body,
    examples: normExamples(meta.examples, rel),
  }
  // actions 可选：有才输出（与后端 skip_serializing_if=null 对齐；空则不写该键）。
  if (meta.actions != null && typeof meta.actions === 'object' && Object.keys(meta.actions).length) {
    doc.actions = meta.actions
  }
  // updatedAt：构建期戳一次（与后端 save 行为一致；--check 比较时忽略此字段）。
  doc.updatedAt = 0
  return doc
}

/** 稳定的字段顺序输出，便于和现有 JSON diff。 */
function stringify (doc) {
  const ordered = {}
  for (const k of ['domain', 'app', 'module', 'file', 'id', 'path', 'title', 'summary', 'keywords', 'order', 'content', 'examples']) {
    ordered[k] = doc[k]
  }
  if ('actions' in doc) ordered.actions = doc.actions
  ordered.updatedAt = doc.updatedAt
  return JSON.stringify(ordered, null, 2) + '\n'
}

/** 比较时忽略 updatedAt 的「内容等价」判断。 */
function sameIgnoringStamp (aText, bDoc) {
  try {
    const a = JSON.parse(aText)
    const b = JSON.parse(JSON.stringify(bDoc))
    a.updatedAt = 0; b.updatedAt = 0
    return JSON.stringify(a) === JSON.stringify(b)
  } catch { return false }
}

async function main () {
  const srcFiles = await listFiles(SRC_DIR, '.md')
  if (!srcFiles.length) {
    console.error(`未找到任何源文件：${path.relative(ROOT, SRC_DIR)}/**/*.md`)
    process.exit(1)
  }
  let built = 0, unchanged = 0, drift = 0, docs = 0
  const expectedOut = new Set()

  for (const abs of srcFiles) {
    const rel = path.relative(SRC_DIR, abs).split(path.sep).join('/')
    // 仅处理「域/应用/模块/<id>.md」(4 段) 的文档；README、说明等非 4 段文件跳过。
    if (rel.split('/').length !== 4) continue
    docs++
    const text = await fs.readFile(abs, 'utf8')
    const doc = compile(rel, text)
    const outAbs = path.join(OUT_DIR, doc.domain, doc.app, doc.module, doc.file)
    expectedOut.add(outAbs)
    const nextText = stringify(doc)

    let prevText = null
    try { prevText = await fs.readFile(outAbs, 'utf8') } catch { /* 不存在 */ }

    if (CHECK) {
      if (prevText == null || !sameIgnoringStamp(prevText, doc)) {
        console.error(`✗ 漂移：${path.relative(ROOT, outAbs)} 与源 ${rel} 不一致`)
        drift++
      } else {
        unchanged++
      }
      continue
    }

    if (prevText != null && sameIgnoringStamp(prevText, doc)) {
      unchanged++
      continue
    }
    await fs.mkdir(path.dirname(outAbs), { recursive: true })
    await fs.writeFile(outAbs, nextText, 'utf8')
    console.log(`✓ 生成 ${path.relative(ROOT, outAbs)}  ← ${rel}`)
    built++
  }

  if (CLEAN && !CHECK) {
    const existing = await listFiles(OUT_DIR, '.json')
    for (const abs of existing) {
      if (!expectedOut.has(abs)) {
        await fs.rm(abs)
        console.log(`🗑  删除孤儿 ${path.relative(ROOT, abs)}（源已不存在）`)
      }
    }
  }

  if (CHECK) {
    if (drift) { console.error(`\n校验失败：${drift} 个产物与源不一致，请运行 node scripts/build-help.mjs 重新生成。`); process.exit(1) }
    console.log(`校验通过：${unchanged} 个产物均与源一致。`)
  } else {
    console.log(`\n完成：生成/更新 ${built}，未变 ${unchanged}，共 ${docs} 篇。`)
  }
}

main().catch((e) => { console.error(e.message || e); process.exit(1) })
