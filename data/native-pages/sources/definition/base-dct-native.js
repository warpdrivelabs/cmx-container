const state = {
  items: [],
  selected: null,
  doc: null,
  selectedFieldSet: '',
  selectedFieldIndex: -1,
  dirty: false,
  loading: false,
  message: '',
  sourceText: '',
  sourceError: '',
  hosts: new Set(),
}

const DATA_TYPES = ['VARCHAR', 'BIGINT', 'INT', 'TINYINT', 'DECIMAL', 'DATE', 'DATETIME', 'TEXT']
const EDIT_MODES = ['', 'cmx-text-input', 'cmx-textarea-input', 'cmx-richtext-input', 'cmx-number-input', 'cmx-date-input', 'cmx-datetime-input', 'cmx-dict-select', 'checkbox']
const DIM_TYPES = ['', 'dimension', 'measure', 'attribute']

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')

const clone = (v) => JSON.parse(JSON.stringify(v ?? null))

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
      if (j?.error) msg = j.error
    } catch {}
    throw new Error(msg)
  }
  return res.json()
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

function ownFields () {
  const fs = currentFieldSet()
  if (!fs) return []
  if (!Array.isArray(fs.fields)) fs.fields = []
  return fs.fields
}

function includeNames (fs = currentFieldSet()) {
  return Array.isArray(fs?.includeFieldSets) ? fs.includeFieldSets.filter(Boolean) : []
}

function includedFields () {
  const names = includeNames()
  const out = []
  for (const name of names) {
    const fs = state.doc?.fieldSets?.[name]
    for (const field of Array.isArray(fs?.fields) ? fs.fields : []) out.push({ ...clone(field), __fromFieldSet: name })
  }
  return out
}

function totalFields (fsName) {
  const fs = state.doc?.fieldSets?.[fsName]
  if (!fs) return 0
  const own = Array.isArray(fs.fields) ? fs.fields.length : 0
  const inc = includeNames(fs).reduce((n, name) => n + (Array.isArray(state.doc?.fieldSets?.[name]?.fields) ? state.doc.fieldSets[name].fields.length : 0), 0)
  return own + inc
}

function currentField () {
  return ownFields()[state.selectedFieldIndex] || null
}

function markDirty (message = '') {
  state.dirty = true
  state.message = message
  state.sourceText = ''
  state.sourceError = ''
}

function refreshAll () {
  for (const host of Array.from(state.hosts)) {
    if (host?.isConnected && typeof host._load === 'function') host._load()
    else state.hosts.delete(host)
  }
}

async function loadList () {
  const data = await apiJson('/api/definitions/list?kind=BASE&domain=base')
  state.items = (Array.isArray(data.items) ? data.items : []).filter((it) => /_dct_/i.test(String(it.file || '')))
  if (!state.selected && state.items[0]) state.selected = state.items[0]
}

async function loadDoc (force = false) {
  if (!state.selected) return null
  if (state.doc && !force) return state.doc
  const q = new URLSearchParams({
    domain: state.selected.domain || 'base',
    module: state.selected.module || '',
    file: state.selected.file || '',
  })
  state.loading = true
  try {
    state.doc = await apiJson(`/api/definitions/config?${q}`)
    ensureShape()
    const keys = fieldSetKeys()
    if (!state.selectedFieldSet || !state.doc.fieldSets[state.selectedFieldSet]) state.selectedFieldSet = keys[0] || ''
    state.selectedFieldIndex = ownFields()[state.selectedFieldIndex] ? state.selectedFieldIndex : (ownFields()[0] ? 0 : -1)
    state.dirty = false
    state.message = ''
    state.sourceText = ''
    state.sourceError = ''
  } finally {
    state.loading = false
  }
  return state.doc
}

async function ensureReady () {
  if (!state.items.length) await loadList()
  await loadDoc()
}

async function selectFile (file) {
  const item = state.items.find((x) => x.file === file)
  if (!item) return
  if (state.dirty && !window.confirm('当前修改尚未保存，确定切换文件？')) return
  state.selected = item
  state.doc = null
  state.selectedFieldSet = ''
  state.selectedFieldIndex = -1
  await loadDoc(true)
  refreshAll()
}

