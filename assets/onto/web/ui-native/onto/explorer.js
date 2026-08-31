/**
 * portal.onto.explorer —— 对象浏览器（O8/§9.2；native_pages 四区）。
 *
 * 逛 O3 灌入 / 动作写入的**对象实例**：
 *   - model：标题 + 当前对象类型 + 计数 + 刷新
 *   - explorer：对象类型列表 + 对象集构造器（加过滤谓词，可移除）
 *   - content：对象列表表格（/object-sets/load 编译一条 SQL）
 *   - property：选中对象详情（全属性）+ 关系区（点关系 → Search-Around 载入相关对象集，图谱式钻取）
 *
 * 纯前端薄壳：全部数据经 /api/onto/v1/*（manifest / object-sets/load / secure /objects/{}/{}/links/{}）。
 * 改壳须重启本体服务。
 */

const CFG = { apiBase: '', fetchInit: { credentials: 'same-origin' }, authHeaders: () => ({}) };
export function configure(o) { Object.assign(CFG, o || {}); return CFG; }

const API = '/api/onto/v1';
const OPS = [['eq', '='], ['ne', '≠'], ['gt', '>'], ['ge', '≥'], ['lt', '<'], ['le', '≤'], ['contains', '包含'], ['isNull', '为空']];

const state = {
  loaded: false,
  manifest: null,
  currentType: null,      // 选中对象类型 apiName
  typeDetail: null,       // 当前类型完整定义（含 properties；manifest 只给 meta）
  filters: [],            // [{property, op, value}]
  rows: [],               // 已载入对象
  sel: null,              // 选中对象 {pk, title, properties}
  breadcrumb: [],         // Search-Around 钻取路径 [{type, pk, link}]
  hosts: new Set(),
  err: '',
};

async function apiJson(url, options = {}) {
  const full = (CFG.apiBase && url.charAt(0) === '/') ? CFG.apiBase + url : url;
  const res = await fetch(full, { ...CFG.fetchInit, ...options, headers: { Accept: 'application/json', ...CFG.authHeaders(), ...(options.headers || {}) } });
  let j = null; try { j = await res.json(); } catch { /* */ }
  if (!res.ok || (j && typeof j.code === 'number' && j.code !== 0)) throw new Error((j && (j.msg || j.error)) || `HTTP ${res.status}`);
  return j && typeof j === 'object' && 'data' in j ? j.data : j;
}
function esc(s) { return String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c])); }
function hostRoot(host) { return host && (host.shadowRoot || host); }

// ── 数据 ──
async function loadManifest() {
  state.manifest = await apiJson(API + '/manifest');
  if (!state.currentType && state.manifest.objectTypes && state.manifest.objectTypes.length) {
    state.currentType = state.manifest.objectTypes[0].apiName;
  }
}
/** 拉当前类型完整定义（manifest 只给 meta，属性/关系需详情）。 */
async function loadTypeDetail() {
  if (!state.currentType) { state.typeDetail = null; return; }
  try { state.typeDetail = await apiJson(API + '/object-types/' + encodeURIComponent(state.currentType)); }
  catch { state.typeDetail = null; }
}
/** 构造当前对象集代数（Base + Filter*）。 */
function buildObjectSet() {
  let set = { op: 'base', objectType: state.currentType };
  for (const f of state.filters) {
    let predicate;
    if (f.op === 'isNull') predicate = { kind: 'isNull', property: f.property };
    else if (f.op === 'contains') predicate = { kind: 'contains', property: f.property, value: f.value };
    else predicate = { kind: f.op, property: f.property, value: coerce(f.value) };
    set = { op: 'filter', source: set, predicate };
  }
  return set;
}
function coerce(v) { if (v === '') return v; const n = Number(v); return Number.isNaN(n) ? v : n; }

async function loadObjects() {
  if (!state.currentType) { state.rows = []; return; }
  try {
    const r = await apiJson(API + '/object-sets/load', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ objectSet: buildObjectSet(), limit: 500 }) });
    state.rows = (r && r.rows) || [];
    state.err = '';
  } catch (e) { state.rows = []; state.err = e.message; }
}
/** Search-Around：从选中对象经关系载入相关对象集（替换 content 列表 + 记面包屑）。 */
async function searchAround(link, terminalType) {
  if (!state.sel) return;
  const fromType = state.currentType, fromPk = state.sel.pk;
  try {
    const r = await apiJson(API + `/objects/${encodeURIComponent(fromType)}/${encodeURIComponent(fromPk)}/links/${encodeURIComponent(link)}`);
    state.breadcrumb.push({ type: fromType, pk: fromPk, link });
    state.currentType = (r && r.objectType) || terminalType || fromType;
    await loadTypeDetail();
    state.rows = (r && r.rows) || [];
    state.filters = [];
    state.sel = null;
    refreshAll();
  } catch (e) { flash(e.message); }
}

