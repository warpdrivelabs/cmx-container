/**
 * portal.onto.workshop —— 应用搭建台最小版 / 对象 360（O8/§9.3）。
 *
 * 面向业务角色的对象操作闭环：选一个对象 → 看其属性 + 各关系区（Search-Around 相关对象）+ 可执行动作。
 *   - model：当前对象标题 + 类型 + 刷新
 *   - explorer：选类型 → 列对象 → 点选进入 360 视图
 *   - content：对象属性卡 + 关系区（每个关系类型一块，Search-Around 列出相关对象）
 *   - property：可执行动作（点开填参 → O4 execute → 结果）
 *
 * 纯前端薄壳，复用 /api/onto/v1（manifest / object-sets/load / objects/{}/{}/links/{} / action-types/{}/execute）。
 */

const CFG = { apiBase: '', fetchInit: { credentials: 'same-origin' }, authHeaders: () => ({}) };
export function configure(o) { Object.assign(CFG, o || {}); return CFG; }
const API = '/api/onto/v1';

const state = {
  loaded: false, manifest: null,
  currentType: null, listRows: [],   // explorer 列表
  sel: null,                          // 选中对象 {pk,title,properties}
  typeDetail: null,                   // 当前类型完整定义
  relations: {},                      // link apiName → 相关对象行[]
  acResult: '', hosts: new Set(), err: '',
};

const { apiJson: _sharedApiJson } = globalThis.__cmxDataComp // 共享 fetch 封装（cmx-data-comp/lib/cmx-page-helpers.js）；经 CFG 转发保留组件壳 configure() 契约
async function apiJson (url, options = {}) { return _sharedApiJson(url, options, CFG) }
const { escHtml: esc } = globalThis.__cmxDataComp // 共享转义（cmx-data-comp/lib/cmx-page-helpers.js；最严格五字符集合，文本/属性上下文皆安全）
function hostRoot(h) { return h && (h.shadowRoot || h); }
function fmt(v) { if (v == null) return ''; if (typeof v === 'object') return JSON.stringify(v); return String(v); }
function typeMeta(a) { return (state.manifest.objectTypes || []).find(t => t.apiName === a); }
function linksOf(a) { return (state.manifest.linkTypes || []).filter(l => l.objectTypeA === a || l.objectTypeB === a); }

async function loadManifest() {
  state.manifest = await apiJson(API + '/manifest');
  if (!state.currentType && state.manifest.objectTypes && state.manifest.objectTypes.length) state.currentType = state.manifest.objectTypes[0].apiName;
}
async function loadTypeDetail() {
  if (!state.currentType) { state.typeDetail = null; return; }
  try { state.typeDetail = await apiJson(API + '/object-types/' + encodeURIComponent(state.currentType)); } catch { state.typeDetail = null; }
}
async function loadList() {
  if (!state.currentType) { state.listRows = []; return; }
  try { const r = await apiJson(API + '/object-sets/load', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ objectSet: { op: 'base', objectType: state.currentType }, limit: 200 }) }); state.listRows = (r && r.rows) || []; }
  catch (e) { state.listRows = []; state.err = e.message; }
}
/** 进入对象 360：载该对象 + 其所有关系区。 */
async function enterObject(pk) {
  state.sel = state.listRows.find(r => r.pk === pk) || null;
  state.relations = {};
  if (!state.sel) return;
  for (const l of linksOf(state.currentType)) {
    try {
      const r = await apiJson(API + `/objects/${encodeURIComponent(state.currentType)}/${encodeURIComponent(pk)}/links/${encodeURIComponent(l.apiName)}`);
      state.relations[l.apiName] = { rows: (r && r.rows) || [], terminal: (r && r.objectType) || '', link: l };
    } catch { state.relations[l.apiName] = { rows: [], terminal: '', link: l }; }
  }
}

