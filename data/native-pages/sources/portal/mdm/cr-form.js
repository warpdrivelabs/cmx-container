/**
 * MDM 供应商新增/变更表单页（native-page · 并列标签页）。
 * 由列表页 openNode 打开，经 host.workspace.context 读 { mode:'create'|'update', supplier }。
 *
 * 保存走平台标准单据保存链路：C.saveDocData → POST /doc/save（坐标 basic/dataplatform/mdm），
 * doc_no 由 cmx-code 按 MDM_GYS 规则铸号，前端不传 doc_no。
 * NOT NULL 列（doc_status/line_no/doc_type_id/doc_date/entity_id）由前端显式提供；
 * JSONB 列（field_deltas/line_payload）传对象（不序列化）。
 * 组件库未加载时直接报错（不降级裸 fetch）——加载失败属异常配置，应显式暴露。
 * 流转（submit）仍调 MDM 专属接口 /mdm/change-requests/submit。
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
async function apiPost(url, payload, dbId) {
  const h = { 'Content-Type': 'application/json', Accept: 'application/json' }; if (dbId) h.db_id = dbId
  const r = await fetch(url, { method: 'POST', headers: h, credentials: 'same-origin', body: JSON.stringify(payload || {}) })
  return unwrap(r, await r.json().catch(() => null))
}

// payload 业务字段（name 提升为 subject_name 公共搜索列，其余下沉到 payload JSONB）
const PAYLOAD_FIELDS = ['tax_no', 'credit_code', 'short_name']
// step：create 模式初始 1（先查重），update 模式初始 2（改已有记录，跳过查重）
const state = { dbId: '', mode: 'create', step: 1, keyName: '', supplier: null, bankLines: [], savedCrId: null, savedLineIdMap: {} }
let rootEl = null
const q = (id) => rootEl && rootEl.querySelector('#' + id)
const val = (id) => { const el = q(id); return el ? (el.value || '').trim() : '' }

function styleCss() {
  return `
  .pg { height:100%; display:flex; flex-direction:column; gap:6px; box-sizing:border-box; padding:8px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor); overflow:auto;
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  /* 分区卡片（对齐设计器 voucher-detail 范式） */
  .sec { border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0); border-radius:6px; overflow:hidden;
    background:var(--sapList_Background,#fff); }
  .sec-hd { display:flex; align-items:center; justify-content:space-between; gap:8px;
    padding:6px 10px; border-bottom:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0);
    background:var(--sapGroup_TitleBackground,transparent); }
  .sec-hd-l { display:flex; align-items:center; gap:6px; }
  .sec-hd-r { display:flex; gap:4px; align-items:center; }
  .sec-hd ui5-icon { color:var(--sapInformativeTextColor,var(--sapHighlightColor)); font-size:1rem; }
  .sec-t { margin:0; font-weight:700; color:var(--sapTitleColor); font-size:0.95rem; }
  .sec-bd { padding:8px 10px; box-sizing:border-box; }
  /* 表头表单卡片：自适应高度 */
  .sec-head { flex:0 0 auto; }
  /* 明细表格卡片：撑满剩余，内部 grid 滚动 */
  .sec-grid { flex:1 1 0; display:flex; flex-direction:column; min-height:120px; }
  .sec-grid .sec-bd { flex:1; min-height:0; padding:0; display:flex; flex-direction:column; }
  .tbl-wrap { flex:1; min-height:0; display:flex; flex-direction:column; }
  .tbl-wrap cmx-revo-grid { display:flex; width:100%; flex:1 1 0%; min-width:0; min-height:0; flex-direction:column; }
  cmx-toolbar { display:block; }
  /* 步骤指示器（步骤条）：仅在 create 模式显示 */
  .step-bar { display:flex; align-items:center; gap:6px; padding:6px 10px;
    background:var(--sapList_Background,#fff); border:1px solid var(--sapGroup_ContentBorderColor,#e0e0e0);
    border-radius:6px; margin-bottom:6px; font-size:0.85rem; color:var(--sapContent_LabelColor); }
  .step-bar .step { display:flex; align-items:center; gap:4px; }
  .step-bar .step .num { display:inline-flex; align-items:center; justify-content:center;
    width:20px; height:20px; border-radius:50%; font-size:0.75rem; font-weight:700;
    background:var(--sapButton_Emphasized_Background,#0a6ed1); color:#fff; }
  .step-bar .step.done .num { background:var(--sapSuccessBorderColor,#2b7c2b); }
  .step-bar .step.cur .num { background:var(--sapButton_Emphasized_Background,#0a6ed1); }
  .step-bar .step.pending .num { background:var(--sapNeutralBorderColor,#899191); }
  .step-bar .sep { color:var(--sapContent_DisabledTextColor); }
  /* 步骤切换的过渡态：按钮区按步骤显隐 */
  .step-actions { display:flex; gap:6px; align-items:center; }
  `
}

function viewHtml() {
  const isEdit = state.mode === 'update'
  // update 模式：查重无意义（改已有记录），保持单页无步骤条。
  // create 模式：2 步步骤条（步骤1 关键信息查重 → 步骤2 完整信息 + 明细）。
  const showSteps = !isEdit
  const step = state.step
  // 步骤条（仅 create 模式）：步骤1 关键信息 / 步骤2 完整信息
  const stepBarHtml = showSteps ? `<div class="step-bar">
      <div class="step ${step >= 1 ? (step > 1 ? 'done' : 'cur') : 'pending'}"><span class="num">1</span><span>关键信息</span></div>
      <span class="sep">→</span>
      <div class="step ${step >= 2 ? 'cur' : 'pending'}"><span class="num">2</span><span>完整信息</span></div>
    </div>` : ''
  // 顶部操作区：create 步骤1 → 「下一步」；create 步骤2 → 空（操作移到底部步骤2 区）；update → 「保存草稿」+「保存并提交」
  let topActions = ''
  if (showSteps && step === 1) {
    topActions = `<ui5-button design="Emphasized" icon="navigation-right-arrow" id="fNext">下一步</ui5-button>`
  } else if (isEdit) {
    topActions = `<ui5-button design="Default" icon="save" id="fSave">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit">保存并提交</ui5-button>`
  }
  // 步骤1 表单卡片（仅 create + step===1 显示）
  const keyFormCard = (showSteps && step === 1) ? `<div class="sec sec-head">
      <div class="sec-hd"><div class="sec-hd-l">
        <ui5-icon name="add-document" design="Default" mode="Decorative"></ui5-icon>
        <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">关键信息</ui5-title>
      </div></div>
      <div class="sec-bd" id="fKeyForm"></div>
    </div>` : ''
  // 步骤2 / update 表单卡片（完整信息）
  const fullFormVisible = !showSteps || step === 2
  const fullFormCard = fullFormVisible ? `<div class="sec sec-head">
      <div class="sec-hd"><div class="sec-hd-l">
        <ui5-icon name="add-document" design="Default" mode="Decorative"></ui5-icon>
        <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">${showSteps ? '完整信息' : '基本信息'}</ui5-title>
      </div>
      </div>
      <div class="sec-bd" id="fForm"></div>
    </div>` : ''
  // 步骤2 / update 明细卡片（银行账户）
  const gridCard = fullFormVisible ? `<div class="sec sec-grid">
      <div class="sec-hd">
        <div class="sec-hd-l">
          <ui5-icon name="accounting-document-verification" design="Default" mode="Decorative"></ui5-icon>
          <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">银行账户</ui5-title>
        </div>
        <div class="sec-hd-r">
          <ui5-button design="Default" icon="add" id="fAddRow">增行</ui5-button>
          <ui5-button design="Transparent" icon="delete" id="fDelRow">删选中</ui5-button>
        </div>
      </div>
      <div class="sec-bd"><div class="tbl-wrap" id="fGrid"></div></div>
    </div>` : ''
  // 步骤2 底部操作区（create 步骤2：返回 + 保存）；update 无步骤2概念，顶部已有操作
  const bottomActions = (showSteps && step === 2) ? `<div class="step-actions" style="margin-top:6px;">
      <ui5-button design="Transparent" icon="navigation-left-arrow" id="fPrev">上一步</ui5-button>
      <ui5-button design="Default" icon="save" id="fSave2">保存草稿</ui5-button>
      <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit2">保存并提交</ui5-button>
    </div>` : ''
  return `<div class="pg">
    <ui5-bar design="Header" accessible-role="Toolbar">
      <ui5-label wrapping-type="Normal" style="font-weight:800;font-size:1.05rem;color:var(--sapShellTitleColor,var(--sapTitleColor));">${isEdit ? '变更供应商' : '新增供应商'}</ui5-label>
      <div slot="endContent" style="display:flex;gap:4px;">${topActions}</div>
    </ui5-bar>
    ${stepBarHtml}
    ${keyFormCard}
    ${fullFormCard}
    ${gridCard}
    ${bottomActions}
  </div>`
}

// 银行行用组件库 cmx-revo-grid（可编辑）。增行用 CmxDataSet.addRow（触发 _refreshSource）+
// refreshLayout 双保险，保证新行即时可见；容器 .bank-fill 有最低高度。
let bankGrid = null
let headForm = null
let keyForm = null
let lineSeq = 0
const newLine = () => { lineSeq += 1; return { id: `nl_${Date.now()}_${lineSeq}`, account_no: '', bank_name: '' } }

// 查重：POST /api/mdm/check-key。步骤1 只采集 name，故 specs 仅 name 项（weight=100 EditDistance），
// clusterKeys 按 name 分块。返回 {exists:false} 或 {exists:true,id,code,message}。
async function checkKey(name) {
  return apiPost('/api/mdm/check-key', {
    dictCode: 'supplier', targetTable: 'cm_supplier',
    keyValue: { name },
    specs: [{ field: 'name', weight: 100, kind: 'EditDistance' }],
    clusterKeys: ['name'],
  }, state.dbId)
}

// 步骤1 表单（cmx-ui5-form，仅 name 字段）。create 模式专用。
function buildKeyForm() {
  const C = cmx(); const wrap = q('fKeyForm'); if (!wrap) return
  wrap.innerHTML = ''
  const form = document.createElement('cmx-ui5-form')
  form.classList.add('cmx-form-neo')
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'supplierKey' })
    cm.setMembers([
      new C.CmxColumn({ id: 'name', caption: '供应商名称', dataType: 'VARCHAR', required: true, edit: { mode: 'text' } }),
    ])
    form.setColumnModel(cm)
  }
  form.setLayout?.('S1 M1 L2 XL2')
  form.setDataSet?.({ name: state.keyName || '' })
  wrap.appendChild(form); keyForm = form
}

// 步骤切换：进入指定步骤并 refresh。
function goStep(n) {
  state.step = n; refresh()
}

// 「下一步」处理：取步骤1 name → 必填校验 → checkKey 查重 → 通过则 keyName 入 state 并进步骤2。
async function onNext() {
  const C = cmx()
  const row = (keyForm && keyForm.getData && keyForm.getData()) || {}
  const name = (row.name || '').trim()
  if (!name) { C.cmxWarn?.('供应商名称不能为空'); return }
  try {
    const d = await checkKey(name)
    if (d && d.exists) {
      // 查重命中：弹框提示后端 message，阻断在步骤1
      C.cmxError?.(d.message || `已存在相似供应商（id=${d.id ?? ''}${d.code ? '，code=' + d.code : ''}），请确认是否继续`)
      return
    }
    state.keyName = name
    goStep(2)
  } catch (e) {
    C.cmxError?.(`查重失败：${e.message}`)
  }
}

// 顶部表单用 cmx-ui5-form（字段定义来自 CmxColumnModel，默认 Neo 皮肤）
// 含义随模式/步骤变化：
//   - create 步骤2：完整信息表单（name 来自步骤1，作只读字段回显；字段 tax_no/credit_code/short_name 可编辑）
//   - update：基本信息表单（含可编辑 name + 三个 payload 字段，单页）
// create 步骤2 的 name 只读展示（edit.mode='readonly'）——值在 setDataSet 时从 state.keyName 带入，
// getData 仍会返回它，buildHead 据 isEdit 决定从表单还是从 state.keyName 取值，互不干扰。
function buildForm() {
  const C = cmx(); const wrap = q('fForm'); if (!wrap) return
  wrap.innerHTML = ''
  const form = document.createElement('cmx-ui5-form')
  form.classList.add('cmx-form-neo')
  const isEdit = state.mode === 'update'
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'supplierHead' })
    const members = []
    if (isEdit) {
      // update：name 可编辑（改已有记录，查重无意义）
      members.push(new C.CmxColumn({ id: 'name', caption: '供应商名称', dataType: 'VARCHAR', required: true, edit: { mode: 'text' } }))
    } else {
      // create 步骤2：name 在步骤1 已填并查重通过，此处仅只读回显，不可改
      members.push(new C.CmxColumn({ id: 'name', caption: '供应商名称', dataType: 'VARCHAR', required: true, edit: { mode: 'readonly' } }))
    }
    members.push(new C.CmxColumn({ id: 'tax_no', caption: '税号', dataType: 'VARCHAR', edit: { mode: 'text' } }))
    members.push(new C.CmxColumn({ id: 'credit_code', caption: '统一社会信用代码', dataType: 'VARCHAR', edit: { mode: 'text' } }))
    members.push(new C.CmxColumn({ id: 'short_name', caption: '简称', dataType: 'VARCHAR', edit: { mode: 'text' } }))
    cm.setMembers(members)
    form.setColumnModel(cm)
  }
  form.setLayout?.('S1 M2 L3 XL3')
  // update：回填已有记录字段（payload 字段从 supplier.payload 解出来铺到表单顶层）
  // create 步骤2：把步骤1 缓存的 keyName 带入 name（只读回显）
  if (isEdit) {
    const o = state.supplier || {}
    const p = (o.payload && typeof o.payload === 'object') ? o.payload : {}
    form.setDataSet?.({
      name: o.subject_name != null ? o.subject_name : (o.name || ''),
      tax_no: p.tax_no != null ? p.tax_no : '',
      credit_code: p.credit_code != null ? p.credit_code : '',
      short_name: p.short_name != null ? p.short_name : '',
    })
  } else {
    form.setDataSet?.({ name: state.keyName || '' })
  }
  wrap.appendChild(form); headForm = form
}

// 银行账户用 cmx-revo-grid（可编辑）。列成员键必须用 id（CmxColumn 以 id 为字段键）。
// 顺序对齐 data-editor：先 appendChild 入 DOM，再 setColumnModel/setOptions，等两帧后 setDataSet。
function bindBankGrid() {
  const C = cmx(); const wrap = q('fGrid'); if (!wrap) return
  wrap.innerHTML = ''
  const grid = document.createElement('cmx-revo-grid')
  // 主内容区可编辑表格：套 Neo 皮肤 + 声明式 fill-height（与设计器详情页一致）。
  grid.setAttribute('data-cmx-fill-height', '')
  grid.setAttribute('data-cmx-options', '{"editable":true,"showTotals":false,"showRequiredMark":false}')
  grid.classList.add('cmx-grid-neo')
  wrap.appendChild(grid); bankGrid = grid
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'bankLines' })
    cm.setMembers([
      new C.CmxColumn({ id: 'account_no', caption: '银行账号', dataType: 'VARCHAR', width: '260px', edit: { mode: 'cmx-text-input' } }),
      new C.CmxColumn({ id: 'bank_name', caption: '开户行', width: '260px', edit: { mode: 'cmx-text-input' } }),
    ])
    grid.setColumnModel(cm)
  }
  grid.setOptions?.({ editable: true, fillHeight: true, showRowIndex: true, selectionMode: 'multi', showTotals: false })
  const fill = () => {
    if (C.CmxDataSet) { const ds = new C.CmxDataSet({}); ds.setRows([newLine()]); grid.setDataSet(ds) }
    else grid.setDataSet?.([newLine()])
    grid.refreshLayout?.()
  }
  requestAnimationFrame(() => requestAnimationFrame(fill))
}
// 标准单据保存坐标（basic/dataplatform/mdm）。saveDocData 据此拼 ?domain=&application=&module=&file=。
const DOC_DEF = { domain: 'basic', application: 'dataplatform', module: 'mdm', file: 'dataplatform_doc_meta_v1.json' }
const TABLE_NAMES = ['cv_mdm_apply', 'cv_mdm_apply_line']
const HEAD_TID = 't1' // 头行临时 id；后端 mint_ids 铸真号后经 idMap 回传