// ── 渲染 ──
function typeMeta(apiName) { return (state.manifest.objectTypes || []).find(t => t.apiName === apiName); }
function linksOf(apiName) { return (state.manifest.linkTypes || []).filter(l => l.objectTypeA === apiName || l.objectTypeB === apiName); }

function modelHtml() {
  const t = state.currentType ? typeMeta(state.currentType) : null;
  const crumb = state.breadcrumb.length ? ` · 钻取 ${state.breadcrumb.map(b => esc(b.link)).join(' → ')}` : '';
  return `<div class="o o-model">
    <b class="o-mt">🔎 对象浏览器</b>
    <span class="o-cur">${t ? esc(t.displayName || t.apiName) : '（未选类型）'} <code>${esc(state.currentType || '')}</code></span>
    <span class="o-cnt">${state.rows.length} 个对象${crumb}</span>
    <span class="o-sp"></span>
    ${state.breadcrumb.length ? '<button class="o-btn xs" data-act="back">← 返回</button>' : ''}
    <button class="o-btn xs" data-act="refresh">刷新</button>
  </div>`;
}

function explorerHtml() {
  const types = state.manifest.objectTypes || [];
  const list = types.map(t => `<li class="o-erow ${t.apiName === state.currentType ? 'sel' : ''}" data-act="pick-type" data-type="${esc(t.apiName)}">
    <span class="o-ename">${esc(t.displayName || t.apiName)}</span><code>${esc(t.apiName)}</code></li>`).join('');
  const t = state.currentType ? typeMeta(state.currentType) : null;
  const props = (state.typeDetail && state.typeDetail.properties) || [];
  const propOpts = props.map(p => `<option value="${esc(p.apiName)}">${esc(p.apiName)}</option>`).join('');
  const chips = state.filters.map((f, i) => `<span class="o-chip">${esc(f.property)} ${esc(opLabel(f.op))} ${esc(f.op === 'isNull' ? '' : f.value)} <button class="o-x" data-act="del-filter" data-i="${i}">✕</button></span>`).join('');
  return `<div class="o o-explorer">
    <div class="o-hd">对象类型 <span class="o-gn">${types.length}</span></div>
    <ul class="o-elist">${list || '<li class="o-empty2">无对象类型</li>'}</ul>
    <div class="o-hd2">对象集构造器</div>
    <div class="o-fb">
      <select class="o-inp xs" data-fb="property">${propOpts || '<option>（无属性）</option>'}</select>
      <select class="o-inp xs" data-fb="op">${OPS.map(([v, l]) => `<option value="${v}">${l}</option>`).join('')}</select>
      <input class="o-inp xs" data-fb="value" placeholder="值"/>
      <button class="o-btn xs" data-act="add-filter">加过滤</button>
    </div>
    <div class="o-chips">${chips}</div>
  </div>`;
}

function contentHtml() {
  const props = ((state.typeDetail && state.typeDetail.properties) || []).slice(0, 5).map(p => p.apiName);
  if (state.err) return `<div class="o o-content"><div class="o-err">✘ ${esc(state.err)}</div></div>`;
  const head = ['pk', 'title', ...props].map(c => `<th>${esc(c)}</th>`).join('');
  const body = state.rows.map(r => {
    const p = r.properties || {};
    const cells = [r.pk, r.title, ...props.map(c => fmt(p[c]))].map(v => `<td>${esc(v)}</td>`).join('');
    return `<tr class="o-orow ${state.sel && state.sel.pk === r.pk ? 'sel' : ''}" data-act="pick-obj" data-pk="${esc(r.pk)}">${cells}</tr>`;
  }).join('');
  return `<div class="o o-content">
    <div class="o-thd">对象列表 <span class="o-gn">${state.rows.length}</span></div>
    <div class="o-tblwrap"><table class="o-tbl"><thead><tr>${head}</tr></thead><tbody>${body || `<tr><td colspan="9" class="o-empty2">无对象（选类型或调整过滤）</td></tr>`}</tbody></table></div>
  </div>`;
}

