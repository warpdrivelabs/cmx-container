#!/usr/bin/env node
// 跨引擎 parity —— 前端参考实现的无头驱动。
//
// 加载真实 packages/cmx-data-comp/src/lib/cmx-master-slave.js（前端 CmxMasterSlave 本体），
// 用固定夹具 setFlatData + 声明式 aggregations 逐层上卷，把上卷后的扁平数据（getFlatData）
// 打到 stdout。Rust parity 测试读它，与后端 CmxMasterSlave 的输出逐字比。
//
// 用法：node ms-driver.mjs <fixture.json>  → stdout: { cv_batch:[...], cv_header:[...], ... }

import { readFileSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
// 定位工作区根（packages/cmx-data-comp 所在仓）：自本文件逐层上溯探测——crate 曾先后驻留
// cmx-container 与 cmx-model 两仓，层数随迁仓漂移，硬编码不可靠，探测到即停。
let MS_PATH = null
for (let dir = __dirname, i = 0; i < 10; i++, dir = resolve(dir, '..')) {
  const p = resolve(dir, 'packages/cmx-data-comp/src/lib/cmx-master-slave.js')
  if (existsSync(p)) { MS_PATH = p; break }
}
if (!MS_PATH) throw new Error('上溯 10 层未找到 packages/cmx-data-comp/src/lib/cmx-master-slave.js')

const { CmxMasterSlave } = await import(MS_PATH)

const fixturePath = process.argv[2] || resolve(__dirname, 'fixture.json')
const fx = JSON.parse(readFileSync(fixturePath, 'utf8'))

// 4 层嵌套 schema（与夹具 aggregations 的点分路径一致）
const schema = [{
  id: 'cv_batch',
  children: [{
    id: 'cv_header',
    children: [{
      id: 'cv_acc_line',
      children: [{ id: 'cv_aux_line' }],
    }],
  }],
}]

// relations：父子经 id→upper_id（与后端一致）
const relations = [
  { parent: 'cv_batch', child: 'cv_header', parentKey: 'id', childKey: 'upper_id' },
  { parent: 'cv_header', child: 'cv_acc_line', parentKey: 'id', childKey: 'upper_id' },
  { parent: 'cv_acc_line', child: 'cv_aux_line', parentKey: 'id', childKey: 'upper_id' },
]

const ms = new CmxMasterSlave({ schema, relations, aggregations: fx.aggregations })
ms.setFlatData(fx.flat)   // 内部 setDataSet 后 _runAggregations 预热上卷一次

// 上卷后的扁平数据：每表一组行，度量已按 aggregations 逐层求和回写
const out = ms.getFlatData()
process.stdout.write(JSON.stringify(out))