// 当天日期串（doc_date NOT NULL 占位）。YYYY-MM-DD。
function todayStr() { const d = new Date(); const z = (n) => String(n).padStart(2, '0'); return `${d.getFullYear()}-${z(d.getMonth() + 1)}-${z(d.getDate())}` }

// 收集银行账户明细为标准 changeset 行。
// 首次保存的行（无 _savedId）→ inserted（用临时 id，后端铸号后 idMap 回传）；
// 已落库的行（有 _savedId，来自上次保存的 idMap）→ updated（带真实 id）。
// line_payload 传对象（不序列化）——JSONB 列经 json_to_dv_loose 绑成 DataValue::Json；
// 传字符串会绑成 Text 被 PG jsonb 列拒收。line_no NOT NULL，按序填。
function collectLines() {
  const ds = bankGrid?.getDataSet?.()
  const rows = ds ? (ds.toPlainRows ? ds.toPlainRows() : (ds.getRows ? ds.getRows() : [])) : []
  const filled = rows.filter((r) => (r.account_no || r.bank_name))
  const inserted = []
  const updated = []
  filled.forEach((r, i) => {
    const payload = { account_no: r.account_no || '', bank_name: r.bank_name || '' }
    const upperId = state.savedCrId != null ? state.savedCrId : HEAD_TID
    if (r._savedId != null) {
      // 已落库行 → updated（行 id 为上次保存回传的真实 id）
      updated.push({ id: r._savedId, fields: { line_no: i + 1, line_payload: payload } })
    } else {
      // 新行 → inserted（临时 id 用于本次 idMap 回传）
      inserted.push({ id: r.id, upper_id: upperId, line_no: i + 1, fields: {
        line_type: 'bank_account', line_action: 'insert', line_payload: payload,
      } })
    }
  })
  return { inserted, updated }
}