function propertyHtml() {
  if (!state.sel) return `<div class="o o-prop"><div class="ph">点左侧对象查看详情</div></div>`;
  const p = state.sel.properties || {};
  const rows = Object.keys(p).map(k => `<div class="o-kv"><span>${esc(k)}</span><b>${esc(fmt(p[k]))}</b></div>`).join('');
  const links = linksOf(state.currentType);
  const linkBtns = links.map(l => {
    const terminal = l.objectTypeA === state.currentType ? l.objectTypeB : l.objectTypeA;
    return `<button class="o-btn xs lnk" data-act="search-around" data-link="${esc(l.apiName)}" data-terminal="${esc(terminal)}">${esc(l.displayName || l.apiName)} → ${esc(terminal)}</button>`;
  }).join('');
  return `<div class="o o-prop">
    <div class="o-phd">📦 <b>${esc(state.sel.title || state.sel.pk)}</b> <code>${esc(state.sel.pk)}</code></div>
    <div class="o-phd2">属性 <span class="o-gn">${Object.keys(p).length}</span></div>
    ${rows || '<div class="o-empty2">无属性</div>'}
    <div class="o-phd2">关系（Search-Around 钻取）<span class="o-gn">${links.length}</span></div>
    <div class="o-lnks">${linkBtns || '<div class="o-empty2">无关系</div>'}</div>
  </div>`;
}

function opLabel(op) { const f = OPS.find(([v]) => v === op); return f ? f[1] : op; }
function fmt(v) { if (v == null) return ''; if (typeof v === 'object') return JSON.stringify(v); return String(v); }

// ── 挂载/刷新/绑定 ──
function viewHtml(view) {
  if (!state.loaded) return `<div class="o"><div class="ph">加载中…</div></div>`;
  if (view === 'model') return modelHtml();
  if (view === 'explorer') return explorerHtml();
  if (view === 'content') return contentHtml();
  if (view === 'property') return propertyHtml();
  return '';
}
let _lp = null;
function ensureLoaded() {
  if (state.loaded) return Promise.resolve();
  if (_lp) return _lp;
  _lp = (async () => { try { await loadManifest(); await loadTypeDetail(); await loadObjects(); state.loaded = true; } catch (e) { state.err = e.message; state.loaded = true; } })();
  return _lp;
}
function mount(ctx, view) {
  const host = ctx.host; state.hosts.add(host); host.__view = view;
  const render = () => { const root = hostRoot(host); if (!root || root.isConnected === false) return; root.innerHTML = `<style>${css()}</style>${viewHtml(view)}`; bind(root, view); };
  requestAnimationFrame(async () => { render(); await ensureLoaded(); render(); });
  return `<style>${css()}</style>${viewHtml(view)}`;
}
function refresh(view) { for (const h of state.hosts) { if (h.__view === view) { const root = hostRoot(h); if (root && root.isConnected !== false) { root.innerHTML = `<style>${css()}</style>${viewHtml(view)}`; bind(root, view); } } } }
function refreshAll() { ['model', 'explorer', 'content', 'property'].forEach(refresh); }
function flash(msg) { state.err = msg; refresh('content'); }

function bind(root, view) {
  root.querySelectorAll('[data-act]').forEach(el => {
    el.addEventListener('click', async (e) => {
      const a = el.getAttribute('data-act');
      if (a === 'refresh') { await loadObjects(); return refreshAll(); }
      if (a === 'back') { state.breadcrumb.pop(); if (state.breadcrumb.length === 0) { /* 顶层 */ } state.sel = null; await loadObjects(); return refreshAll(); }
      if (a === 'pick-type') { state.currentType = el.getAttribute('data-type'); state.filters = []; state.sel = null; state.breadcrumb = []; await loadTypeDetail(); await loadObjects(); return refreshAll(); }
      if (a === 'add-filter') return addFilter(root);
      if (a === 'del-filter') { state.filters.splice(+el.getAttribute('data-i'), 1); await loadObjects(); return refreshAll(); }
      if (a === 'pick-obj') { const pk = el.getAttribute('data-pk'); state.sel = state.rows.find(r => r.pk === pk) || null; refresh('content'); refresh('property'); return; }
      if (a === 'search-around') return searchAround(el.getAttribute('data-link'), el.getAttribute('data-terminal'));
    });
  });
}
async function addFilter(root) {
  const g = s => { const el = root.querySelector(s); return el ? el.value : ''; };
  const property = g('[data-fb="property"]'), op = g('[data-fb="op"]'), value = g('[data-fb="value"]');
  if (!property) return;
  state.filters.push({ property, op, value });
  await loadObjects(); refreshAll();
}