function selectFieldSet (name) {
  state.selectedFieldSet = name
  state.selectedFieldIndex = ownFields()[0] ? 0 : -1
  state.message = ''
  refreshAll()
}

function selectField (index) {
  state.selectedFieldIndex = Number(index)
  refreshAll()
}

function setPath (obj, path, value, type = '') {
  const segs = String(path || '').split('.').filter(Boolean)
  const leaf = segs.pop()
  if (!leaf) return
  let node = obj
  for (const seg of segs) {
    if (!node[seg] || typeof node[seg] !== 'object') node[seg] = {}
    node = node[seg]
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

function updateModuleMeta (field, value) {
  ensureShape()
  if (field === 'version') {
    const n = Number(value)
    state.doc.moduleMeta.version = Number.isFinite(n) ? n : value
  } else if (value === '') delete state.doc.moduleMeta[field]
  else state.doc.moduleMeta[field] = value
  markDirty()
}

function updateFieldSetProp (field, value) {
  const fs = currentFieldSet()
  if (!fs) return
  if (field === 'includeFieldSets') {
    const names = String(value || '').split(',').map((x) => x.trim()).filter(Boolean)
      .filter((name, idx, arr) => name !== state.selectedFieldSet && arr.indexOf(name) === idx)
    if (names.length) fs.includeFieldSets = names
    else delete fs.includeFieldSets
  } else if (value === '') delete fs[field]
  else fs[field] = value
  markDirty()
}

function updateField (index, path, value, type = '') {
  const field = ownFields()[Number(index)]
  if (!field) return
  setPath(field, path, value, type)
  if (path === 'id' && !field.name) field.name = value
  if (path === 'name' && !field.id) field.id = value
  markDirty()
}

function addField () {
  const fields = ownFields()
  let id = 'new_field'
  let i = 2
  while (fields.some((f) => f.id === id || f.name === id)) id = `new_field_${i++}`
  const field = { id, name: id, caption: { zh_CN: '新字段' }, dataType: 'VARCHAR', fieldLength: 64, nullable: true, edit: { mode: 'cmx-text-input' } }
  const at = state.selectedFieldIndex >= 0 && state.selectedFieldIndex < fields.length ? state.selectedFieldIndex + 1 : fields.length
  fields.splice(at, 0, field)
  state.selectedFieldIndex = at
  markDirty('已新增字段')
  refreshAll()
}

function deleteField (index = state.selectedFieldIndex) {
  const fields = ownFields()
  const idx = Number(index)
  const field = fields[idx]
  if (!field) return
  if (!window.confirm(`确定删除字段 ${field.id || field.name || `#${idx + 1}`}？`)) return
  fields.splice(idx, 1)
  state.selectedFieldIndex = fields[idx] ? idx : fields.length - 1
  markDirty('字段已删除')
  refreshAll()
}

function moveField (index, dir) {
  const fields = ownFields()
  const idx = Number(index)
  const next = idx + dir
  if (!fields[idx] || next < 0 || next >= fields.length) return
  ;[fields[idx], fields[next]] = [fields[next], fields[idx]]
  state.selectedFieldIndex = next
  markDirty('字段顺序已调整')
  refreshAll()
}

function uniqueFieldSetName (base = 'newFieldSet') {
  let name = base
  let i = 2
  while (state.doc?.fieldSets?.[name]) name = `${base}${i++}`
  return name
}

function addFieldSet () {
  ensureShape()
  const name = uniqueFieldSetName()
  state.doc.fieldSets[name] = { remark: '新字段集', fields: [] }
  state.selectedFieldSet = name
  state.selectedFieldIndex = -1
  markDirty('已新增字段集')
  refreshAll()
}

function renameFieldSet (newName) {
  ensureShape()
  const oldName = state.selectedFieldSet
  const name = String(newName || '').trim()
  if (!oldName || !name || name === oldName) return
  if (!/^[A-Za-z_][A-Za-z0-9_-]*$/.test(name)) {
    state.message = '字段集名称需以字母/下划线开头，仅允许字母、数字、_-'
    refreshAll()
    return
  }
  if (state.doc.fieldSets[name]) {
    state.message = `字段集已存在：${name}`
    refreshAll()
    return
  }
  state.doc.fieldSets[name] = state.doc.fieldSets[oldName]
  delete state.doc.fieldSets[oldName]
  for (const fs of Object.values(state.doc.fieldSets)) {
    if (Array.isArray(fs?.includeFieldSets)) fs.includeFieldSets = fs.includeFieldSets.map((x) => x === oldName ? name : x)
  }
  state.selectedFieldSet = name
  markDirty('字段集已重命名')
  refreshAll()
}

function deleteFieldSet () {
  ensureShape()
  const name = state.selectedFieldSet
  const fs = state.doc.fieldSets[name]
  if (!name || !fs) return
  const refs = Object.entries(state.doc.fieldSets)
    .filter(([key, value]) => key !== name && Array.isArray(value?.includeFieldSets) && value.includeFieldSets.includes(name))
    .map(([key]) => key)
  if (refs.length) {
    state.message = `字段集被引用，不能删除：${refs.join(', ')}`
    refreshAll()
    return
  }
  const count = (Array.isArray(fs.fields) ? fs.fields.length : 0) + includeNames(fs).length
  if (count && !window.confirm(`字段集 ${name} 非空，确定删除？`)) return
  delete state.doc.fieldSets[name]
  state.selectedFieldSet = fieldSetKeys()[0] || ''
  state.selectedFieldIndex = ownFields()[0] ? 0 : -1
  markDirty('字段集已删除')
  refreshAll()
}

async function saveDoc () {
  if (!state.selected || !state.doc) return
  const q = new URLSearchParams({
    domain: state.selected.domain || 'base',
    module: state.selected.module || '',
    file: state.selected.file || '',
  })
  state.loading = true
  refreshAll()
  try {
    const res = await apiJson(`/api/definitions/config?${q}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(state.doc),
    })
    state.doc = res.saved || state.doc
    ensureShape()
    state.dirty = false
    state.message = '已保存'
    state.sourceText = ''
    state.sourceError = ''
  } catch (err) {
    state.message = err.message || String(err)
  } finally {
    state.loading = false
    refreshAll()
  }
}

async function reloadDoc () {
  if (state.dirty && !window.confirm('当前修改尚未保存，确定刷新？')) return
  await loadDoc(true)
  refreshAll()
}

function sourceText () {
  return state.sourceText || JSON.stringify(state.doc || {}, null, 2)
}

function updateSourceText (value) {
  state.sourceText = value
  try {
    const doc = JSON.parse(value || '{}')
    state.doc = doc
    ensureShape()
    const keys = fieldSetKeys()
    if (!state.doc.fieldSets[state.selectedFieldSet]) state.selectedFieldSet = keys[0] || ''
    state.selectedFieldIndex = ownFields()[state.selectedFieldIndex] ? state.selectedFieldIndex : (ownFields()[0] ? 0 : -1)
    state.sourceError = ''
    markDirty('源码已修改')
  } catch (err) {
    state.sourceError = err.message || String(err)
    state.message = `JSON 错误：${state.sourceError}`
  }
}

function optionHtml (items, value) {
  return items.map((it) => `<option value="${esc(it)}" ${String(value || '') === it ? 'selected' : ''}>${esc(it || '无')}</option>`).join('')
}

function mount (ctx, html, after) {
  queueMicrotask(() => {
    if (ctx.host) state.hosts.add(ctx.host)
    const host = ctx.host
    const root = host && (host.renderRoot || host.shadowRoot?.querySelector('.native-page-root'))
    if (root && typeof after === 'function') after(root)
  })
  return `${styleHtml()}${html}`
}

function styleHtml () {
  return `<style>
    .bdn-wrap{display:flex;flex-direction:column;flex:1 1 auto;width:100%;height:100%;min-width:0;min-height:0;font:13px/1.45 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#fff)}
    .bdn-head{height:42px;display:flex;align-items:center;gap:8px;padding:0 10px;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);box-sizing:border-box;flex:0 0 auto}
    .bdn-title{font-weight:700;font-size:14px}.bdn-sub{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.bdn-spacer{flex:1 1 auto}.bdn-actions{display:flex;align-items:center;gap:6px}
    .bdn-btn{height:28px;border:1px solid var(--sapButton_BorderColor,#8a8f94);border-radius:4px;background:var(--sapButton_Background,#fff);color:var(--sapButton_TextColor,#0064d9);padding:0 9px;cursor:pointer;font:inherit;display:inline-flex;align-items:center;justify-content:center;gap:4px;white-space:nowrap}
    .bdn-btn.primary{background:var(--sapButton_Emphasized_Background,#0070f2);border-color:var(--sapButton_Emphasized_BorderColor,#0070f2);color:#fff}.bdn-btn.danger{color:var(--sapNegativeTextColor,#b00)}.bdn-btn.icon{width:28px;padding:0}.bdn-btn:disabled{opacity:.45;cursor:default}
    .bdn-body{flex:1 1 auto;min-height:0;overflow:auto;background:var(--sapGroup_ContentBackground,#fafafa);padding:10px;box-sizing:border-box}.bdn-stack{display:flex;flex-direction:column;gap:10px;height:100%;min-height:0}
    .bdn-card{border:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);border-radius:6px;background:var(--sapTile_Background,#fff);display:flex;flex-direction:column;min-height:0;overflow:hidden}.bdn-card.flex{flex:1 1 auto}.bdn-card.third{flex:0 0 34%;min-height:140px}
    .bdn-card-head{height:36px;display:flex;align-items:center;gap:8px;padding:0 10px;border-bottom:1px solid var(--sapGroup_TitleBorderColor,#d9d9d9);box-sizing:border-box;flex:0 0 auto}.bdn-card-title{font-weight:700}
    .bdn-scroll{flex:1 1 auto;min-height:0;overflow:auto}.bdn-list{display:flex;flex-direction:column;gap:6px;padding:8px}.bdn-item{border:1px solid transparent;border-radius:6px;background:transparent;color:inherit;padding:8px;text-align:left;cursor:pointer;font:inherit}.bdn-item:hover{background:var(--sapList_Hover_Background,#f5f6f7)}.bdn-item.active{border-color:var(--sapHighlightColor,#0070f2);background:color-mix(in srgb,var(--sapHighlightColor,#0070f2) 10%,transparent);box-shadow:inset 3px 0 0 var(--sapHighlightColor,#0070f2)}
    .bdn-item strong{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.bdn-muted{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70)}
    .bdn-form{display:grid;grid-template-columns:92px minmax(0,1fr);gap:8px 10px;align-items:center;padding:10px}.bdn-form label{color:var(--sapContent_LabelColor,#6a6d70)}
    .bdn-input,.bdn-form input,.bdn-form textarea,.bdn-form select,.bdn-table input,.bdn-table select{width:100%;height:28px;box-sizing:border-box;border:1px solid var(--sapField_BorderColor,#89919a);border-radius:4px;padding:0 7px;background:var(--sapField_Background,#fff);color:var(--sapField_TextColor,var(--sapTextColor,#1d2d3e));font:inherit}
    .bdn-form textarea{height:auto;min-height:58px;padding:6px 7px;resize:vertical}.bdn-table{width:100%;border-collapse:collapse;font-size:12px}.bdn-table th,.bdn-table td{border-bottom:1px solid var(--sapList_BorderColor,#eee);padding:5px;text-align:left;vertical-align:middle}.bdn-table th{position:sticky;top:0;background:var(--sapList_HeaderBackground,#f7f7f7);z-index:1;color:var(--sapContent_LabelColor,#6a6d70);font-weight:600}.bdn-table tr.active{background:var(--sapList_SelectionBackgroundColor,#e5f2ff)}
    .bdn-table input[type=checkbox]{appearance:none;width:16px;height:16px;min-width:16px;padding:0;border-radius:3px;position:relative;vertical-align:middle}.bdn-table input[type=checkbox]:checked{background:#0a6ed1;border-color:#0a6ed1}.bdn-table input[type=checkbox]:checked:after{content:"";position:absolute;left:4px;top:1px;width:4px;height:8px;border:solid #fff;border-width:0 2px 2px 0;transform:rotate(45deg)}
    .bdn-section{border:1px solid var(--sapGroup_TitleBorderColor,#e5e5e5);border-radius:6px;margin:8px;overflow:hidden}.bdn-section-head{height:34px;display:flex;align-items:center;gap:8px;padding:0 10px;background:var(--sapList_HeaderBackground,#f7f7f7);border-bottom:1px solid var(--sapGroup_TitleBorderColor,#e5e5e5);font-weight:700}.bdn-chip{font-size:11px;border:1px solid color-mix(in srgb,var(--sapHighlightColor,#0070f2) 45%,transparent);background:color-mix(in srgb,var(--sapHighlightColor,#0070f2) 10%,transparent);border-radius:10px;padding:1px 7px}
    .bdn-code{flex:1 1 auto;min-height:0;width:100%;border:0;resize:none;outline:none;padding:10px;box-sizing:border-box;font:12px/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--sapField_Background,#fff);color:inherit}.bdn-empty{padding:12px;color:var(--sapContent_LabelColor,#6a6d70)}.bdn-status{min-height:20px;padding:4px 10px;color:var(--sapContent_LabelColor,#6a6d70);font-size:12px}
  </style>`
}

function headerHtml (title) {
  const file = state.selected?.file || ''
  return `<div class="bdn-head">
    <div style="min-width:0"><div class="bdn-title">${esc(title)}</div><div class="bdn-sub">${esc(file)}${state.dirty ? ' · 未保存' : ''}${state.loading ? ' · 加载中' : ''}${state.message ? ` · ${esc(state.message)}` : ''}</div></div>
    <span class="bdn-spacer"></span>
    <button class="bdn-btn primary" data-action="save">保存</button>
    <button class="bdn-btn" data-action="reload">刷新</button>
  </div>`
}

function bindCommon (root) {
  root.querySelector('[data-action="save"]')?.addEventListener('click', saveDoc)
  root.querySelector('[data-action="reload"]')?.addEventListener('click', reloadDoc)
  root.querySelectorAll('[data-module-meta]').forEach((el) => el.addEventListener('input', () => updateModuleMeta(el.getAttribute('data-module-meta'), el.value)))
  root.querySelectorAll('[data-fs-prop]').forEach((el) => el.addEventListener('input', () => updateFieldSetProp(el.getAttribute('data-fs-prop'), el.value)))
  root.querySelectorAll('[data-field-index][data-field-prop]').forEach((el) => {
    const event = el.type === 'checkbox' ? 'change' : 'input'
    el.addEventListener('focus', () => { state.selectedFieldIndex = Number(el.getAttribute('data-field-index')) })
    el.addEventListener(event, () => {
      updateField(el.getAttribute('data-field-index'), el.getAttribute('data-field-prop'), el.type === 'checkbox' ? el.checked : el.value, el.getAttribute('data-value-type') || '')
    })
  })
}

function fieldRowHtml (field, index) {
  return `<tr class="${index === state.selectedFieldIndex ? 'active' : ''}" data-row="${index}">
    <td style="width:34px">${index + 1}</td>
    <td><input data-field-index="${index}" data-field-prop="id" value="${esc(field.id || '')}"></td>
    <td><input data-field-index="${index}" data-field-prop="name" value="${esc(field.name || '')}"></td>
    <td><input data-field-index="${index}" data-field-prop="caption.zh_CN" value="${esc(field.caption?.zh_CN || '')}"></td>
    <td><select data-field-index="${index}" data-field-prop="dataType">${optionHtml(DATA_TYPES, field.dataType)}</select></td>
    <td><input data-field-index="${index}" data-field-prop="fieldLength" data-value-type="number" type="number" value="${esc(field.fieldLength ?? '')}"></td>
    <td><input data-field-index="${index}" data-field-prop="nullable" data-value-type="boolean" type="checkbox" ${field.nullable === false ? '' : 'checked'}></td>
    <td><select data-field-index="${index}" data-field-prop="edit.mode">${optionHtml(EDIT_MODES, field.edit?.mode)}</select></td>
    <td style="width:92px">
      <button class="bdn-btn icon" data-action="up" data-index="${index}" title="上移">↑</button>
      <button class="bdn-btn icon" data-action="down" data-index="${index}" title="下移">↓</button>
      <button class="bdn-btn icon danger" data-action="delete-field" data-index="${index}" title="删除">×</button>
    </td>
  </tr>`
}

function readOnlyFieldRowHtml (field, index) {
  return `<tr>
    <td style="width:34px">${index + 1}</td>
    <td>${esc(field.id || '')}</td>
    <td>${esc(field.name || '')}</td>
    <td>${esc(field.caption?.zh_CN || field.title || '')}</td>
    <td>${esc(field.dataType || '')}</td>
    <td>${esc(field.fieldLength ?? '')}</td>
    <td>${field.nullable === false ? '否' : '是'}</td>
    <td>${esc(field.__fromFieldSet || '')}</td>
  </tr>`
}

function fileListHtml () {
  return state.items.map((it) => `<button class="bdn-item ${state.selected?.file === it.file ? 'active' : ''}" data-file="${esc(it.file)}">
    <strong>${esc(it.title || it.file)}</strong>
    <span class="bdn-muted">${esc(it.domain || 'base')} · ${esc(it.file)} · ${Number(it.fieldSetCount || 0)} 字段集</span>
  </button>`).join('') || '<div class="bdn-empty">暂无字典基础元数据文件</div>'
}

function fieldSetListHtml () {
  return fieldSetKeys().map((name) => {
    const fs = state.doc.fieldSets[name] || {}
    const composite = includeNames(fs).length > 0
    return `<button class="bdn-item ${state.selectedFieldSet === name ? 'active' : ''}" data-fs="${esc(name)}">
      <strong>${esc(name)} ${composite ? '<span class="bdn-chip">组合</span>' : ''}</strong>
      <span class="bdn-muted">${esc(fs.remark || '')} · ${totalFields(name)} 字段</span>
    </button>`
  }).join('') || '<div class="bdn-empty">暂无字段集</div>'
}

function managerHtml () {
  const fs = currentFieldSet()
  const own = ownFields()
  const included = includedFields()
  return `<div class="bdn-wrap">
    ${headerHtml('字典基础元数据(native_pages)')}
    <div class="bdn-body"><div class="bdn-stack">
      <section class="bdn-card third">
        <div class="bdn-card-head"><span class="bdn-card-title">字段集</span><span class="bdn-spacer"></span><button class="bdn-btn" data-action="add-fieldset">新增字段集</button><button class="bdn-btn danger" data-action="delete-fieldset">删除字段集</button></div>
        <div class="bdn-scroll"><div class="bdn-list">${fieldSetListHtml()}</div></div>
      </section>
      <section class="bdn-card flex">
        <div class="bdn-card-head"><span class="bdn-card-title">字段定义：${esc(state.selectedFieldSet || '无')}</span><span class="bdn-spacer"></span><button class="bdn-btn" data-action="add-field" ${fs ? '' : 'disabled'}>新增字段</button></div>
        ${included.length ? `<section class="bdn-section"><div class="bdn-section-head">组合引用字段 <span class="bdn-chip">${included.length}</span></div><div class="bdn-scroll"><table class="bdn-table"><thead><tr><th>#</th><th>ID</th><th>Name</th><th>标题</th><th>类型</th><th>长度</th><th>可空</th><th>来源字段集</th></tr></thead><tbody>${included.map(readOnlyFieldRowHtml).join('')}</tbody></table></div></section>` : ''}
        <div class="bdn-scroll"><table class="bdn-table"><thead><tr><th>#</th><th>ID</th><th>Name</th><th>标题</th><th>类型</th><th>长度</th><th>可空</th><th>控件</th><th>操作</th></tr></thead><tbody>${own.map(fieldRowHtml).join('') || '<tr><td colspan="9" class="bdn-empty">当前字段集暂无自有字段</td></tr>'}</tbody></table></div>
      </section>
    </div></div>
  </div>`
}

function listHtml () {
  const meta = state.doc?.moduleMeta || {}
  return `<div class="bdn-wrap">
    <div class="bdn-card flex" style="border:0;border-radius:0">
      <div class="bdn-card-head"><span class="bdn-card-title">基础文件</span><span class="bdn-spacer"></span><button class="bdn-btn" data-action="reload-list">刷新</button></div>
      <div class="bdn-scroll"><div class="bdn-list">${fileListHtml()}</div></div>
      <div class="bdn-card-head"><span class="bdn-card-title">字段集</span><span class="bdn-spacer"></span><button class="bdn-btn" data-action="add-fieldset">新增</button></div>
      <div class="bdn-scroll"><div class="bdn-list">${fieldSetListHtml()}</div></div>
      <div class="bdn-card-head"><span class="bdn-card-title">元数据属性</span></div>
      <div class="bdn-form">
        <label>编码</label><input data-module-meta="moduleCode" value="${esc(meta.moduleCode || '')}">
        <label>名称</label><input data-module-meta="metaName" value="${esc(meta.metaName || '')}">
        <label>版本</label><input data-module-meta="version" type="number" value="${esc(meta.version ?? '')}">
        <label>说明</label><input data-module-meta="remark" value="${esc(meta.remark || '')}">
      </div>
      <div class="bdn-status">${esc(state.message || '')}</div>
    </div>
  </div>`
}

function inspectorHtml () {
  const fs = currentFieldSet()
  const field = currentField()
  const allNames = fieldSetKeys().filter((name) => name !== state.selectedFieldSet)
  return `<div class="bdn-wrap">
    <div class="bdn-body">
      <section class="bdn-card">
        <div class="bdn-card-head"><span class="bdn-card-title">字段集信息</span></div>
        <div class="bdn-form">
          <label>名称</label><input data-action-input="rename-fieldset" value="${esc(state.selectedFieldSet || '')}" ${fs ? '' : 'readonly'}>
          <label>说明</label><input data-fs-prop="remark" value="${esc(fs?.remark || '')}">
          <label>组合引用</label><textarea data-fs-prop="includeFieldSets" placeholder="逗号分隔，例如 dictionaryCommonFields,tenantFields">${esc(includeNames(fs).join(', '))}</textarea>
          <label>可引用项</label><div class="bdn-muted">${esc(allNames.join(', ') || '无')}</div>
        </div>
      </section>
      <section class="bdn-card flex">
        <div class="bdn-card-head"><span class="bdn-card-title">字段检查器</span></div>
        <div class="bdn-form">
          ${field ? `
            <label>字段ID</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="id" value="${esc(field.id || '')}">
            <label>字段名称</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="name" value="${esc(field.name || '')}">
            <label>标题</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="caption.zh_CN" value="${esc(field.caption?.zh_CN || '')}">
            <label>数据类型</label><select data-field-index="${state.selectedFieldIndex}" data-field-prop="dataType">${optionHtml(DATA_TYPES, field.dataType)}</select>
            <label>长度</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="fieldLength" data-value-type="number" type="number" value="${esc(field.fieldLength ?? '')}">
            <label>整数位</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="intDigits" data-value-type="number" type="number" value="${esc(field.intDigits ?? '')}">
            <label>小数位</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="decimalDigits" data-value-type="number" type="number" value="${esc(field.decimalDigits ?? '')}">
            <label>可空</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="nullable" data-value-type="boolean" type="checkbox" ${field.nullable !== false ? 'checked' : ''}>
            <label>维度类型</label><select data-field-index="${state.selectedFieldIndex}" data-field-prop="dimType">${optionHtml(DIM_TYPES, field.dimType)}</select>
            <label>参照字典</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="refDict" value="${esc(field.refDict || '')}">
            <label>参照字段</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="refField" value="${esc(field.refField || '')}">
            <label>显示字段</label><input data-field-index="${state.selectedFieldIndex}" data-field-prop="displayField" value="${esc(field.displayField || '')}">
            <label>录入控件</label><select data-field-index="${state.selectedFieldIndex}" data-field-prop="edit.mode">${optionHtml(EDIT_MODES, field.edit?.mode)}</select>
            <label>说明</label><textarea data-field-index="${state.selectedFieldIndex}" data-field-prop="remark">${esc(field.remark || '')}</textarea>
          ` : '<label>当前字段</label><div class="bdn-muted">请选择一个自有字段</div>'}
          <label>保存状态</label><div>${state.dirty ? '未保存' : '已保存'}</div>
        </div>
      </section>
    </div>
  </div>`
}

function sourceHtml () {
  return `<div class="bdn-wrap">
    ${headerHtml('字典基础元数据源码')}
    <textarea class="bdn-code" data-source spellcheck="false">${esc(sourceText())}</textarea>
    <div class="bdn-status" style="color:${state.sourceError ? 'var(--sapNegativeTextColor,#b00)' : 'var(--sapContent_LabelColor,#6a6d70)'}">${esc(state.sourceError ? `JSON 错误：${state.sourceError}` : (state.message || ''))}</div>
  </div>`
}

function bindPage (root) {
  bindCommon(root)
  root.querySelectorAll('[data-file]').forEach((btn) => btn.addEventListener('click', () => selectFile(btn.getAttribute('data-file')).catch((err) => { state.message = err.message || String(err); refreshAll() })))
  root.querySelectorAll('[data-fs]').forEach((btn) => btn.addEventListener('click', () => selectFieldSet(btn.getAttribute('data-fs'))))
  root.querySelectorAll('[data-row]').forEach((row) => row.addEventListener('click', (ev) => {
    if (ev.target instanceof Element && ev.target.closest('input,select,button')) return
    selectField(row.getAttribute('data-row'))
  }))
  root.querySelector('[data-action="add-field"]')?.addEventListener('click', addField)
  root.querySelector('[data-action="add-fieldset"]')?.addEventListener('click', addFieldSet)
  root.querySelector('[data-action="delete-fieldset"]')?.addEventListener('click', deleteFieldSet)
  root.querySelector('[data-action="reload-list"]')?.addEventListener('click', async () => { await loadList(); refreshAll() })
  root.querySelectorAll('[data-action="delete-field"]').forEach((btn) => btn.addEventListener('click', () => deleteField(btn.getAttribute('data-index'))))
  root.querySelectorAll('[data-action="up"]').forEach((btn) => btn.addEventListener('click', () => moveField(btn.getAttribute('data-index'), -1)))
  root.querySelectorAll('[data-action="down"]').forEach((btn) => btn.addEventListener('click', () => moveField(btn.getAttribute('data-index'), 1)))
  root.querySelector('[data-action-input="rename-fieldset"]')?.addEventListener('change', (ev) => renameFieldSet(ev.target.value))
  root.querySelector('[data-source]')?.addEventListener('input', (ev) => updateSourceText(ev.target.value))
}

async function render (ctx, mode) {
  try {
    await ensureReady()
  } catch (err) {
    state.message = err.message || String(err)
  }
  const html = mode === 'list' ? listHtml() : mode === 'inspector' ? inspectorHtml() : mode === 'source' ? sourceHtml() : managerHtml()
  return mount(ctx, html, bindPage)
}

export default {
  defaultView: 'manager',
  views: {
    manager: (ctx) => render(ctx, 'manager'),
    list: (ctx) => render(ctx, 'list'),
    inspector: (ctx) => render(ctx, 'inspector'),
    source: (ctx) => render(ctx, 'source'),
  },
}