// 构造头表 fields（payload 化结构）。NOT NULL 列必须带齐（doc_status/line_no/doc_type_id/doc_date/entity_id），
// 否则标准保存 validate_changeset 报 422 或落库报 23502。doc_no 不带（cmx-code 铸号）。
// doc_type_id/entity_id 为占位值（MDM 暂无字典联动/业务主体隔离，待 M7 接入后替换）。
//
// 字段映射（cv_mdm_apply 骨架 + 公共搜索列 + payload）：
//   - name → subject_name（顶层公共搜索列）
//   - tax_no/credit_code/short_name → payload JSONB（业务字段下沉）
//   - create 步骤2：name 来自 state.keyName（步骤1 填的，步骤2 表单不再含 name）
//   - update：name 来自 headForm.getData().name（update 单页，表单含 name）
function buildHead() {
  const row = (headForm && headForm.getData && headForm.getData()) || {}
  const isEdit = state.mode === 'update'
  // name 取值：update 从表单；create 从步骤1 缓存的 keyName（步骤2 表单不含 name）
  const name = (isEdit ? (row.name || '') : (state.keyName || '')).trim()
  const tax = (row.tax_no || '').trim(); const cc = (row.credit_code || '').trim(); const sn = (row.short_name || '').trim()
  // 公共 NOT NULL 占位列
  const base = { line_no: 1, doc_status: 'draft', doc_type_id: 1, doc_date: todayStr(), entity_id: 1 }
  // payload：tax_no/credit_code/short_name 下沉 JSONB（name 已提升为 subject_name）
  const payload = { tax_no: tax, credit_code: cc, short_name: sn }
  if (isEdit) {
    const o = state.supplier || {}
    const deltas = {}
    const oldPayload = (o.payload && typeof o.payload === 'object') ? o.payload : {}
    // payload 字段 diff（裸名 key，对齐后端 field_deltas 解析）
    for (const f of PAYLOAD_FIELDS) {
      const oldV = (oldPayload[f] != null ? String(oldPayload[f]) : '').trim()
      const cur = f === 'tax_no' ? tax : (f === 'credit_code' ? cc : sn)
      if (cur !== oldV) deltas[f] = { old: oldV, new: cur }
    }
    // subject_name diff（兼容旧数据 o.name 回退）
    const oldName = ((o.subject_name != null ? String(o.subject_name) : '') || (o.name != null ? String(o.name) : '')).trim()
    if (name !== oldName) deltas['subject_name'] = { old: oldName, new: name }
    return { ...base, doc_type: 'mdm_supplier_change', cr_type: 'update', target_dict_code: 'supplier',
      target_record_id: Number(o.id), subject_name: name, payload, field_deltas: deltas }
  }
  return { ...base, doc_type: 'mdm_supplier_apply', cr_type: 'create', target_dict_code: 'supplier',
    subject_name: name, payload }
}

