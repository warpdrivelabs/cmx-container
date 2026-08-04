#!/usr/bin/env node
// 跨引擎 parity —— 前端参考实现的无头驱动。
//
// 加载真实 packages/cmx-data-comp/src/lib/cmx-master-slave.js（前端 CmxMasterSlave 本体），
// 用固定夹具 setFlatData + 声明式 aggregations 逐层上卷，把上卷后的扁平数据（getFlatData）
// 打到 stdout。Rust parity 测试读它，与后端 CmxMasterSlave 的输出逐字比。
//
// 用法：node ms-driver.mjs <fixture.json>  → stdout: { cv_batch:[...], cv_header:[...], ... }

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
// 定位 presentation 根：本文件在 cmx-container/crates/libs/cmx-doc/cmx-doc-store-pg/tests/parity/
// = presentation 下 7 层，故上溯 7 层。
const REPO = resolve(__dirname, '../'.repeat(7))
const MS_PATH = resolve(REPO, 'packages/cmx-data-comp/src/lib/cmx-master-slave.js')

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