// ── 渲染 ──
function modelHtml() {
  const t = state.currentType ? typeMeta(state.currentType) : null;
  return `<div class="o o-model">
    <b class="o-mt">🗂 对象 360 · 应用搭建台</b>
    ${state.sel ? `<span class="o-cur">${esc(state.sel.title || state.sel.pk)} <code>${esc(state.currentType)}#${esc(state.sel.pk)}</code></span>` : `<span class="o-cur">${t ? esc(t.displayName || t.apiName) : '（选对象）'}</span>`}
    <span class="o-sp"></span><button class="o-btn xs" data-act="refresh">刷新</button>
  </div>`;
}
function explorerHtml() {
  const types = (state.manifest.objectTypes || []).map(t => `<option value="${esc(t.apiName)}" ${t.apiName === state.currentType ? 'selected' : ''}>${esc(t.displayName || t.apiName)}</option>`).join('');
  const rows = state.listRows.map(r => `<li class="o-erow ${state.sel && state.sel.pk === r.pk ? 'sel' : ''}" data-act="enter" data-pk="${esc(r.pk)}"><span class="o-ename">${esc(r.title || r.pk)}</span><code>${esc(r.pk)}</code></li>`).join('');
  return `<div class="o o-explorer">
    <div class="o-hd">选对象类型</div>
    <select class="o-inp" data-act="pick-type">${types}</select>
    <div class="o-hd" style="margin-top:10px">对象 <span class="o-gn">${state.listRows.length}</span></div>
    <ul class="o-elist">${rows || '<li class="o-empty2">无对象</li>'}</ul>
  </div>`;
}
function contentHtml() {
  if (!state.sel) return `<div class="o o-content"><div class="ph">← 左侧选一个对象进入 360 视图</div></div>`;
  const p = state.sel.properties || {};
  const propRows = Object.keys(p).map(k => `<div class="o-kv"><span>${esc(k)}</span><b>${esc(fmt(p[k]))}</b></div>`).join('');
  const relBlocks = Object.keys(state.relations).map(link => {
    const rel = state.relations[link];
    const items = rel.rows.map(r => `<li class="o-relrow"><span>${esc(r.title || r.pk)}</span><code>${esc(r.pk)}</code></li>`).join('');
    return `<div class="o-relblock">
      <div class="o-relhd">🔗 ${esc(rel.link.displayName || link)} → ${esc(rel.terminal)} <span class="o-gn">${rel.rows.length}</span></div>
      <ul class="o-rellist">${items || '<li class="o-empty2">无相关对象</li>'}</ul>
    </div>`;
  }).join('');
  return `<div class="o o-content o-scroll">
    <div class="o-card">
      <div class="o-chd">📦 ${esc(state.sel.title || state.sel.pk)} <code>${esc(state.sel.pk)}</code></div>
      <div class="o-kvs">${propRows || '<div class="o-empty2">无属性</div>'}</div>
    </div>
    <div class="o-relwrap">${relBlocks || '<div class="o-empty2">无关系</div>'}</div>
  </div>`;
}
function propertyHtml() {
  const acts = (state.manifest.actionTypes || []);
  if (!state.sel) return `<div class="o o-prop"><div class="ph">选对象后可执行动作</div></div>`;
  const btns = acts.map(a => `<button class="o-btn lnk" data-act="run-action" data-id="${esc(a.apiName)}">⚡ ${esc(a.displayName || a.apiName)}</button>`).join('');
  return `<div class="o o-prop">
    <div class="o-phd">可执行动作 <span class="o-gn">${acts.length}</span></div>
    <div class="o-acts">${btns || '<div class="o-empty2">无动作</div>'}</div>
    <div class="o-runbox" data-role="ac-result">${state.acResult || ''}</div>
  </div>`;
}

// ── 挂载/刷新/绑定 ──
function viewHtml(v) { if (!state.loaded) return `<div class="o"><div class="ph">加载中…</div></div>`; return { model: modelHtml, explorer: explorerHtml, content: contentHtml, property: propertyHtml }[v](); }
let _lp = null;
function ensureLoaded() { if (state.loaded) return Promise.resolve(); if (_lp) return _lp; _lp = (async () => { try { await loadManifest(); await loadTypeDetail(); await loadList(); state.loaded = true; } catch (e) { state.err = e.message; state.loaded = true; } })(); return _lp; }
function mount(ctx, view) { const host = ctx.host; state.hosts.add(host); host.__view = view; const render = () => { const root = hostRoot(host); if (!root || root.isConnected === false) return; root.innerHTML = `<style>${css()}</style>${viewHtml(view)}`; bind(root, view); }; requestAnimationFrame(async () => { render(); await ensureLoaded(); render(); }); return `<style>${css()}</style>${viewHtml(view)}`; }
function refresh(v) { for (const h of state.hosts) if (h.__view === v) { const root = hostRoot(h); if (root && root.isConnected !== false) { root.innerHTML = `<style>${css()}</style>${viewHtml(v)}`; bind(root, v); } } }
function refreshAll() { ['model', 'explorer', 'content', 'property'].forEach(refresh); }

function bind(root, view) {
  root.querySelectorAll('[data-act]').forEach(el => {
    const ev = el.tagName === 'SELECT' ? 'change' : 'click';
    el.addEventListener(ev, async (e) => {
      const a = el.getAttribute('data-act');
      if (a === 'refresh') { await loadList(); if (state.sel) await enterObject(state.sel.pk); return refreshAll(); }
      if (a === 'pick-type') { state.currentType = el.value; state.sel = null; state.relations = {}; await loadTypeDetail(); await loadList(); return refreshAll(); }
      if (a === 'enter') { await enterObject(el.getAttribute('data-pk')); return refreshAll(); }
      if (a === 'run-action') return runAction(el.getAttribute('data-id'));
    });
  });
}
async function runAction(id) {
  const action = (state.manifest.actionTypes || []).find(a => a.apiName === id);
  // 拉动作完整定义得参数
  let def = null; try { def = await apiJson(API + '/action-types/' + encodeURIComponent(id)); } catch { /* */ }
  const params = (def && def.parameters) || [];
  const inputs = params.map(p => `<div style="margin:6px 0"><label style="font-size:11.5px;color:var(--o-muted,#94a3b8)">${esc(p.name)}${p.required ? ' <span style="color:var(--o-err,#ef4444)">*</span>' : ''}</label><input class="o-inp" data-k="p:${esc(p.name)}" placeholder="参数值"/></div>`).join('');
  const hint = state.sel ? `<div class="o-dlgmuted">当前对象 <b>${esc(state.currentType)}#${esc(state.sel.pk)}</b>（如动作参数含对象主键，请填 ${esc(state.sel.pk)}）</div>` : '';
  const c = await openDialog({ title: '执行 · ' + (action ? (action.displayName || id) : id), severity: 'warn', body: '<p class="o-dlgwarn">执行将真实写回对象并触发副作用。</p>' + hint + (inputs || '<p class="o-dlgmuted">无参数</p>'), buttons: [{ label: '取消', id: '__cancel' }, { label: '执行', id: 'run', kind: 'ok' }] });
  if (c !== 'run') return;
  const args = {}; const V = _lastDialogValues || {};
  for (const k in V) { if (!k.startsWith('p:')) continue; const v = (V[k] || '').trim(); if (!v) continue; const n = k.slice(2); let pv; try { pv = JSON.parse(v); } catch { pv = v; } args[n] = pv; }
  try {
    const r = await apiJson(API + '/action-types/' + encodeURIComponent(id) + '/execute', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ params: args, actor: 'workshop', subjects: ['role:admin'] }) });
    state.acResult = `<div class="o-runok">✔ 已执行 <b>${esc(id)}</b> · 编辑 ${(r.edits || []).length} 条${r.effects ? ' · 副作用 ' + r.effects : ''} <span class="o-runmeta">${esc(r.status || '')}</span></div>`;
    refresh('property'); // 先即时显示结果
    // 执行后刷新当前对象（属性可能变；Search-Around 可能慢，放结果显示之后）
    await loadList(); if (state.sel) await enterObject(state.sel.pk);
    refresh('content'); refresh('explorer'); refresh('model');
  } catch (e) { state.acResult = `<div class="o-runerr">✘ ${esc(e.message)}</div>`; refresh('property'); }
}

