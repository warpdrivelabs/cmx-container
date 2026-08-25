const state = {
  items: [],
  selected: null,
  doc: null,
  selectedFieldSet: '',
  selectedFieldIndex: -1,
  dirty: false,
  message: '',
  hosts: new Set(),
}

const DATA_TYPES = ['VARCHAR', 'BIGINT', 'INT', 'TINYINT', 'DECIMAL', 'DATE', 'DATETIME', 'TEXT']
const EDIT_MODES = ['', 'cmx-text-input', 'cmx-textarea-input', 'cmx-richtext-input', 'cmx-number-input', 'cmx-date-input', 'cmx-datetime-input', 'cmx-dict-select', 'checkbox']

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')

async function apiJson (url, options = {}) {
  const res = await fetch(url, {
    ...options,
    headers: { Accept: 'application/json', ...(options.headers || {}) },
    credentials: 'same-origin',
  })
  if (!res.ok) {
    let msg = `HTTP ${res.status}`
    try {
      const j = await res.json()
      if (j && j.error) msg = j.error
    } catch {}
    throw new Error(msg)
  }
  return res.json()
}

async function loadList () {
  const data = await apiJson('/api/definitions/list?kind=BASE&domain=base')
  state.items = Array.isArray(data.items) ? data.items : []
  if (!state.selected && state.items[0]) state.selected = state.items.find((x) => String(x.file).includes('dct')) || state.items[0]
}

async function loadDoc (force = false) {
  if (!state.selected) return null
  if (state.doc && !force) return state.doc
  const q = new URLSearchParams({
    domain: state.selected.domain || 'base',
    module: state.selected.module || '',
    file: state.selected.file || '',
  })
  state.doc = await apiJson(`/api/definitions/config?${q}`)
  ensureShape()
  const keys = fieldSetKeys()
  if (!state.selectedFieldSet || !state.doc.fieldSets[state.selectedFieldSet]) state.selectedFieldSet = keys[0] || ''
  state.selectedFieldIndex = currentFields()[state.selectedFieldIndex] ? state.selectedFieldIndex : (currentFields()[0] ? 0 : -1)
  state.dirty = false
  return state.doc
}

async function ensureReady () {
  await loadList()
  await loadDoc()
}

function ensureShape () {
  if (!state.doc || typeof state.doc !== 'object') state.doc = {}
  if (!state.doc.moduleMeta || typeof state.doc.moduleMeta !== 'object') state.doc.moduleMeta = {}
  if (!state.doc.fieldSets || typeof state.doc.fieldSets !== 'object') state.doc.fieldSets = {}
}

function fieldSetKeys () {
  ensureShape()
  return Object.keys(state.doc.fieldSets)
}

function currentFieldSet () {
  ensureShape()
  return state.doc.fieldSets[state.selectedFieldSet] || null
}

function currentFields () {
  const fs = currentFieldSet()
  if (!fs) return []
  if (!Array.isArray(fs.fields)) fs.fields = []
  return fs.fields
}

function currentField () {
  return currentFields()[state.selectedFieldIndex] || null
}

function markDirty (message = '') {
  state.dirty = true
  state.message = message
}

function refreshAll () {
  for (const host of Array.from(state.hosts)) {
    if (host && host.isConnected && typeof host._load === 'function') host._load()
    else state.hosts.delete(host)
  }
}

function selectFile (file) {
  const item = state.items.find((x) => x.file === file)
  if (!item) return
  state.selected = item
  state.doc = null
  state.selectedFieldSet = ''
  state.selectedFieldIndex = -1
  refreshAll()
}

function selectFieldSet (name) {
  state.selectedFieldSet = name
  state.selectedFieldIndex = currentFields()[0] ? 0 : -1
  refreshAll()
}

function selectField (index) {
  state.selectedFieldIndex = index
  refreshAll()
}

function setPath (obj, path, value, type = '') {
  const segs = path.split('.')
  const leaf = segs.pop()
  let node = obj
  for (const s of segs) {
    if (!node[s] || typeof node[s] !== 'object') node[s] = {}
    node = node[s]
  }
  if (type === 'boolean') node[leaf] = !!value
  else if (type === 'number') {
    if (value === '') delete node[leaf]
    else {
      const n = Number(value)
      node[leaf] = Number.isFinite(n) ? n : value
    }
  } else if (value === '') delete node[leaf]
  else node[leaf] = value
}