async function doSave(submit) {
  const C = cmx()
  const isEdit = state.mode === 'update'
  const headRow = (headForm && headForm.getData && headForm.getData()) || {}
  // name 校验：update 从表单；create 从步骤1 缓存的 keyName（步骤2 表单不含 name）
  const nameVal = (isEdit ? (headRow.name || '') : (state.keyName || '')).trim()
  if (!nameVal) { C.cmxWarn?.('供应商名称不能为空'); return }
  // 组件库未加载直接报错（不降级裸 fetch）——加载失败属异常配置，应显式暴露。
  if (typeof C.saveDocData !== 'function') { C.cmxError?.('组件库未加载，无法保存'); return }
  // 构造标准 merge changeset。头表：首次 inserted（临时 id），二次起 updated（真实 id）。
  const changes = {}
  if (state.savedCrId != null) {
    changes.cv_mdm_apply = { updated: [{ id: state.savedCrId, fields: buildHead() }] }
  } else {
    changes.cv_mdm_apply = { inserted: [{ id: HEAD_TID, fields: buildHead() }] }
  }
  // 明细行：collectLines 按是否已落库区分 inserted/updated
  const { inserted: lineIns, updated: lineUpd } = collectLines()
  const lineChanges = {}
  if (lineIns.length) lineChanges.inserted = lineIns
  if (lineUpd.length) lineChanges.updated = lineUpd
  if (lineIns.length || lineUpd.length) changes[TABLE_NAMES[1]] = lineChanges
  try {
    const data = await C.saveDocData(null,
      { ...DOC_DEF, dbId: state.dbId },
      { saveMode: 'merge', changes, tableNames: TABLE_NAMES })
    const idMap = (data && data.idMap) || {}
    // 头表：首次保存拿到真实 crId 后存入 state（二次起走 updated，避免重复新增）
    const isFirstSave = state.savedCrId == null
    if (isFirstSave && idMap[HEAD_TID] != null) {
      state.savedCrId = idMap[HEAD_TID]
    }
    // 明细行：把每行临时 id → 真实 id 回写到 DataSet 行的 _savedId（下次保存识别为 updated）
    if (lineIns.length) syncSavedLineIds(idMap)
    const crId = state.savedCrId
    if (submit && crId != null) {
      await apiPost('/api/mdm/change-requests/submit', { crId }, state.dbId)
    }
    C.cmxInfo?.(submit ? `变更申请 ${crId} 已提交审批` : (isFirstSave ? `已创建变更申请 ${crId}（草稿）` : `变更申请 ${crId} 已更新`))
  } catch (e) {
    // 422 列级校验失败：e.violations 经 formatViolations 多行中文展示
    if (e && e.violations && typeof C.formatViolations === 'function') {
      C.cmxError?.(`数据校验未通过：\n${C.formatViolations(e.violations, TABLE_NAMES)}`)
    } else {
      C.cmxError?.(`保存失败：${e.message}`)
    }
  }
}