// 简易对话框（快照 data-k）
let _lastDialogValues = {};
function openDialog(opts) {
  return new Promise((resolve) => {
    const ov = document.createElement('div'); ov.className = 'o-dlg-overlay';
    const btns = (opts.buttons || [{ label: '确定', id: 'ok', kind: 'primary' }]).map(b => `<button class="o-btn ${b.kind || ''}" data-dlg="${esc(b.id)}">${esc(b.label)}</button>`).join('');
    ov.innerHTML = `<style>${css()}</style><div class="o o-dlg" role="dialog"><div class="o-dlghd">${esc(opts.title)}</div><div class="o-dlgbody">${opts.body || ''}</div><div class="o-dlgfoot">${btns}</div></div>`;
    const done = (id) => { _lastDialogValues = {}; ov.querySelectorAll('[data-k]').forEach(el => { _lastDialogValues[el.getAttribute('data-k')] = el.value; }); ov.remove(); resolve(id === '__cancel' ? null : id); };
    ov.addEventListener('click', (e) => { if (e.target === ov) return done('__cancel'); const b = e.target.closest('[data-dlg]'); if (b) done(b.getAttribute('data-dlg')); });
    document.body.appendChild(ov);
  });
}

function css() {
  return `
  .o{--o-bg:var(--sapBackgroundColor,#0b1020);--o-fg:var(--sapTextColor,#e6ecf5);--o-muted:var(--sapContent_LabelColor,#94a3b8);--o-border:var(--sapList_BorderColor,#243049);--o-panel:var(--sapList_Background,#121a2e);--o-accent:var(--sapButton_Emphasized_Background,#22d3ee);--o-ok:#22c55e;--o-err:#ef4444;--o-mono:ui-monospace,Menlo,monospace;color:var(--o-fg);font:13px/1.5 ui-sans-serif,system-ui,'PingFang SC',sans-serif;height:100%;box-sizing:border-box}
  .ph,.o-empty2{color:var(--o-muted);padding:14px;text-align:center;font-size:12.5px}
  code{color:var(--o-muted);font-family:var(--o-mono);font-size:11px}
  .o-btn{cursor:pointer;border:1px solid var(--o-border);background:var(--o-panel);color:var(--o-fg);border-radius:7px;padding:5px 10px;font-size:12px}
  .o-btn:hover{border-color:var(--o-accent)}.o-btn.xs{padding:3px 8px;font-size:11.5px}.o-btn.ok{background:var(--o-ok);border:none;color:#052e16;font-weight:700}.o-btn.primary{background:var(--o-accent);border:none;color:#04283a;font-weight:700}
  .o-inp{width:100%;background:var(--o-panel);border:1px solid var(--o-border);color:var(--o-fg);border-radius:6px;padding:5px 8px;font-size:12px;box-sizing:border-box}
  .o-model{display:flex;align-items:center;gap:10px;padding:10px 14px;height:100%;box-sizing:border-box}.o-mt{font-size:14px}.o-cur{font-size:12.5px}.o-sp{flex:1}
  .o-explorer{padding:10px;overflow:auto}.o-hd{font-size:11.5px;font-weight:700;color:var(--o-muted);padding:4px 2px}.o-gn{background:var(--o-panel);border-radius:10px;padding:0 7px;font-size:10.5px;margin-left:4px}
  .o-elist{list-style:none;margin:6px 0 0;padding:0}.o-erow{display:flex;align-items:center;gap:8px;padding:5px 8px;border-radius:6px;cursor:pointer}.o-erow:hover{background:var(--o-panel)}.o-erow.sel{background:var(--o-panel);box-shadow:inset 2.5px 0 0 var(--o-accent)}.o-ename{flex:1;font-size:12.5px}
  .o-content{height:100%}.o-scroll{overflow:auto;padding:14px}
  .o-card{background:var(--o-panel);border:1px solid var(--o-border);border-radius:12px;padding:12px 14px;margin-bottom:14px}
  .o-chd{font-size:14px;font-weight:700;margin-bottom:8px}
  .o-kv{display:flex;justify-content:space-between;gap:10px;padding:5px 0;border-bottom:1px solid var(--o-border);font-size:12.5px}.o-kv span{color:var(--o-muted)}.o-kv b{font-family:var(--o-mono);font-weight:600;word-break:break-all}
  .o-relwrap{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:12px}
  .o-relblock{background:var(--o-panel);border:1px solid var(--o-border);border-radius:10px;padding:10px}
  .o-relhd{font-size:12px;font-weight:700;color:var(--o-accent);margin-bottom:6px}
  .o-rellist{list-style:none;margin:0;padding:0}.o-relrow{display:flex;justify-content:space-between;gap:6px;padding:4px 0;border-bottom:1px solid var(--o-border);font-size:12px}
  .o-prop{padding:12px;overflow:auto}.o-phd{font-size:13.5px;font-weight:700;margin-bottom:10px}
  .o-acts{display:flex;flex-direction:column;gap:6px}.o-btn.lnk{text-align:left}
  .o-runbox{margin-top:12px}.o-runok{padding:8px 11px;border-radius:8px;background:rgba(34,197,94,.1);border:1px solid rgba(34,197,94,.35);color:var(--o-ok);font-size:12.5px}.o-runerr{padding:8px 11px;border-radius:8px;background:rgba(239,68,68,.1);border:1px solid rgba(239,68,68,.35);color:var(--o-err);font-size:12.5px}.o-runmeta{color:var(--o-muted);font-size:11px}
  .o-dlg-overlay{position:fixed;inset:0;background:rgba(4,8,18,.6);display:flex;align-items:center;justify-content:center;z-index:1000}
  .o-dlg{width:420px;max-width:92vw;background:var(--o-panel);border:1px solid var(--o-border);border-radius:14px}
  .o-dlghd{padding:12px 16px;font-size:14px;font-weight:700;border-bottom:1px solid var(--o-border)}.o-dlgbody{padding:14px 16px;font-size:12.5px}.o-dlgwarn{color:var(--o-err);font-weight:600}.o-dlgmuted{color:var(--o-muted);margin:6px 0}.o-dlgfoot{display:flex;justify-content:flex-end;gap:8px;padding:12px 16px;border-top:1px solid var(--o-border)}
  label{display:block}
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
