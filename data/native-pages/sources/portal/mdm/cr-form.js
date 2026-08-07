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

const BIZ_FIELDS = ['name', 'tax_no', 'credit_code', 'short_name']
const state = { dbId: '', mode: 'create', supplier: null, bankLines: [], savedCrId: null, savedLineIdMap: {} }
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
  `
}

function viewHtml() {
  const o = state.supplier || {}
  const isEdit = state.mode === 'update'
  return `<div class="pg">
    <ui5-bar design="Header" accessible-role="Toolbar">
      <ui5-label wrapping-type="Normal" style="font-weight:800;font-size:1.05rem;color:var(--sapShellTitleColor,var(--sapTitleColor));">${isEdit ? '变更供应商' : '新增供应商'}</ui5-label>
      <div slot="endContent" style="display:flex;gap:4px;">
        <ui5-button design="Default" icon="save" id="fSave">保存草稿</ui5-button>
        <ui5-button design="Emphasized" icon="paper-plane" id="fSubmit">保存并提交</ui5-button>
      </div>
    </ui5-bar>
    <div class="sec sec-head">
      <div class="sec-hd"><div class="sec-hd-l">
        <ui5-icon name="add-document" design="Default" mode="Decorative"></ui5-icon>
        <ui5-title level="H6" size="H6" wrapping-type="Normal" class="sec-t">基本信息</ui5-title>
      </div></div>
      <div class="sec-bd" id="fForm"></div>
    </div>
    <div class="sec sec-grid">
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
    </div>
  </div>`
}

// 银行行用组件库 cmx-revo-grid（可编辑）。增行用 CmxDataSet.addRow（触发 _refreshSource）+
// refreshLayout 双保险，保证新行即时可见；容器 .bank-fill 有最低高度。
let bankGrid = null
let headForm = null
let lineSeq = 0
const newLine = () => { lineSeq += 1; return { id: `nl_${Date.now()}_${lineSeq}`, account_no: '', bank_name: '' } }

// 顶部表单用 cmx-ui5-form（字段定义来自 CmxColumnModel，默认 Neo 皮肤）
function buildForm() {
  const C = cmx(); const wrap = q('fForm'); if (!wrap) return
  wrap.innerHTML = ''
  const form = document.createElement('cmx-ui5-form')
  form.classList.add('cmx-form-neo')
  if (C.CmxColumnModel && C.CmxColumn) {
    const cm = new C.CmxColumnModel({ datasetId: 'supplierHead' })
    cm.setMembers([
      new C.CmxColumn({ id: 'name', caption: '供应商名称', dataType: 'VARCHAR', required: true, edit: { mode: 'text' } }),
      new C.CmxColumn({ id: 'tax_no', caption: '税号', dataType: 'VARCHAR', edit: { mode: 'text' } }),
      new C.CmxColumn({ id: 'credit_code', caption: '统一社会信用代码', dataType: 'VARCHAR', edit: { mode: 'text' } }),
      new C.CmxColumn({ id: 'short_name', caption: '简称', dataType: 'VARCHAR', edit: { mode: 'text' } }),
    ])
    form.setColumnModel(cm)
  }
  form.setLayout?.('S1 M2 L3 XL3')
  form.setDataSet?.({ ...(state.supplier || {}) })
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

// 构造头表 fields。NOT NULL 列必须带齐（doc_status/line_no/doc_type_id/doc_date/entity_id），
// 否则标准保存 validate_changeset 报 422 或落库报 23502。doc_no 不带（cmx-code 铸号）。
// doc_type_id/entity_id 为占位值（MDM 暂无字典联动/业务主体隔离，待 M7 接入后替换）。
function buildHead() {
  const row = (headForm && headForm.getData && headForm.getData()) || {}
  const name = (row.name || '').trim(); const tax = (row.tax_no || '').trim(); const cc = (row.credit_code || '').trim(); const sn = (row.short_name || '').trim()
  // 公共 NOT NULL 占位列
  const base = { line_no: 1, doc_status: 'draft', doc_type_id: 1, doc_date: todayStr(), entity_id: 1 }
  if (state.mode === 'update') {
    const o = state.supplier || {}
    const deltas = {}
    const cur = { name, tax_no: tax, credit_code: cc, short_name: sn }
    for (const f of BIZ_FIELDS) if ((cur[f] || '') !== (o[f] || '')) deltas[f] = { old: o[f] ?? '', new: cur[f] ?? '' }
    return { ...base, doc_type: 'mdm_supplier_change', cr_type: 'update', target_dict_code: 'supplier',
      target_record_id: Number(o.id), name, tax_no: tax, credit_code: cc, short_name: sn, field_deltas: deltas }
  }
  return { ...base, doc_type: 'mdm_supplier_apply', cr_type: 'create', target_dict_code: 'supplier',
    name, tax_no: tax, credit_code: cc, short_name: sn }
}

async function doSave(submit) {
  const C = cmx()
  const headRow = (headForm && headForm.getData && headForm.getData()) || {}
  if (!(headRow.name || '').trim()) { C.cmxWarn?.('供应商名称不能为空'); return }
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
  root.querySelector('#fSave')?.addEventListener('click', () => doSave(false))
  root.querySelector('#fSubmit')?.addEventListener('click', () => doSave(true))
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
      if (host) whenRendered(host, '.pg', (r) => bind(r))
      return `<style>${styleCss()}</style>${viewHtml()}`
    },
  },
}