// 把本次 inserted 明细行的临时 id → 真实 id（来自 idMap）回写到 bankGrid 的 DataSet 行 _savedId。
// 这样下次保存时 collectLines 能识别这些行为「已落库」走 updated，避免重复 insert。
function syncSavedLineIds(idMap) {
  if (!idMap || !bankGrid) return
  const ds = bankGrid.getDataSet?.()
  if (!ds || !ds.rows) return
  ds.rows.forEach((r) => {
    if (r._savedId == null && r.id != null && idMap[r.id] != null) {
      r._savedId = idMap[r.id]
    }
  })
}

function bind(root) {
  rootEl = root
  // 表单/表格按当前步骤构建（步骤1 只有 keyForm，步骤2/update 有 form + grid）
  const showSteps = state.mode !== 'update'
  if (showSteps && state.step === 1) {
    // 步骤1：仅关键信息表单 + 「下一步」
    try { buildKeyForm() } catch (e) { console.error('[cr-form] buildKeyForm fail', e) }
  } else {
    // 步骤2 / update：完整信息表单 + 银行账户表格
    try { bindBankGrid() } catch (e) { console.error('[cr-form] bindBankGrid fail', e) }
    try { buildForm() } catch (e) { console.error('[cr-form] buildForm fail', e) }
    root.querySelector('#fAddRow')?.addEventListener('click', () => {
      const C = cmx()
      const ds = bankGrid?.getDataSet?.()
      if (ds?.addRow) ds.addRow(newLine()); else bankGrid?.addRow?.(newLine())
      queueMicrotask(() => bankGrid?.refreshLayout?.())
    })
    root.querySelector('#fDelRow')?.addEventListener('click', () => {
      const ids = bankGrid?.getSelectedIds?.(); if (ids?.length) { bankGrid.removeRows(ids); queueMicrotask(() => bankGrid?.refreshLayout?.()) }
    })
  }
  // 步骤1 「下一步」（仅 create 步骤1）
  root.querySelector('#fNext')?.addEventListener('click', onNext)
  // 步骤2 「上一步」（仅 create 步骤2）：回步骤1，保留 keyName 让用户可改
  root.querySelector('#fPrev')?.addEventListener('click', () => goStep(1))
  // 保存/提交：update 顶部按钮（fSave/fSubmit）+ create 步骤2 底部按钮（fSave2/fSubmit2）共用 doSave
  root.querySelector('#fSave')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit')?.addEventListener('click', () => doSave(true))
  root.querySelector('#fSave2')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit2')?.addEventListener('click', () => doSave(true))
}

function refresh() {
  const host = currentHost; if (!host) return
  const root = host.renderRoot || host.shadowRoot; if (!root) return
  root.innerHTML = `<style>${styleCss()}</style>${viewHtml()}`
  bind(root)
}
let currentHost = null
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
      state.dbId = (ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || ''
      const ctxGet = (k) => { try { return host?.workspace?.context?.get?.(k) } catch { return undefined } }
      state.mode = ctxGet('mode') || 'create'
      state.supplier = ctxGet('supplier') || null
      state.bankLines = [{ account_no: '', bank_name: '' }]
      // 步骤初值：update 跳过查重（step=2 单页）；create 从步骤1 起步（先查重）
      state.step = state.mode === 'update' ? 2 : 1
      state.keyName = ''
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