function css() {
  return `
  .o{--o-bg:var(--sapBackgroundColor,#0b1020);--o-fg:var(--sapTextColor,#e6ecf5);--o-muted:var(--sapContent_LabelColor,#94a3b8);--o-border:var(--sapList_BorderColor,#243049);--o-panel:var(--sapList_Background,#121a2e);--o-accent:var(--sapButton_Emphasized_Background,#22d3ee);--o-ok:#22c55e;--o-err:#ef4444;--o-mono:ui-monospace,Menlo,monospace;color:var(--o-fg);font:13px/1.5 ui-sans-serif,system-ui,'PingFang SC',sans-serif;height:100%;box-sizing:border-box}
  .ph,.o-empty2{color:var(--o-muted);padding:14px;text-align:center;font-size:12.5px}
  code{color:var(--o-muted);font-family:var(--o-mono);font-size:11px}
  .o-btn{cursor:pointer;border:1px solid var(--o-border);background:var(--o-panel);color:var(--o-fg);border-radius:7px;padding:5px 10px;font-size:12px}
  .o-btn:hover{border-color:var(--o-accent)}
  .o-btn.xs{padding:3px 8px;font-size:11.5px}
  .o-inp{background:var(--o-panel);border:1px solid var(--o-border);color:var(--o-fg);border-radius:6px;padding:5px 8px;font-size:12px;box-sizing:border-box}
  .o-inp.xs{padding:3px 6px;font-size:11.5px}
  .o-model{display:flex;align-items:center;gap:10px;padding:10px 14px;height:100%;box-sizing:border-box}
  .o-mt{font-size:14px}.o-cur{font-size:12.5px}.o-cnt{color:var(--o-muted);font-size:11.5px}.o-sp{flex:1}
  .o-explorer{padding:10px;overflow:auto}
  .o-hd,.o-hd2{font-size:11.5px;font-weight:700;color:var(--o-muted);padding:6px 2px 4px}
  .o-hd2{margin-top:10px;border-top:1px solid var(--o-border);padding-top:10px}
  .o-gn{background:var(--o-panel);border-radius:10px;padding:0 7px;font-size:10.5px;margin-left:4px}
  .o-elist{list-style:none;margin:0;padding:0}
  .o-erow{display:flex;align-items:center;gap:8px;padding:5px 8px;border-radius:6px;cursor:pointer}
  .o-erow:hover{background:var(--o-panel)}.o-erow.sel{background:var(--o-panel);box-shadow:inset 2.5px 0 0 var(--o-accent)}
  .o-ename{flex:1;font-size:12.5px}
  .o-fb{display:flex;gap:4px;flex-wrap:wrap;align-items:center;margin:4px 0}
  .o-fb .o-inp{flex:1;min-width:60px}
  .o-chips{display:flex;flex-wrap:wrap;gap:5px;margin-top:6px}
  .o-chip{background:var(--o-panel);border:1px solid var(--o-border);border-radius:12px;padding:2px 8px;font-size:11px;display:flex;align-items:center;gap:4px}
  .o-x{background:none;border:none;color:var(--o-muted);cursor:pointer;font-size:10px;padding:0}
  .o-content{display:flex;flex-direction:column;height:100%}
  .o-thd{font-size:12px;font-weight:700;color:var(--o-muted);padding:8px 10px;border-bottom:1px solid var(--o-border)}
  .o-tblwrap{flex:1;overflow:auto}
  .o-tbl{width:100%;border-collapse:collapse;font-size:12px}
  .o-tbl th{position:sticky;top:0;background:var(--o-panel);color:var(--o-muted);font-weight:600;font-size:11px;text-align:left;padding:6px 8px;border-bottom:1px solid var(--o-border)}
  .o-tbl td{padding:5px 8px;border-bottom:1px solid var(--o-border)}
  .o-orow{cursor:pointer}.o-orow:hover{background:var(--o-panel)}.o-orow.sel{background:rgba(34,211,238,.08)}
  .o-err{color:var(--o-err);padding:14px;font-size:12.5px}
  .o-prop{padding:12px;overflow:auto}
  .o-phd{font-size:13.5px;font-weight:700;margin-bottom:8px}
  .o-phd2{font-size:12px;font-weight:700;color:var(--o-muted);margin:12px 0 6px;border-top:1px solid var(--o-border);padding-top:10px}
  .o-kv{display:flex;justify-content:space-between;gap:10px;padding:5px 0;border-bottom:1px solid var(--o-border);font-size:12.5px}
  .o-kv span{color:var(--o-muted)}.o-kv b{font-family:var(--o-mono);font-weight:600;text-align:right;word-break:break-all}
  .o-lnks{display:flex;flex-direction:column;gap:6px}
  .o-btn.lnk{text-align:left;justify-content:flex-start}
  `;
}

export { mount };
export default {
  defaultView: 'content',
  views: {
    async model(ctx) { return mount(ctx, 'model'); },
    async explorer(ctx) { return mount(ctx, 'explorer'); },
    async content(ctx) { return mount(ctx, 'content'); },
    async property(ctx) { return mount(ctx, 'property'); },
  },
};