async function saveDoc () {
  if (!state.selected || !state.doc) return
  const q = new URLSearchParams({
    domain: state.selected.domain || 'base',
    module: state.selected.module || '',
    file: state.selected.file || '',
  })
  const res = await apiJson(`/api/definitions/config?${q}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(state.doc),
  })
  state.doc = res.saved || state.doc
  ensureShape()
  state.dirty = false
  state.message = '已保存'
  refreshAll()
}

function addField () {
  const fields = currentFields()
  let id = 'new_field'
  let i = 2
  while (fields.some((f) => f.id === id || f.name === id)) id = `new_field_${i++}`
  fields.push({
    id,
    name: id,
    dataType: 'VARCHAR',
    fieldLength: 64,
    nullable: true,
    caption: { zh_CN: '新字段' },
    edit: { mode: 'cmx-text-input' },
  })
  state.selectedFieldIndex = fields.length - 1
  markDirty('已新增字段')
  refreshAll()
}

function deleteField () {
  const fields = currentFields()
  if (!fields[state.selectedFieldIndex]) return
  fields.splice(state.selectedFieldIndex, 1)
  state.selectedFieldIndex = fields[state.selectedFieldIndex] ? state.selectedFieldIndex : fields.length - 1
  markDirty('字段已删除')
  refreshAll()
}

function updateFieldSetRemark (value) {
  const fs = currentFieldSet()
  if (!fs) return
  fs.remark = value
  markDirty()
}

function updateField (path, value, type = '') {
  const field = currentField()
  if (!field) return
  setPath(field, path, value, type)
  if (path === 'id' && !field.name) field.name = value
  if (path === 'name' && !field.id) field.id = value
  markDirty()
}

function updateFieldAt (index, path, value, type = '') {
  const field = currentFields()[index]
  if (!field) return
  setPath(field, path, value, type)
  if (path === 'id' && !field.name) field.name = value
  if (path === 'name' && !field.id) field.id = value
  markDirty()
}

function optionHtml (items, value) {
  return items.map((it) => `<option value="${esc(it)}" ${String(value || '') === it ? 'selected' : ''}>${esc(it || '无')}</option>`).join('')
}

function mount (ctx, html, after) {
  queueMicrotask(() => {
    if (ctx.host) state.hosts.add(ctx.host)
    // native host 用 light DOM：渲染根挂在 host.renderRoot（兼容旧 shadowRoot 写法）
    const host = ctx.host
    const root = host && (host.renderRoot
      || (host.shadowRoot && host.shadowRoot.querySelector('.native-page-root'))
      || (host.querySelector && host.querySelector('.native-page-root')))
    if (root && typeof after === 'function') after(root)
  })
  return `<style>
    .np-wrap{display:flex;flex-direction:column;flex:1 1 auto;min-height:0;width:100%;height:100%;font:13px/1.45 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#fff)}
    .np-head{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:9px 12px;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);flex:0 0 auto}
    .np-title{font-size:14px;font-weight:700}.np-sub{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}.np-actions{display:flex;gap:6px;align-items:center}
    .np-body{flex:1 1 auto;min-height:0;overflow:auto;padding:10px}.np-grid{display:grid;grid-template-columns:320px minmax(0,1fr);gap:10px;height:100%;min-height:0}
    .np-panel{border:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);border-radius:6px;min-height:0;overflow:hidden;display:flex;flex-direction:column;background:var(--sapList_Background,#fff)}
    .np-panel-head{padding:8px 10px;font-weight:700;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);display:flex;justify-content:space-between;gap:8px;align-items:center}
    .np-scroll{overflow:auto;min-height:0;flex:1 1 auto}.np-list{display:flex;flex-direction:column;gap:6px;padding:8px}
    .np-item{border:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);background:var(--sapList_Background,#fff);text-align:left;padding:8px;border-radius:6px;cursor:pointer;color:inherit}
    .np-item.active{border-color:var(--sapHighlightColor,#0070f2);box-shadow:inset 3px 0 0 var(--sapHighlightColor,#0070f2)}
    .np-form{display:grid;grid-template-columns:110px minmax(0,1fr);gap:8px 10px;align-items:center}.np-form label{color:var(--sapContent_LabelColor,#6a6d70)}
    .np-input,.np-form input,.np-form select,.np-form textarea{width:100%;box-sizing:border-box;border:1px solid var(--sapField_BorderColor,#89919a);border-radius:4px;padding:5px 7px;background:var(--sapField_Background,#fff);color:inherit;font:inherit}
    .np-form textarea{min-height:64px;resize:vertical}.np-table{width:100%;border-collapse:collapse;font-size:12px}.np-table th,.np-table td{border-bottom:1px solid var(--sapList_BorderColor,#eee);padding:5px;text-align:left;vertical-align:top}.np-table tr.active{background:var(--sapList_SelectionBackgroundColor,#e5f2ff)}
    .np-btn{border:1px solid var(--sapButton_BorderColor,#8a8f94);border-radius:4px;background:var(--sapButton_Background,#fff);color:var(--sapButton_TextColor,#0064d9);padding:5px 9px;cursor:pointer;font:inherit}.np-btn.primary{background:var(--sapButton_Emphasized_Background,#0070f2);border-color:var(--sapButton_Emphasized_BorderColor,#0070f2);color:#fff}.np-btn.danger{color:var(--sapNegativeTextColor,#b00)}
    .np-code{margin:0;padding:10px;min-height:100%;box-sizing:border-box;white-space:pre-wrap;font:12px/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--sapShell_Background,#f7f7f7)}
    .np-code-edit{width:100%;height:100%;min-height:320px;box-sizing:border-box;border:0;padding:10px;font:12px/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--sapShell_Background,#f7f7f7);color:inherit;resize:none}.np-empty{padding:12px;color:var(--sapContent_LabelColor,#6a6d70)}
  </style>${html}`
}

function bindInputs (root) {
  root.querySelectorAll('[data-fs-remark]').forEach((el) => el.addEventListener('input', () => updateFieldSetRemark(el.value)))
  root.querySelectorAll('[data-field-prop]').forEach((el) => {
    el.addEventListener(el.type === 'checkbox' ? 'change' : 'input', () => {
      updateField(el.getAttribute('data-field-prop'), el.type === 'checkbox' ? el.checked : el.value, el.getAttribute('data-value-type') || '')
    })
  })
  root.querySelectorAll('[data-field-index][data-field-inline]').forEach((el) => {
    el.addEventListener('focus', () => { state.selectedFieldIndex = Number(el.getAttribute('data-field-index')) })
    el.addEventListener(el.type === 'checkbox' ? 'change' : 'input', () => {
      state.selectedFieldIndex = Number(el.getAttribute('data-field-index'))
      updateFieldAt(Number(el.getAttribute('data-field-index')), el.getAttribute('data-field-inline'), el.type === 'checkbox' ? el.checked : el.value, el.getAttribute('data-value-type') || '')
    })
  })
}

export default {
  defaultView: 'manager',
  views: {
    async manager (ctx) {
      await ensureReady()
      const fields = currentFields()
      return mount(ctx, `<div class="np-wrap">
        <div class="np-head">
          <div><div class="np-title">字典基础元数据</div><div class="np-sub">${esc(state.selected?.file || '')}${state.dirty ? ' · 未保存' : ''}${state.message ? ` · ${esc(state.message)}` : ''}</div></div>
          <div class="np-actions"><button class="np-btn primary" data-action="save">保存</button><button class="np-btn" data-action="reload">刷新</button></div>
        </div>
        <div class="np-body">
          <div class="np-panel">
            <div class="np-panel-head"><span>字段集：${esc(state.selectedFieldSet || '无')}</span><span class="np-actions"><button class="np-btn" data-action="add-field">新增字段</button><button class="np-btn danger" data-action="delete-field">删除字段</button></span></div>
            <div class="np-scroll">
              <table class="np-table"><thead><tr><th>ID</th><th>名称</th><th>标题</th><th>类型</th><th>长度</th><th>必填</th><th>控件</th></tr></thead><tbody>
                ${fields.map((f, i) => `<tr class="${i === state.selectedFieldIndex ? 'active' : ''}" data-field-index="${i}">
                  <td><input data-field-index="${i}" data-field-inline="id" value="${esc(f.id || '')}"></td>
                  <td><input data-field-index="${i}" data-field-inline="name" value="${esc(f.name || '')}"></td>
                  <td><input data-field-index="${i}" data-field-inline="caption.zh_CN" value="${esc(f.caption?.zh_CN || '')}"></td>
                  <td><select data-field-index="${i}" data-field-inline="dataType">${optionHtml(DATA_TYPES, f.dataType)}</select></td>
                  <td><input data-field-index="${i}" data-field-inline="fieldLength" data-value-type="number" type="number" value="${esc(f.fieldLength ?? '')}"></td>
                  <td><input data-field-index="${i}" data-field-inline="nullable" data-value-type="boolean" type="checkbox" ${f.nullable === false ? '' : 'checked'}></td>
                  <td><select data-field-index="${i}" data-field-inline="edit.mode">${optionHtml(EDIT_MODES, f.edit?.mode)}</select></td>
                </tr>`).join('') || '<tr><td colspan="7" class="np-empty"><cmx-empty-state icon="table-row" title="当前字段集暂无字段" size="sm"></cmx-empty-state></td></tr>'}
              </tbody></table>
            </div>
          </div>
        </div>
      </div>`, (root) => {
        bindInputs(root)
        root.querySelector('[data-action="save"]')?.addEventListener('click', saveDoc)
        root.querySelector('[data-action="reload"]')?.addEventListener('click', async () => { state.doc = null; await loadDoc(true); refreshAll() })
        root.querySelector('[data-action="add-field"]')?.addEventListener('click', addField)
        root.querySelector('[data-action="delete-field"]')?.addEventListener('click', deleteField)
        root.querySelectorAll('tr[data-field-index]').forEach((tr) => tr.addEventListener('click', (e) => {
          if (e.target instanceof Element && e.target.closest('input,select,textarea')) return
          selectField(Number(tr.getAttribute('data-field-index')))
        }))
      })
    },
    async list (ctx) {
      await ensureReady()
      const files = state.items.map((it) => `<button class="np-item ${state.selected?.file === it.file ? 'active' : ''}" data-file="${esc(it.file)}"><strong>${esc(it.title || it.file)}</strong><div class="np-sub">${esc(it.file)} · ${esc(it.fieldSetCount || 0)} 字段集</div></button>`).join('')
      const sets = fieldSetKeys().map((name) => `<button class="np-item ${state.selectedFieldSet === name ? 'active' : ''}" data-fs="${esc(name)}"><strong>${esc(name)}</strong><div class="np-sub">${esc(state.doc.fieldSets[name]?.remark || '')}</div></button>`).join('')
      return mount(ctx, `<div class="np-wrap"><div class="np-list">
        ${files}
        <div class="np-panel-head" style="border:0;padding:8px 0 0"><span>字段集</span></div>
        ${sets || '<cmx-empty-state icon="folder" title="暂无字段集" size="sm"></cmx-empty-state>'}
      </div></div>`, (root) => {
        root.querySelectorAll('[data-file]').forEach((btn) => btn.addEventListener('click', () => selectFile(btn.getAttribute('data-file'))))
        root.querySelectorAll('[data-fs]').forEach((btn) => btn.addEventListener('click', () => selectFieldSet(btn.getAttribute('data-fs'))))
      })
    },
    async inspector (ctx) {
      await ensureReady()
      const fs = currentFieldSet()
      const field = currentField()
      return mount(ctx, `<div class="np-wrap"><div class="np-body">
        <div class="np-form">
          <label>字段集名</label><input value="${esc(state.selectedFieldSet || '')}" readonly>
          <label>字段集说明</label><textarea data-fs-remark>${esc(fs?.remark || '')}</textarea>
          ${field ? `
            <label>字段ID</label><input data-field-prop="id" value="${esc(field.id || '')}">
            <label>字段名称</label><input data-field-prop="name" value="${esc(field.name || '')}">
            <label>标题</label><input data-field-prop="caption.zh_CN" value="${esc(field.caption?.zh_CN || '')}">
            <label>数据类型</label><select data-field-prop="dataType">${optionHtml(DATA_TYPES, field.dataType)}</select>
            <label>长度</label><input data-field-prop="fieldLength" data-value-type="number" type="number" value="${esc(field.fieldLength ?? '')}">
            <label>整数位</label><input data-field-prop="intDigits" data-value-type="number" type="number" value="${esc(field.intDigits ?? '')}">
            <label>小数位</label><input data-field-prop="decimalDigits" data-value-type="number" type="number" value="${esc(field.decimalDigits ?? '')}">
            <label>允许为空</label><input data-field-prop="nullable" data-value-type="boolean" type="checkbox" ${field.nullable !== false ? 'checked' : ''}>
            <label>参照字典</label><input data-field-prop="refDict" value="${esc(field.refDict || '')}">
            <label>参照字段</label><input data-field-prop="refField" value="${esc(field.refField || '')}">
            <label>显示字段</label><input data-field-prop="displayField" value="${esc(field.displayField || '')}">
            <label>录入控件</label><select data-field-prop="edit.mode">${optionHtml(EDIT_MODES, field.edit?.mode)}</select>
          ` : '<label>当前字段</label><div>无</div>'}
          <label>保存状态</label><div>${state.dirty ? '未保存' : '已保存'}</div>
        </div>
      </div></div>`, (root) => {
        bindInputs(root)
      })
    },
    async source (ctx) {
      await ensureReady()
      return mount(ctx, `<div class="np-wrap">
        <div class="np-head"><div><div class="np-title">源码</div><div class="np-sub">${state.dirty ? '未保存' : '已保存'}</div></div></div>
        <textarea class="np-code-edit" data-source>${esc(JSON.stringify(state.doc || {}, null, 2))}</textarea>
      </div>`, (root) => {
        root.querySelector('[data-source]')?.addEventListener('input', () => {
          try {
            state.doc = JSON.parse(root.querySelector('[data-source]').value)
            ensureShape()
            markDirty('源码已修改')
          } catch (err) {
            state.message = err.message || String(err)
          }
        })
      })
    },
  },
}
