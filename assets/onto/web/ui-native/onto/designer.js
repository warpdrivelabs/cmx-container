/*
 * portal.onto.designer —— 本体设计工作台（native_pages 四区：model / explorer / content / property）。
 * **薄壳**：本体图渲染/布局/拖拽/连线全部下放到独立组件 <cmx-ontology-graph>（@cmx/ontology-graph，
 * 与 @cmx/decision-graph 平级，vendor 到 assets/onto/web/ui-native/vendor/）。本壳只负责：后端 I/O
 * （装载 manifest/对象类型/关系、保存、发布）、四区渲染、以及选中元素的强类型 Inspector（属性表格/
 * 关系速建气泡——领域逻辑作为宿主注入的编辑器留在 shell，不进通用图核）。
 *
 * 四区（对齐 portal 四区标准 + 前端定义 UX 方案）：
 *   - model：顶部本体元信息（名/版本/统计/校验/发布），四区装配锚。
 *   - explorer：七类元素分组树（对象/关系/接口/共享属性/动作/函数），查找 + 新建。
 *   - content：<cmx-ontology-graph> 组件画布 + 工具栏（+对象/+关系/重排/保存/发布）。
 *   - property：选中元素的 Inspector（对象类型概览 + 属性卡内表格 / 关系详情）。
 *
 * 组件加载：native 页由 new Function 执行、不能相对 import，故首次挂载时 fetch 组件源
 * （/api/native-pages/portal.onto.graph-component）→ blob module URL → 动态 import → 自注册
 * <cmx-ontology-graph>（一次性、幂等）。独立 :8097 与门户 F3 反代都走 /api/native-pages。
 */

const CFG = { apiBase: '', fetchInit: { credentials: 'same-origin' }, authHeaders: () => ({}) };
export function configure(o) { Object.assign(CFG, o || {}); return CFG; }

const API = '/api/onto/v1';
const BASE_TYPES = ['string', 'integer', 'long', 'double', 'decimal', 'boolean', 'date', 'timestamp', 'array', 'struct', 'attachment', 'mediaReference', 'marking', 'geohash', 'geoShape', 'vector'];
const CARDS = ['oneToOne', 'oneToMany', 'manyToMany'];
const CARD_LABEL = { oneToOne: '1:1', oneToMany: '1:N', manyToMany: 'N:M' };
const STATUS = ['experimental', 'active', 'deprecated'];
const STATUS_LABEL = { experimental: '试验', active: '激活', deprecated: '废弃' };
// 语义类型词表（复用 cmx-meta-data semanticType 惯例；驱动渲染/校验提示）。
const SEMANTIC_TYPES = ['', 'money', 'percent', 'quantity', 'rate', 'email', 'phone', 'url', 'idCard', 'countryCode', 'currency', 'geoPoint', 'color', 'duration'];
const SEMANTIC_LABEL = { '': '—', money: '金额', percent: '百分比', quantity: '数量', rate: '比率', email: '邮箱', phone: '电话', url: '链接', idCard: '证件号', countryCode: '国家码', currency: '币种', geoPoint: '坐标', color: '颜色', duration: '时长' };
// 复合类型（可展开子属性）。
const COMPOSITE_TYPES = ['struct', 'array'];
// UI5 动能层词表（动作/函数）。
const FN_RUNTIMES = ['feel', 'rhai', 'wasm', 'nativeRust'];
const FN_RT_LABEL = { feel: 'FEEL 表达式', rhai: 'Rhai 脚本', wasm: 'WASM 沙箱', nativeRust: '内置 Rust' };
const FN_KINDS = ['query', 'derivedProperty', 'validation', 'actionLogic', 'aggregation'];
const FN_KIND_LABEL = { query: '查询', derivedProperty: '派生属性', validation: '校验', actionLogic: '动作逻辑', aggregation: '聚合' };
const EDIT_OPS = ['createObject', 'modifyObject', 'deleteObject', 'addLink', 'removeLink'];
const EDIT_OP_LABEL = { createObject: '创建对象', modifyObject: '修改对象', deleteObject: '删除对象', addLink: '加关系', removeLink: '删关系' };
const SIDE_KINDS = ['notification', 'webhook', 'callFunction', 'startBusinessProcess', 'computeReport', 'emitEvent'];
const SIDE_KIND_LABEL = { notification: '通知', webhook: 'Webhook', callFunction: '调用函数', startBusinessProcess: '触发流程', computeReport: '生成报表', emitEvent: '发事件' };
// apiName 合法性（镜像后端 is_valid_api_name）。
function validApiName(s) { return /^[A-Za-z_][A-Za-z0-9_]*$/.test(s || ''); }

// ── 单例状态（本体设计工作台通常单实例；多本体可扩 props.ontology）──
const state = {
  loaded: false,
  manifest: null,     // GET /manifest
  spec: null,         // 组件 payload {nodes,edges}
  sel: null,          // { kind:'object'|'interface'|'link', id }
  detail: null,       // 选中元素的完整后端 def
  dirty: false,
  versions: [],
  shared: [],         // 共享属性目录（含 baseType/semanticType，UI2 引用用）
  selRows: new Set(), // UI2 批量：选中的属性行索引
  pendingLink: null,  // UI3 关系速建气泡：{source,target,apiName,cardinality,roleA,roleB,sourceProperty,targetProperty}
  linkProps: {},      // 属性到属性映射（会话内）：apiName → {sourceProperty,targetProperty}；后端 LinkType 暂不存外键属性，refreshAll 后据此重挂
  hosts: new Set(),   // 各区 host（含 __view）
  el: null,           // <cmx-ontology-graph> 实例
};

const { apiJson: _sharedApiJson } = globalThis.__cmxDataComp // 共享 fetch 封装（cmx-data-comp/lib/cmx-page-helpers.js）；经 CFG 转发保留组件壳 configure() 契约
const { deepClone } = globalThis.__cmxDataComp // 共享深拷贝（cmx-data-comp/lib/cmx-deep-clone.js；审查 B-04）
async function apiJson (url, options = {}) { return _sharedApiJson(url, options, CFG) }
const { escHtml: esc } = globalThis.__cmxDataComp // 共享转义（cmx-data-comp/lib/cmx-page-helpers.js；最严格五字符集合，文本/属性上下文皆安全）

// ── 组件加载（一次性，幂等）──
let _componentPromise = null;
function ensureComponent() {
  if (typeof customElements !== 'undefined' && customElements.get('cmx-ontology-graph')) return Promise.resolve();
  if (_componentPromise) return _componentPromise;
  _componentPromise = (async () => {
    const src = await apiJson('/api/native-pages/portal.onto.graph-component');
    const code = src && src.source ? src.source : '';
    const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }));
    try { await import(url); } finally { setTimeout(() => URL.revokeObjectURL(url), 5000); }
  })();
  return _componentPromise;
}

// ── 数据层：manifest → 组件 spec（nodes/edges）──
async function loadAll() {
  const m = await apiJson(API + '/manifest');
  state.manifest = m;
  // 对象类型节点：拉完整定义得属性（清单不含属性体）。
  const nodes = [];
  for (const ot of (m.objectTypes || [])) {
    let full = null;
    try { full = await apiJson(API + '/object-types/' + encodeURIComponent(ot.apiName)); } catch { /* */ }
    nodes.push({
      id: ot.apiName, kind: 'object',
      displayName: ot.displayName, status: ot.status,
      properties: (full && full.properties || []).map(p => ({
        apiName: p.apiName, baseType: p.baseType,
        isPrimaryKey: full && p.apiName === full.primaryKey,
        isTitle: full && p.apiName === full.titleProperty,
        required: p.required, isIndexed: p.isIndexed, semanticType: p.semanticType,
      })),
      implements: (full && full.implements) || [],
    });
  }
  for (const iface of (m.interfaces || [])) nodes.push({ id: iface.apiName, kind: 'interface', displayName: iface.displayName });
  const edges = (m.linkTypes || []).map(l => ({
    apiName: l.apiName, source: l.objectTypeA, target: l.objectTypeB,
    displayName: l.displayName, cardinality: l.cardinality,
    ...(state.linkProps[l.apiName] || {}),
  }));
  state.spec = { name: '本体', nodes, edges };
  // 保留手动布局与手动布线（保存/刷新会重建组件；从存活组件取回 _layout/_edgeRoutes 注入，避免重置位置）。
  const live = (state.el && typeof state.el.getSpec === 'function') ? state.el.getSpec() : state.spec;
  if (live && live._layout) {
    const ids = new Set(nodes.map(n => n.id)); const L = {};
    for (const k in live._layout) if (ids.has(k)) L[k] = live._layout[k];
    if (Object.keys(L).length) state.spec._layout = L;
  }
  if (live && live._edgeRoutes) {
    const as = new Set(edges.map(e => e.apiName)); const R = {};
    for (const k in live._edgeRoutes) if (as.has(k)) R[k] = live._edgeRoutes[k];
    if (Object.keys(R).length) state.spec._edgeRoutes = R;
  }
  try { state.versions = await apiJson(API + '/versions') || []; } catch { state.versions = []; }
  // 共享属性目录（含 baseType/semanticType）：UI2「引用共享属性」用。清单只给 apiName/displayName，逐一拉详情。
  try {
    const list = m.sharedProperties || [];
    const details = await Promise.all(list.map(sp => apiJson(API + '/shared-properties/' + encodeURIComponent(sp.apiName)).catch(() => null)));
    state.shared = details.filter(Boolean);
  } catch { state.shared = []; }
  state.loaded = true;
}

async function saveObjectTypeFromDetail(d) {
  await apiJson(API + '/object-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(d) });
}
async function publish(summary) {
  return apiJson(API + '/publish', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ summary: summary || '' }) });
}

// ── 四区渲染 ──
let _loadPromise = null;
function ensureLoaded() {
  if (state.loaded) return Promise.resolve();
  if (_loadPromise) return _loadPromise;
  _loadPromise = (async () => {
    await ensureComponent().catch(() => {});
    try { await loadAll(); } catch (e) { flashAll('加载失败：' + e.message, true); }
  })();
  return _loadPromise;
}
function mount(ctx, view) {
  const host = ctx.host; state.hosts.add(host); host.__view = view;
  const render = () => { const root = hostRoot(host); if (!root || root.isConnected === false) return; root.innerHTML = `<style>${css()}</style>${viewHtml(view)}`; bind(root, view, host); if (view === 'content') mountComponent(host); };
  requestAnimationFrame(async () => {
    render();
    // 单次共享加载（多区并发挂载只 load 一次）；无论哪个区先到，load 完成后各区各自最终渲染。
    await ensureLoaded();
    render();
  });
  return `<style>${css()}</style>${viewHtml(view)}`;
}
function refreshAll() { for (const v of ['model', 'explorer', 'content', 'property']) refresh(v); }
function refresh(view) {
  for (const host of state.hosts) {
    if (host.__view !== view) continue;
    const root = hostRoot(host); if (!root || root.isConnected === false) continue;
    root.innerHTML = `<style>${css()}</style>${viewHtml(view)}`; bind(root, view, host);
    if (view === 'content') mountComponent(host);
  }
}
function hostRoot(host) { return host && (host.shadowRoot || host); }
function viewHtml(view) {
  if (!state.loaded) return `<div class="o"><div class="ph">加载中…</div></div>`;
  if (view === 'model') return modelHtml();
  if (view === 'explorer') return explorerHtml();
  if (view === 'property') return propertyHtml();
  return contentHtml();
}

// ── model 区：本体元信息 + 统计 + 发布 ──
function modelHtml() {
  const m = state.manifest || {};
  const ver = state.versions[0];
  const counts = [
    ['对象类型', (m.objectTypes || []).length], ['关系', (m.linkTypes || []).length],
    ['接口', (m.interfaces || []).length], ['共享属性', (m.sharedProperties || []).length],
    ['动作', (m.actionTypes || []).length], ['函数', (m.functions || []).length],
  ];
  return `<div class="o o-model">
    <div class="o-mtitle">🕸 本体设计工作台</div>
    <div class="o-mtiles">${counts.map(c => `<div class="o-tile"><b>${c[1]}</b><span>${c[0]}</span></div>`).join('')}</div>
    <div class="o-mver">${ver ? `已发布 v${ver.version} · <code>${esc(ver.rev)}</code>` : '<span class="o-draft">草稿 · 未发布</span>'}</div>
    <button class="o-btn ok" data-act="publish">📦 发布本体</button>
  </div>`;
}

// ── explorer 区：七类元素分组树 ──
function explorerHtml() {
  const m = state.manifest || {};
  const grp = (title, icon, items, kind) => {
    const rows = (items || []).map(it => {
      const on = state.sel && state.sel.id === it.apiName;
      return `<li class="o-erow ${on ? 'sel' : ''}" data-sel-kind="${kind}" data-sel-id="${esc(it.apiName)}"><span class="o-ename">${esc(it.displayName || it.apiName)}</span><code>${esc(it.apiName)}</code></li>`;
    }).join('') || `<li class="o-empty2">—</li>`;
    return `<div class="o-grp"><div class="o-ghd">${icon} ${title} <span class="o-gn">${(items || []).length}</span></div><ul class="o-elist">${rows}</ul></div>`;
  };
  return `<div class="o o-explorer">
    <div class="o-search"><input class="o-inp" placeholder="🔍 查找类型…" data-role="search"/></div>
    ${grp('对象类型', '📦', m.objectTypes, 'object')}
    ${grp('关系类型', '🔗', m.linkTypes, 'link')}
    ${grp('接口', '◈', m.interfaces, 'interface')}
    ${grp('共享属性', '⊞', m.sharedProperties, 'shared')}
    ${grp('动作类型', '⚡', m.actionTypes, 'action')}
    ${grp('函数', 'ƒ', m.functions, 'function')}
    <div class="o-newbar"><button class="o-btn xs" data-act="new-object">+ 对象类型</button><button class="o-btn xs" data-act="new-interface">+ 接口</button><button class="o-btn xs" data-act="new-action">+ 动作</button><button class="o-btn xs" data-act="new-from-template">+ 从模板</button><button class="o-btn xs" data-act="new-function">+ 函数</button></div>
  </div>`;
}

// ── content 区：工具栏 + 组件画布 + UI3 关系速建气泡 ──
function contentHtml() {
  return `<div class="o o-content">
    <div class="o-toolbar">
      <b class="o-title">本体图</b>
      <span class="o-dirty ${state.dirty ? 'on' : ''}">${state.dirty ? '● 未保存' : '已同步'}</span>
      <span class="o-sp"></span>
      <button class="o-btn xs" data-act="new-object">+ 对象类型</button>
      <button class="o-btn xs" data-act="auto-layout">重排</button>
    </div>
    <div class="o-hint">拖节点移位 · 从卡片右缘小圆拉线建关系（拖回自身=自关联层级）· 点节点在右侧编辑 · 点边选中</div>
    <div class="o-canvaswrap" data-graph-host></div>
    ${linkBubbleHtml()}
  </div>`;
}
// UI3 关系速建气泡（画布内联浮层；拉线落点后弹出，替代 prompt）。
function linkBubbleHtml() {
  const pl = state.pendingLink; if (!pl) return '';
  const arrow = pl.self ? `${esc(pl.source)} ⤴ 自关联` : `${esc(pl.source)} ▶ ${esc(pl.target)}`;
  return `<div class="o-linkbubble" data-role="link-bubble">
    <div class="o-lbhd">新建关系 <b>${arrow}</b></div>
    ${(pl.sourceProperty || pl.targetProperty) ? `<div class="o-lbhd" style="color:var(--o-accent)">属性连接：<b>${esc(pl.sourceProperty || '?')}</b> → <b>${esc(pl.targetProperty || '(主键)')}</b></div>` : ''}
    <label>apiName</label><input class="o-inp" data-lf="apiName" value="${esc(pl.apiName)}"/>
    <label>显示名</label><input class="o-inp" data-lf="displayName" placeholder="如 客户下单" value="${esc(pl.displayName || '')}"/>
    <div class="o-row2">
      <div><label>基数</label><select class="o-inp" data-lf="cardinality">${CARDS.map(c => `<option value="${c}" ${c === pl.cardinality ? 'selected' : ''}>${CARD_LABEL[c]}</option>`).join('')}</select></div>
      <div><label>A→B 角色</label><input class="o-inp" data-lf="roleA" value="${esc(pl.roleA || '')}"/></div>
    </div>
    <label>B→A 角色</label><input class="o-inp" data-lf="roleB" value="${esc(pl.roleB || '')}"/>
    <div class="o-lbfoot"><button class="o-btn xs" data-act="cancel-link">取消</button><button class="o-btn xs primary" data-act="confirm-link">创建关系</button></div>
  </div>`;
}
function mountComponent(host) {
  const root = hostRoot(host);
  const slot = root && root.querySelector('[data-graph-host]');
  if (!slot) return;
  if (typeof customElements === 'undefined' || !customElements.get('cmx-ontology-graph')) {
    slot.innerHTML = '<div class="ph">本体图组件加载中…</div>';
    ensureComponent().then(() => mountComponent(host)).catch(() => { slot.innerHTML = '<div class="ph">组件加载失败</div>'; });
    return;
  }
  let el = slot.querySelector('cmx-ontology-graph');
  if (!el) {
    el = document.createElement('cmx-ontology-graph');
    el.style.cssText = 'display:block;width:100%;height:100%';
    slot.innerHTML = ''; slot.appendChild(el);
    wireComponent(el);
  }
  state.el = el;
  if (typeof el.setSpec === 'function' && state.spec) el.setSpec(state.spec);
}
function wireComponent(el) {
  el.addEventListener('type-select', (e) => {
    const n = e.detail.node;
    if (!n) return;
    selectElement(n.kind === 'interface' ? 'interface' : 'object', n.id);
  });
  el.addEventListener('edge-select', (e) => { if (e.detail.apiName) selectElement('link', e.detail.apiName); });
  el.addEventListener('spec-change', (e) => { state.spec = e.detail.spec; });
  el.addEventListener('link-add', (e) => openLinkBubble(e.detail.source, e.detail.target, e.detail.sourceProperty, e.detail.targetProperty));
  el.addEventListener('connect-rejected', (e) => flashAll(e.detail.reason || '连线被拒', true));
}

// ── property 区：选中元素 Inspector ──
function propertyHtml() {
  if (!state.sel) return `<div class="o o-prop"><div class="ph">在画布或左侧选中一个类型以编辑</div></div>`;
  if (state.sel.kind === 'object') return objectInspectorHtml();
  if (state.sel.kind === 'link') return linkInspectorHtml();
  if (state.sel.kind === 'action') return actionInspectorHtml();
  if (state.sel.kind === 'function') return functionInspectorHtml();
  return `<div class="o o-prop"><div class="o-phd">${esc(state.sel.id)}</div><div class="o-pmuted">该类型的 Inspector 待补（O1 已支持后端 CRUD）。</div></div>`;
}
function objectInspectorHtml() {
  const d = state.detail || {};
  const props = d.properties || [];
  // 即时结构校验：重名集 + 主键计数（用于就地红标）。
  const seen = {};
  props.forEach(p => { seen[p.apiName] = (seen[p.apiName] || 0) + 1; });
  const pkCount = props.filter(p => p.apiName === d.primaryKey).length;
  const issues = collectPropIssues(props, d);

  const rows = props.map((p, i) => {
    const dup = seen[p.apiName] > 1;
    const bad = !validApiName(p.apiName) || dup;
    const isRef = !!p.sharedProperty;
    const composite = COMPOSITE_TYPES.includes(p.baseType);
    const selected = state.selRows.has(i);
    const nameCell = isRef
      ? `<span class="o-refname" title="引用共享属性 ${esc(p.sharedProperty)}">⊞ ${esc(p.apiName)}</span>`
      : `<input class="o-cin ${bad ? 'bad' : ''}" data-pf="apiName" data-pi="${i}" value="${esc(p.apiName)}" title="${dup ? '重名' : (!validApiName(p.apiName) ? '非法 apiName（字母/下划线开头）' : '')}"/>`;
    const typeCell = isRef
      ? `<span class="o-rty">${esc(p.baseType)}</span>`
      : `<select class="o-csel" data-pf="baseType" data-pi="${i}">${BASE_TYPES.map(t => `<option ${t === p.baseType ? 'selected' : ''}>${t}</option>`).join('')}</select>`;
    const expandBtn = composite ? `<button class="o-xbtn" data-act="toggle-sub" data-pi="${i}" title="子属性">${p.__open ? '▾' : '▸'}</button>` : '';
    const mainRow = `<tr class="o-prow ${selected ? 'rsel' : ''}" data-pi="${i}" draggable="true">
      <td class="o-c handle" title="拖拽排序" data-drag="${i}">⣿</td>
      <td class="o-c"><input type="checkbox" class="o-rowsel" data-pi="${i}" ${selected ? 'checked' : ''}/></td>
      <td>${expandBtn}${nameCell}</td>
      <td>${typeCell}</td>
      <td class="o-c"><input type="radio" name="pk" data-pf="pk" data-pi="${i}" ${p.apiName === d.primaryKey ? 'checked' : ''}/></td>
      <td class="o-c"><input type="checkbox" data-pf="required" data-pi="${i}" ${p.required ? 'checked' : ''}/></td>
      <td class="o-c"><input type="checkbox" data-pf="isIndexed" data-pi="${i}" ${p.isIndexed ? 'checked' : ''}/></td>
      <td><select class="o-csel sem" data-pf="semanticType" data-pi="${i}" ${isRef ? 'disabled' : ''}>${SEMANTIC_TYPES.map(s => `<option value="${s}" ${s === (p.semanticType || '') ? 'selected' : ''}>${SEMANTIC_LABEL[s]}</option>`).join('')}</select></td>
      <td><button class="o-btn xs danger" data-act="del-prop" data-pi="${i}">✕</button></td>
    </tr>`;
    // 子属性展开行（struct/array 的嵌套属性；纯前端存 p.children）。
    const subRow = (composite && p.__open) ? `<tr class="o-subrow"><td colspan="9">${subPropsHtml(p, i)}</td></tr>` : '';
    return mainRow + subRow;
  }).join('');

  const batchBar = `<div data-role="batch-anchor">${batchBarHtml()}</div>`;
  const issuesBar = issues.length
    ? `<div class="o-issues">⚠ ${issues.length} 处问题：${issues.map(x => esc(x)).join('；')}</div>`
    : `<div class="o-okbar">✓ 结构合法</div>`;
  const sharedRefBar = state.shared.length
    ? `<div class="o-refbar">引用共享属性：<select class="o-inp o-refsel" data-role="ref-shared"><option value="">选一个标准字段…</option>${state.shared.map(s => `<option value="${esc(s.apiName)}">${esc(s.displayName || s.apiName)} · ${esc(s.baseType)}${s.semanticType ? ' · ' + esc(SEMANTIC_LABEL[s.semanticType] || s.semanticType) : ''}</option>`).join('')}</select><button class="o-btn xs" data-act="add-shared">+ 引入</button></div>`
    : '';

  return `<div class="o o-prop">
    <div class="o-phd">📦 ${esc(d.displayName || d.apiName)} <code>${esc(d.apiName)}</code></div>
    <label>显示名</label><input class="o-inp" data-df="displayName" value="${esc(d.displayName || '')}"/>
    <div class="o-row2">
      <div><label>标题属性</label><select class="o-inp" data-df="titleProperty"><option value="">—</option>${props.map(p => `<option ${p.apiName === d.titleProperty ? 'selected' : ''}>${esc(p.apiName)}</option>`).join('')}</select></div>
      <div><label>状态</label><select class="o-inp" data-df="status">${STATUS.map(s => `<option value="${s}" ${s === d.status ? 'selected' : ''}>${STATUS_LABEL[s]}</option>`).join('')}</select></div>
    </div>
    <div class="o-phd2">属性 <span class="o-pn">${props.length}</span> <span class="o-sp"></span><button class="o-btn xs" data-act="add-prop">+ 加属性</button></div>
    ${sharedRefBar}
    ${batchBar}
    <table class="o-ptable"><thead><tr><th></th><th></th><th>apiName</th><th>类型</th><th title="主键">PK</th><th title="必填">*</th><th title="索引">⚡</th><th>语义</th><th></th></tr></thead>
      <tbody data-role="prop-tbody">${rows || '<tr><td colspan=9 class="o-empty2">暂无属性，点「+ 加属性」或引用共享属性</td></tr>'}</tbody></table>
    ${issuesBar}
    <div class="o-pactions"><button class="o-btn primary" data-act="save-object" ${issues.length ? 'disabled title="修正问题后可保存"' : ''}>保存对象类型</button><button class="o-btn danger" data-act="del-object" data-id="${esc(d.apiName)}">删除对象类型</button></div>
  </div>`;
}

// 子属性（struct/array 的嵌套属性）——纯前端存 p.children，随对象类型 constraints 落库（O1 保留 constraints jsonb）。
function subPropsHtml(p, pi) {
  const kids = p.children || [];
  const rows = kids.map((c, ci) => `<div class="o-subrowl" data-pi="${pi}" data-ci="${ci}">
    <input class="o-cin xs" data-sf="apiName" value="${esc(c.apiName)}" placeholder="子属性名"/>
    <select class="o-csel xs" data-sf="baseType">${BASE_TYPES.map(t => `<option ${t === c.baseType ? 'selected' : ''}>${t}</option>`).join('')}</select>
    <button class="o-btn xs danger" data-act="del-sub" data-pi="${pi}" data-ci="${ci}">✕</button>
  </div>`).join('');
  return `<div class="o-subwrap"><div class="o-sublabel">${p.baseType === 'array' ? '元素结构' : '子属性'}</div>${rows}<button class="o-btn xs" data-act="add-sub" data-pi="${pi}">+ 子属性</button></div>`;
}

// 批量操作条 HTML（空选时不显）。
function batchBarHtml() {
  if (!state.selRows.size) return '';
  return `<div class="o-batch">已选 ${state.selRows.size} 项 <button class="o-btn xs" data-act="batch-required">批量必填</button><button class="o-btn xs" data-act="batch-unrequired">批量取消必填</button><button class="o-btn xs danger" data-act="batch-del">批量删除</button><button class="o-btn xs" data-act="batch-clear">清除选择</button></div>`;
}
// 原地更新批量条（不触发全表重渲，避免打断连续勾选）。
function updateBatchBar(root) {
  const anchor = root.querySelector('[data-role="batch-anchor"]');
  if (anchor) anchor.innerHTML = batchBarHtml();
}

// 即时校验：apiName 非法/重名、无主键、主键指向不存在属性。
function collectPropIssues(props, d) {
  const out = [];
  const seen = {};
  props.forEach(p => {
    if (!validApiName(p.apiName)) out.push(`「${p.apiName || '(空)'}」apiName 非法`);
    seen[p.apiName] = (seen[p.apiName] || 0) + 1;
  });
  Object.keys(seen).forEach(k => { if (seen[k] > 1) out.push(`「${k}」重名 ${seen[k]} 次`); });
  if (props.length && !d.primaryKey) out.push('未指定主键');
  if (d.primaryKey && !props.some(p => p.apiName === d.primaryKey)) out.push(`主键「${d.primaryKey}」不在属性中`);
  return out;
}
function linkInspectorHtml() {
  // UI3 可编辑：优先用完整 detail（含 roleA/roleB）；detail 未到则回退清单元数据。
  const meta = (state.manifest.linkTypes || []).find(x => x.apiName === state.sel.id) || {};
  const l = (state.detail && state.detail.apiName === state.sel.id) ? state.detail : meta;
  const self = l.objectTypeA === l.objectTypeB;
  return `<div class="o o-prop">
    <div class="o-phd">🔗 ${esc(l.displayName || l.apiName)} <code>${esc(l.apiName)}</code></div>
    <div class="o-lendpoints">${esc(l.objectTypeA)} <span class="o-larrow">${self ? '⤴' : '▶'}</span> ${esc(l.objectTypeB)}${self ? ' <span class="o-lself">自关联层级</span>' : ''}</div>
    <label>显示名</label><input class="o-inp" data-lf="displayName" value="${esc(l.displayName || '')}"/>
    <div class="o-row2">
      <div><label>基数</label><select class="o-inp" data-lf="cardinality">${CARDS.map(c => `<option value="${c}" ${c === (l.cardinality || 'oneToMany') ? 'selected' : ''}>${CARD_LABEL[c]}</option>`).join('')}</select></div>
      <div><label>状态</label><select class="o-inp" data-lf="status">${STATUS.map(s => `<option value="${s}" ${s === (l.status || 'experimental') ? 'selected' : ''}>${STATUS_LABEL[s]}</option>`).join('')}</select></div>
    </div>
    <div class="o-row2">
      <div><label>A→B 角色</label><input class="o-inp" data-lf="roleA" value="${esc(l.roleA || '')}"/></div>
      <div><label>B→A 角色</label><input class="o-inp" data-lf="roleB" value="${esc(l.roleB || '')}"/></div>
    </div>
    <div class="o-pactions"><button class="o-btn primary" data-act="save-link" data-id="${esc(l.apiName)}">保存关系</button><button class="o-btn danger" data-act="del-link" data-id="${esc(l.apiName)}">删除关系</button></div>
  </div>`;
}
// UI3 关系 Inspector 保存（upsert；apiName/两端不变，改 displayName/基数/角色/状态）。
async function doSaveLink(root, id) {
  const l = (state.detail && state.detail.apiName === id) ? state.detail : ((state.manifest.linkTypes || []).find(x => x.apiName === id) || {});
  const q = (sel) => { const el = root.querySelector(sel); return el ? el.value.trim() : undefined; };
  const body = {
    apiName: id, objectTypeA: l.objectTypeA, objectTypeB: l.objectTypeB,
    cardinality: q('[data-lf="cardinality"]') || l.cardinality || 'oneToMany',
    displayName: q('[data-lf="displayName"]') || '',
    roleA: q('[data-lf="roleA"]') || '', roleB: q('[data-lf="roleB"]') || '',
    status: q('[data-lf="status"]') || l.status || 'experimental',
  };
  await saveLink(body);
}

// ══════════════ UI5 动能层：函数 Inspector ══════════════
function functionInspectorHtml() {
  const d = state.detail || {};
  const inputs = d.inputs || [];
  const rt = d.runtime || 'feel';
  const bodyLabel = rt === 'feel' ? 'FEEL 表达式' : rt === 'rhai' ? 'Rhai 脚本' : rt === 'wasm' ? 'WASM 模块' : '内置实现引用';
  const inRows = inputs.map((p, i) => `<div class="o-fnrow" data-i="${i}">
    <input class="o-cin xs" data-inf="name" value="${esc(p.name || '')}" placeholder="参数名"/>
    <select class="o-csel xs" data-inf="type">${BASE_TYPES.concat(['object', 'objectSet']).map(t => `<option ${t === (p.type || 'string') ? 'selected' : ''}>${t}</option>`).join('')}</select>
    <button class="o-btn xs danger" data-act="fn-del-input" data-i="${i}">✕</button>
  </div>`).join('');
  const outType = (d.output && d.output.type) || 'double';
  return `<div class="o o-prop">
    <div class="o-phd">ƒ ${esc(d.displayName || d.apiName)} <code>${esc(d.apiName)}</code></div>
    <label>显示名</label><input class="o-inp" data-ff="displayName" value="${esc(d.displayName || '')}"/>
    <div class="o-row2">
      <div><label>运行时</label><select class="o-inp" data-ff="runtime">${FN_RUNTIMES.map(r => `<option value="${r}" ${r === rt ? 'selected' : ''}>${FN_RT_LABEL[r]}</option>`).join('')}</select></div>
      <div><label>用途</label><select class="o-inp" data-ff="kind">${FN_KINDS.map(k => `<option value="${k}" ${k === (d.kind || 'query') ? 'selected' : ''}>${FN_KIND_LABEL[k]}</option>`).join('')}</select></div>
    </div>
    <div class="o-phd2">输入参数 <span class="o-pn">${inputs.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="fn-add-input">+ 参数</button></div>
    <div class="o-fnlist">${inRows || '<div class="o-empty2">无参数</div>'}</div>
    <div class="o-row2"><div><label>返回类型</label><select class="o-inp" data-ff="outputType">${BASE_TYPES.concat(['object', 'objectSet']).map(t => `<option ${t === outType ? 'selected' : ''}>${t}</option>`).join('')}</select></div><div></div></div>
    <label>${bodyLabel} <span class="o-feelhint">吃对象/对象集：可读属性、Search-Around</span></label>
    <textarea class="o-code" data-ff="body" rows="5" spellcheck="false" placeholder="${rt === 'feel' ? 'if amount > 1000 then 0.8 else 0.2' : '// 逻辑'}">${esc(d.body || '')}</textarea>
    <label>描述</label><input class="o-inp" data-ff="description" value="${esc(d.description || '')}"/>
    <div class="o-pactions"><button class="o-btn primary" data-act="save-function">保存函数</button><button class="o-btn" data-act="eval-function" data-id="${esc(d.apiName)}">▶ 求值试算</button><button class="o-btn danger" data-act="del-function" data-id="${esc(d.apiName)}">删除函数</button></div>
    <div class="o-runbox" data-role="fn-result">${state.fnResult || ''}</div>
  </div>`;
}
// DOM → state.detail（结构变更/保存前同步；保光标不重渲的字段在此收集）。
function collectFn(root) {
  const d = state.detail; if (!d) return;
  const g = (s) => { const el = root.querySelector(s); return el ? el.value : undefined; };
  const dn = g('[data-ff="displayName"]'); if (dn !== undefined) d.displayName = dn;
  const rt = g('[data-ff="runtime"]'); if (rt !== undefined) d.runtime = rt;
  const kd = g('[data-ff="kind"]'); if (kd !== undefined) d.kind = kd;
  const ot = g('[data-ff="outputType"]'); if (ot !== undefined) d.output = { type: ot };
  const bd = g('[data-ff="body"]'); if (bd !== undefined) d.body = bd;
  const ds = g('[data-ff="description"]'); if (ds !== undefined) d.description = ds;
  const rows = root.querySelectorAll('.o-fnrow');
  const inputs = [];
  rows.forEach(r => { const n = r.querySelector('[data-inf="name"]'); const t = r.querySelector('[data-inf="type"]'); if (n && n.value.trim()) inputs.push({ name: n.value.trim(), type: t ? t.value : 'string' }); });
  d.inputs = inputs;
}
function fnAddInput(root) { collectFn(root); state.detail.inputs = state.detail.inputs || []; state.detail.inputs.push({ name: 'arg' + (state.detail.inputs.length + 1), type: 'string' }); refresh('property'); }
function fnDelInput(root, i) { collectFn(root); state.detail.inputs.splice(i, 1); refresh('property'); }
async function doSaveFunction(root) {
  collectFn(root);
  try { await apiJson(API + '/functions', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(state.detail) }); flashAll('已保存函数 ' + state.detail.apiName); await loadAll(); refreshAll(); selectElement('function', state.detail.apiName, true); }
  catch (e) { flashAll('保存函数失败：' + e.message, true); }
}
async function doDelFunction(id) {
  const c = await openDialog({ title: '删除函数 ' + id, severity: 'warn', body: `<p class="o-dlgmuted">若有动作类型以此函数背书（functionBacking）或派生属性引用，将失效。</p>`, buttons: [{ label: '取消', id: '__cancel' }, { label: '删除', id: 'delete', kind: 'danger' }] });
  if (c !== 'delete') return;
  try { await apiJson(API + '/functions/' + encodeURIComponent(id), { method: 'DELETE' }); flashAll('已删除函数 ' + id); state.sel = null; state.detail = null; await loadAll(); refreshAll(); }
  catch (e) { flashAll('删除失败：' + e.message, true); }
}

// ── O8：函数求值试算（对接 O5 /functions/{n}/evaluate）──
async function doEvalFunction(root, id) {
  collectFn(root);
  const d = state.detail || {};
  const inputs = d.inputs || [];
  // 每输入一个带 data-k 的输入框（data-k 前缀标类型：s|/o|/S|），openDialog 快照其值。
  const rows = inputs.map(p => `<div style="margin:6px 0">
    <label style="font-size:11.5px;color:var(--o-muted,#94a3b8)">${esc(p.name)} <code>${esc(p.type)}</code></label>
    <input class="o-inp" data-k="in:${esc(p.type)}:${esc(p.name)}" placeholder="${p.type === 'object' ? '{&quot;objectType&quot;:&quot;X&quot;,&quot;pk&quot;:&quot;1&quot;}' : p.type === 'objectSet' ? '{&quot;op&quot;:&quot;base&quot;,&quot;objectType&quot;:&quot;X&quot;}' : '值（数字/字符串）'}"/>
  </div>`).join('');
  const isAgg = d.kind === 'aggregation';
  const body = isAgg
    ? `<p class="o-dlgmuted">聚合函数：填对象集代数 + 聚合规格。</p>
       <label>objectSet</label><input class="o-inp" data-k="agg:objectSet" placeholder='{"op":"base","objectType":"X"}'/>
       <label>aggregation</label><input class="o-inp" data-k="agg:aggregation" placeholder='{"kind":"count"}'/>`
    : (rows || '<p class="o-dlgmuted">此函数无输入参数。</p>');
  const c = await openDialog({ title: '求值试算 · ' + id, severity: 'info', body, buttons: [{ label: '取消', id: '__cancel' }, { label: '求值', id: 'run', kind: 'primary' }] });
  if (c !== 'run') return;
  const V = _lastDialogValues || {};
  const payload = { args: {}, objects: {}, objectSets: {} };
  for (const k in V) {
    const v = (V[k] || '').trim(); if (!v) continue;
    if (k.startsWith('agg:')) { try { payload[k.slice(4)] = JSON.parse(v); } catch (e) {} continue; }
    const m = k.match(/^in:([^:]+):(.+)$/); if (!m) continue;
    const type = m[1], name = m[2];
    if (type === 'object') { try { payload.objects[name] = JSON.parse(v); } catch (e) {} }
    else if (type === 'objectSet') { try { payload.objectSets[name] = JSON.parse(v); } catch (e) {} }
    else { let pv; try { pv = JSON.parse(v); } catch (e) { pv = v; } payload.args[name] = pv; }
  }
  try {
    const r = await apiJson(API + '/functions/' + encodeURIComponent(id) + '/evaluate', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
    state.fnResult = `<div class="o-runok">✔ 结果：<b>${esc(JSON.stringify(r.result))}</b> <span class="o-runmeta">${esc(r.kind || '')}/${esc(r.runtime || '')}</span></div>`;
  } catch (e) {
    state.fnResult = `<div class="o-runerr">✘ ${esc(e.message)}</div>`;
  }
  refresh('property');
}

// ── O8：动作试算/执行（对接 O4 /action-types/{n}/dry-run|execute）──
async function doRunAction(root, id, dryRun) {
  collectAction(root);
  const d = state.detail || {};
  const params = d.parameters || [];
  const rows = params.map(p => `<div style="margin:6px 0">
    <label style="font-size:11.5px;color:var(--o-muted,#94a3b8)">${esc(p.name)}${p.required ? ' <span style="color:var(--o-err,#ef4444)">*</span>' : ''}</label>
    <input class="o-inp" data-k="p:${esc(p.name)}" placeholder="参数值"/>
  </div>`).join('');
  const c = await openDialog({ title: (dryRun ? '试算' : '执行') + ' · ' + id, severity: dryRun ? 'info' : 'warn',
    body: (dryRun ? '' : '<p class="o-dlgwarn">执行将真实写回对象数据并入 Outbox（副作用）。</p>') + (rows || '<p class="o-dlgmuted">此动作无参数。</p>'),
    buttons: [{ label: '取消', id: '__cancel' }, { label: dryRun ? '试算' : '确认执行', id: 'run', kind: dryRun ? 'primary' : 'ok' }] });
  if (c !== 'run') return;
  const V = _lastDialogValues || {};
  const args = {};
  for (const k in V) { if (!k.startsWith('p:')) continue; const v = (V[k] || '').trim(); if (!v) continue; const n = k.slice(2); let pv; try { pv = JSON.parse(v); } catch (e) { pv = v; } args[n] = pv; }
  const url = API + '/action-types/' + encodeURIComponent(id) + (dryRun ? '/dry-run' : '/execute');
  try {
    const r = await apiJson(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ params: args, actor: 'designer' }) });
    const edits = (r.edits || []).length, fx = r.effects || 0;
    state.acResult = `<div class="o-runok">✔ ${dryRun ? '试算通过' : '已执行'} · 编辑 ${edits} 条${fx ? ' · 副作用 ' + fx + ' 条' : ''} <span class="o-runmeta">${esc(r.status || '')}${r.logId ? ' · log#' + r.logId : ''}</span></div>`;
  } catch (e) {
    state.acResult = `<div class="o-runerr">✘ ${esc(e.message)}</div>`;
  }
  refresh('property');
}

// ══════════════ UI5 动能层：动作类型 Inspector ══════════════
function actionInspectorHtml() {
  const d = state.detail || {};
  if (!state.flowDefsLoaded) loadFlowDefs(); // 惰性拉已发布流程（「起流程」副作用选择器数据源）
  if (!state.reportDefsLoaded) loadReportDefs(); // 惰性拉报表（「生成报表」副作用选择器数据源）
  const params = d.parameters || [];
  const logic = d.logic || [];
  const vals = d.validations || [];
  const fx = d.sideEffects || [];
  const objTypes = (state.manifest.objectTypes || []).map(o => o.apiName);
  const fnNames = (state.manifest.functions || []).map(f => f.apiName);
  const paramRows = params.map((p, i) => `<div class="o-fnrow" data-i="${i}">
    <input class="o-cin xs" data-paf="name" value="${esc(p.name || '')}" placeholder="参数名"/>
    <select class="o-csel xs" data-paf="type">${['object', 'objectSet', 'string', 'long', 'double', 'boolean'].map(t => `<option ${t === (p.type || 'object') ? 'selected' : ''}>${t}</option>`).join('')}</select>
    <select class="o-csel xs" data-paf="objectType"><option value="">—</option>${objTypes.map(o => `<option ${o === p.objectType ? 'selected' : ''}>${esc(o)}</option>`).join('')}</select>
    <button class="o-btn xs danger" data-act="ac-del-param" data-i="${i}">✕</button>
  </div>`).join('');
  const logicRows = logic.map((r, i) => `<div class="o-fnrow" data-i="${i}">
    <select class="o-csel xs" data-lof="op">${EDIT_OPS.map(o => `<option value="${o}" ${o === (r.op || 'modifyObject') ? 'selected' : ''}>${EDIT_OP_LABEL[o]}</option>`).join('')}</select>
    <input class="o-cin xs" data-lof="target" value="${esc(r.target || '')}" placeholder="目标（参数名/对象）"/>
    <button class="o-btn xs danger" data-act="ac-del-logic" data-i="${i}">✕</button>
  </div>`).join('');
  const valRows = vals.map((v, i) => `<div class="o-fnrow2" data-i="${i}">
    <input class="o-code o-codein" data-vaf="expression" value="${esc(v.expression || '')}" placeholder="FEEL 校验表达式，如 amount > 0"/>
    <input class="o-cin xs" data-vaf="message" value="${esc(v.message || '')}" placeholder="错误提示"/>
    <button class="o-btn xs danger" data-act="ac-del-val" data-i="${i}">✕</button>
  </div>`).join('');
  const RICH_KINDS = ['startBusinessProcess', 'computeReport', 'webhook', 'notification'];
  const FX_TAG = { startBusinessProcess: '🔀 起流程', computeReport: '📊 生成报表', webhook: '🪝 Webhook', notification: '🔔 通知' };
  const fxRows = fx.map((s, i) => {
    const kindSel = `<select class="o-csel xs" data-sef="kind">${SIDE_KINDS.map(k => `<option value="${k}" ${k === (s.kind || 'notification') ? 'selected' : ''}>${SIDE_KIND_LABEL[k]}</option>`).join('')}</select>`;
    const del = `<button class="o-btn xs danger" data-act="ac-del-fx" data-i="${i}">✕</button>`;
    if (RICH_KINDS.includes(s.kind)) {
      return `<div class="o-fxblock" data-i="${i}"><div class="o-fnrow">${kindSel}<span class="o-fxtag">${FX_TAG[s.kind]}</span>${del}</div>${richSideEffectHtml(s, i, params)}</div>`;
    }
    return `<div class="o-fxblock" data-i="${i}"><div class="o-fnrow">${kindSel}<input class="o-cin xs" data-sef="ref" value="${esc(s.function || s.topic || '')}" placeholder="目标（函数/主题）"/>${del}</div></div>`;
  }).join('');
  return `<div class="o o-prop">
    <div class="o-phd">⚡ ${esc(d.displayName || d.apiName)} <code>${esc(d.apiName)}</code></div>
    <div class="o-row2">
      <div><label>显示名</label><input class="o-inp" data-af="displayName" value="${esc(d.displayName || '')}"/></div>
      <div><label>状态</label><select class="o-inp" data-af="status">${STATUS.map(s => `<option value="${s}" ${s === (d.status || 'experimental') ? 'selected' : ''}>${STATUS_LABEL[s]}</option>`).join('')}</select></div>
    </div>
    <div class="o-phd2">参数 <span class="o-pn">${params.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="ac-add-param">+ 参数</button></div>
    <div class="o-fnlist">${paramRows || '<div class="o-empty2">无参数</div>'}</div>
    <div class="o-phd2">编辑规则（写回） <span class="o-pn">${logic.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="ac-add-logic">+ 规则</button></div>
    <div class="o-fnlist">${logicRows || '<div class="o-empty2">无编辑</div>'}</div>
    <div class="o-phd2">提交校验 <span class="o-feelhint">FEEL</span> <span class="o-pn">${vals.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="ac-add-val">+ 校验</button></div>
    <div class="o-fnlist">${valRows || '<div class="o-empty2">无校验</div>'}</div>
    <div class="o-phd2">副作用 <span class="o-pn">${fx.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="ac-add-fx">+ 副作用</button></div>
    <div class="o-fnlist">${fxRows || '<div class="o-empty2">无副作用</div>'}</div>
    <label>函数背书（复杂逻辑走函数）</label>
    <select class="o-inp" data-af="functionBacking"><option value="">—</option>${fnNames.map(f => `<option ${f === d.functionBacking ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select>
    <div class="o-pactions"><button class="o-btn primary" data-act="save-action">保存动作</button><button class="o-btn" data-act="dryrun-action" data-id="${esc(d.apiName)}">▶ 试算</button><button class="o-btn ok" data-act="exec-action" data-id="${esc(d.apiName)}">⚡ 执行</button><button class="o-btn danger" data-act="del-action" data-id="${esc(d.apiName)}">删除动作</button></div>
    <div class="o-runbox" data-role="ac-result">${state.acResult || ''}</div>
  </div>`;
}
// 副作用可视化配置（起流程 / Webhook / 通知）——
const SE_RESERVED = ['kind', 'flowDefKey', 'businessKey', 'reportCode', 'function', 'url', 'topic', 'template', '_vars'];
// 惰性拉 flowengine 已发布流程定义（经 onto /flow/definitions 代理；flow 不可达则空，降级为自由输入）。
async function loadFlowDefs() {
  if (state.flowDefsLoaded || state._flowDefsLoading) return;
  state._flowDefsLoading = true;
  try { const r = await apiJson(API + '/flow/definitions'); state.flowDefs = (r && r.definitions) || []; }
  catch (e) { state.flowDefs = []; }
  state.flowDefsLoaded = true; state._flowDefsLoading = false;
  refresh('property');
}
// 惰性拉 cmx-report 报表列表（经 onto /report/definitions 代理；report 不可达则空，降级为自由输入）。
async function loadReportDefs() {
  if (state.reportDefsLoaded || state._reportDefsLoading) return;
  state._reportDefsLoading = true;
  try { const r = await apiJson(API + '/report/definitions'); state.reportDefs = (r && r.reports) || []; }
  catch (e) { state.reportDefs = []; }
  state.reportDefsLoaded = true; state._reportDefsLoading = false;
  refresh('property');
}
// 副作用对象的内联额外字段 → 键值映射编辑模型（{name,value}[]；流程变量 / Webhook 请求体 / 通知数据）。
function seVars(s) {
  if (Array.isArray(s._vars)) return s._vars;
  s._vars = Object.keys(s).filter(k => !SE_RESERVED.includes(k))
    .map(k => ({ name: k, value: typeof s[k] === 'string' ? s[k] : JSON.stringify(s[k]) }));
  return s._vars;
}
// 从富配置块 DOM 收集键值映射行。
function collectSeVars(block) {
  const vars = [];
  block.querySelectorAll('.o-sevar').forEach(vr => { const n = vr.querySelector('[data-sev="name"]'); const val = vr.querySelector('[data-sev="value"]'); const nm = n ? n.value.trim() : ''; if (nm) vars.push({ name: nm, value: val ? val.value.trim() : '' }); });
  return vars;
}
// 富配置块：起流程（flowDefKey 选择器 + businessKey）/ Webhook（URL）/ 通知（模板），三者共用「参数→载荷」键值映射。
function richSideEffectHtml(s, i, params) {
  const kind = s.kind;
  const dlPars = (params || []).map(p => `<option value="$${esc(p.name)}"></option>`).join('');
  const vars = seVars(s);
  const MAP_LABEL = { startBusinessProcess: '流程变量映射', computeReport: '报表参数（orgCode/periodCode/version）', webhook: '请求体字段', notification: '通知数据字段' };
  const mapLabel = MAP_LABEL[kind] || '载荷字段';
  const mapEmpty = kind === 'startBusinessProcess' ? '无映射（仅传 businessKey）' : kind === 'computeReport' ? '无参数（生成报表通常需 orgCode/periodCode）' : '无字段（仅传固定载荷）';
  const nameHint = kind === 'startBusinessProcess' ? '流程变量名' : kind === 'computeReport' ? '参数名（orgCode…）' : '字段名';
  const varRows = vars.map((v, j) => `<div class="o-fnrow o-sevar" data-i="${i}" data-j="${j}">
    <input class="o-cin xs" data-sev="name" value="${esc(v.name || '')}" placeholder="${nameHint}"/>
    <input class="o-cin xs" data-sev="value" list="opar-${i}" value="${esc(v.value || '')}" placeholder="$参数 或 字面量"/>
    <button class="o-btn xs danger" data-act="ac-del-sevar" data-i="${i}" data-j="${j}">✕</button>
  </div>`).join('');
  let head = '';
  if (kind === 'startBusinessProcess') {
    const defs = state.flowDefs || [];
    const dlDefs = defs.map(d => `<option value="${esc(d.key)}">${esc(d.name || d.key)}</option>`).join('');
    const note = !state.flowDefsLoaded ? '加载中…' : (defs.length ? `${defs.length} 个已发布流程` : '（flowengine 未连/无已发布流程，可直接输入键）');
    head = `<label>流程定义 flowDefKey <span class="o-hint">${note}</span></label>
      <input class="o-cin" data-sef="flowDefKey" list="ofd-${i}" value="${esc(s.flowDefKey || '')}" placeholder="选已发布流程 / 输入键（支持 $参数 插值）"/>
      <datalist id="ofd-${i}">${dlDefs}</datalist>
      <label>业务键 businessKey <span class="o-hint">可选，回查/单据关联</span></label>
      <input class="o-cin" data-sef="businessKey" list="opar-${i}" value="${esc(s.businessKey || '')}" placeholder="如 $orderId 或 PO-2024"/>`;
  } else if (kind === 'computeReport') {
    const reps = state.reportDefs || [];
    const dlReps = reps.map(r => `<option value="${esc(r.code)}">${esc(r.name || r.code)}</option>`).join('');
    const note = !state.reportDefsLoaded ? '加载中…' : (reps.length ? `${reps.length} 张报表` : '（cmx-report 未连/无报表，可直接输入编码）');
    head = `<label>报表 reportCode <span class="o-hint">${note}</span></label>
      <input class="o-cin" data-sef="reportCode" list="orp-${i}" value="${esc(s.reportCode || '')}" placeholder="选报表 / 输入编码（支持 $参数）"/>
      <datalist id="orp-${i}">${dlReps}</datalist>`;
  } else if (kind === 'webhook') {
    head = `<label>Webhook URL <span class="o-hint">POST；host 须在白名单 ONTO_WEBHOOK_ALLOW</span></label>
      <input class="o-cin" data-sef="url" list="opar-${i}" value="${esc(s.url || '')}" placeholder="http(s)://host/path（支持 $参数 插值）"/>`;
  } else { // notification
    head = `<label>通知模板 template</label>
      <input class="o-cin" data-sef="template" list="opar-${i}" value="${esc(s.template || '')}" placeholder="模板键（如 orderClosed，支持 $参数）"/>`;
  }
  return `<div class="o-fxflow">
    ${head}
    <div class="o-phd3">${mapLabel} <span class="o-pn">${vars.length}</span><span class="o-sp"></span><button class="o-btn xs" data-act="ac-add-sevar" data-i="${i}">+ 字段</button></div>
    <div class="o-fnlist">${varRows || `<div class="o-empty2">${mapEmpty}</div>`}</div>
    <datalist id="opar-${i}">${dlPars}</datalist>
  </div>`;
}
// 存前序列化：把每个富副作用（起流程/Webhook/通知）的 _vars 折成内联字段（→ 载荷/变量），并剥除 _vars 编辑态。
function serializeActionForSave(d) {
  const clone = deepClone(d);
  (clone.sideEffects || []).forEach(s => {
    if (Array.isArray(s._vars)) { s._vars.forEach(v => { if (v.name) s[v.name] = v.value; }); }
    delete s._vars;
  });
  return clone;
}
function collectAction(root) {
  const d = state.detail; if (!d) return;
  const g = (s) => { const el = root.querySelector(s); return el ? el.value : undefined; };
  const dn = g('[data-af="displayName"]'); if (dn !== undefined) d.displayName = dn;
  const st = g('[data-af="status"]'); if (st !== undefined) d.status = st;
  const fb = g('[data-af="functionBacking"]'); d.functionBacking = fb || undefined;
  // 按各 section 的独立 marker 精确收集。
  d.parameters = [];
  root.querySelectorAll('[data-paf="name"]').forEach(n => { const r = n.closest('.o-fnrow'); const t = r.querySelector('[data-paf="type"]'); const ot = r.querySelector('[data-paf="objectType"]'); if (n.value.trim()) { const p = { name: n.value.trim(), type: t ? t.value : 'object' }; if (ot && ot.value) p.objectType = ot.value; d.parameters.push(p); } });
  d.logic = [];
  root.querySelectorAll('[data-lof="op"]').forEach(op => { const r = op.closest('.o-fnrow'); const tg = r.querySelector('[data-lof="target"]'); d.logic.push({ op: op.value, target: tg ? tg.value.trim() : '' }); });
  d.validations = [];
  root.querySelectorAll('[data-vaf="expression"]').forEach(ex => { const r = ex.closest('.o-fnrow2'); const m = r.querySelector('[data-vaf="message"]'); if (ex.value.trim()) d.validations.push({ expression: ex.value.trim(), message: m ? m.value.trim() : '' }); });
  d.sideEffects = [];
  root.querySelectorAll('.o-fxblock').forEach(block => {
    const k = block.querySelector('[data-sef="kind"]'); if (!k) return;
    const kind = k.value; const s = { kind };
    if (kind === 'startBusinessProcess') {
      const fd = block.querySelector('[data-sef="flowDefKey"]'); if (fd && fd.value.trim()) s.flowDefKey = fd.value.trim();
      const bk = block.querySelector('[data-sef="businessKey"]'); if (bk && bk.value.trim()) s.businessKey = bk.value.trim();
      s._vars = collectSeVars(block);
    } else if (kind === 'computeReport') {
      const rc = block.querySelector('[data-sef="reportCode"]'); if (rc && rc.value.trim()) s.reportCode = rc.value.trim();
      s._vars = collectSeVars(block);
    } else if (kind === 'webhook') {
      const u = block.querySelector('[data-sef="url"]'); if (u && u.value.trim()) s.url = u.value.trim();
      s._vars = collectSeVars(block);
    } else if (kind === 'notification') {
      const t = block.querySelector('[data-sef="template"]'); if (t && t.value.trim()) s.template = t.value.trim();
      s._vars = collectSeVars(block);
    } else {
      const ref = block.querySelector('[data-sef="ref"]'); const v = ref ? ref.value.trim() : '';
      if (v) { const key = kind === 'callFunction' ? 'function' : 'topic'; s[key] = v; }
    }
    d.sideEffects.push(s);
  });
}
function acAdd(root, field, seed) { collectAction(root); state.detail[field] = state.detail[field] || []; state.detail[field].push(seed); refresh('property'); }
function acDel(root, field, i) { collectAction(root); (state.detail[field] || []).splice(i, 1); refresh('property'); }
async function doSaveAction(root) {
  collectAction(root);
  const payload = serializeActionForSave(state.detail);
  try { await apiJson(API + '/action-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) }); flashAll('已保存动作 ' + state.detail.apiName); await loadAll(); refreshAll(); selectElement('action', state.detail.apiName, true); }
  catch (e) { flashAll('保存动作失败：' + e.message, true); }
}
async function doDelAction(id) {
  const c = await openDialog({ title: '删除动作类型 ' + id, severity: 'warn', body: `<p class="o-dlgmuted">动作是本体的受治理写入口；删除后依赖它的应用/流程触发将失效。</p>`, buttons: [{ label: '取消', id: '__cancel' }, { label: '删除', id: 'delete', kind: 'danger' }] });
  if (c !== 'delete') return;
  try { await apiJson(API + '/action-types/' + encodeURIComponent(id), { method: 'DELETE' }); flashAll('已删除动作 ' + id); state.sel = null; state.detail = null; await loadAll(); refreshAll(); }
  catch (e) { flashAll('删除失败：' + e.message, true); }
}
async function doNewFunction() {
  const apiName = window.prompt('新函数 apiName（如 delayRisk）：', ''); if (!apiName) return;
  try { await apiJson(API + '/functions', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ apiName, displayName: apiName, runtime: 'feel', kind: 'query', inputs: [], output: { type: 'double' }, body: '' }) }); flashAll('已建函数 ' + apiName); await loadAll(); refreshAll(); selectElement('function', apiName, true); }
  catch (e) { flashAll('建函数失败：' + e.message, true); }
}
async function doNewAction() {
  const apiName = window.prompt('新动作类型 apiName（如 reassignOrder）：', ''); if (!apiName) return;
  try { await apiJson(API + '/action-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ apiName, displayName: apiName, parameters: [], logic: [], validations: [], sideEffects: [] }) }); flashAll('已建动作 ' + apiName); await loadAll(); refreshAll(); selectElement('action', apiName, true); }
  catch (e) { flashAll('建动作失败：' + e.message, true); }
}
// 从内置模板新建动作（关账联动等预置「动作 + 副作用」组合）——建后直接进编辑器（副作用富块自动呈现）。
async function doNewFromTemplate() {
  let tpls = [];
  try { const r = await apiJson(API + '/action-templates'); tpls = (r && r.templates) || []; }
  catch (e) { return flashAll('拉取模板失败：' + e.message, true); }
  if (!tpls.length) return flashAll('无可用模板', true);
  const opts = tpls.map((t, i) => `<option value="${i}">${esc(t.name)}</option>`).join('');
  const help = tpls.map(t => `<div class="o-tplrow"><b>${esc(t.name)}</b> — ${esc(t.description || '')}</div>`).join('');
  const body = `<div class="o-tplhelp">${help}</div>
    <div class="o-row2"><div><label>选择模板</label><select class="o-inp" data-k="tpl">${opts}</select></div>
    <div><label>新动作 apiName</label><input class="o-inp" data-k="apiName" placeholder="如 monthEndClose"/></div></div>`;
  const c = await openDialog({ title: '从模板新建动作', body, buttons: [{ label: '取消', id: '__cancel' }, { label: '使用模板', id: 'use', kind: 'primary' }] });
  if (c !== 'use') return;
  const idx = +((_lastDialogValues && _lastDialogValues['tpl']) || 0);
  const apiName = ((_lastDialogValues && _lastDialogValues['apiName']) || '').trim();
  if (!validApiName(apiName)) return flashAll('apiName 非法（字母/下划线开头）', true);
  const tpl = tpls[idx]; if (!tpl) return;
  const action = Object.assign({ apiName }, JSON.parse(JSON.stringify(tpl.action || {})));
  try { await apiJson(API + '/action-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(action) }); flashAll('已从模板「' + tpl.name + '」建动作 ' + apiName); await loadAll(); refreshAll(); selectElement('action', apiName, true); }
  catch (e) { flashAll('建动作失败：' + e.message, true); }
}

// ── 选中 + 详情装载 ──
let _selGen = 0;
async function selectElement(kind, id, forceReload) {
  // 已选中同一对象且 detail 在手（可能有未保存编辑）→ 不重新拉取，避免覆盖正在编辑的 state.detail。
  const same = state.sel && state.sel.kind === kind && state.sel.id === id;
  if (!same) { state.fnResult = ''; state.acResult = ''; } // 换选中元素 → 清运行结果
  const gen = ++_selGen; // 代际守卫：快速切换时，晚到的 detail 不覆盖新选中（防 stale race）。
  state.sel = { kind, id };
  const stale = () => gen !== _selGen;
  if (kind === 'object') {
    if (!same || forceReload || !state.detail || state.detail.apiName !== id) {
      state.selRows.clear();
      try {
        const d = await apiJson(API + '/object-types/' + encodeURIComponent(id));
        if (stale()) return;
        state.detail = d;
        state.detail.__origPk = state.detail.primaryKey; // UI4：记原始主键，保存时比对是否变更
        (state.detail.properties || []).forEach(p => {
          if (p.constraints && Array.isArray(p.constraints.children)) p.children = p.constraints.children;
        });
      } catch { if (!stale()) state.detail = null; }
    }
  } else if (kind === 'link') {
    // UI3 关系 Inspector 可编辑：拉完整定义（含 roleA/roleB/backing）。
    if (!same || forceReload || !state.detail || state.detail.apiName !== id) {
      try { const d = await apiJson(API + '/link-types/' + encodeURIComponent(id)); if (stale()) return; state.detail = d; } catch { if (!stale()) state.detail = null; }
    }
  } else if (kind === 'action') {
    // UI5 动作类型 Inspector 可编辑（parameters/logic/validations/sideEffects/functionBacking）。
    if (!same || forceReload || !state.detail || state.detail.apiName !== id) {
      try { const d = await apiJson(API + '/action-types/' + encodeURIComponent(id)); if (stale()) return; state.detail = d; } catch { if (!stale()) state.detail = null; }
    }
  } else if (kind === 'function') {
    // UI5 函数 Inspector 可编辑（runtime/kind/inputs/output/body）。
    if (!same || forceReload || !state.detail || state.detail.apiName !== id) {
      try { const d = await apiJson(API + '/functions/' + encodeURIComponent(id)); if (stale()) return; state.detail = d; } catch { if (!stale()) state.detail = null; }
    }
  } else { state.detail = null; state.selRows.clear(); }
  if (stale()) return; // 已被更新的选中取代 → 不刷新（由最新那次负责渲染）。
  if (state.el && typeof state.el.selectNode === 'function' && (kind === 'object' || kind === 'interface')) state.el.selectNode(id);
  refresh('explorer'); refresh('property');
}

// ── 关系速建气泡（拉线落点后；UI3 画布内联气泡，替代 prompt）──
function openLinkBubble(src, tgt, srcProp, tgtProp) {
  const self = src === tgt;
  state.pendingLink = {
    source: src, target: tgt,
    apiName: suggestLinkName(src, tgt, self),
    cardinality: self ? 'oneToMany' : 'oneToMany',
    roleA: self ? 'parent' : '', roleB: self ? 'child' : '',
    sourceProperty: srcProp || '', targetProperty: tgtProp || '',
    self,
  };
  refresh('content');
}
function suggestLinkName(src, tgt, self) {
  const cap = (s) => s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
  const lo = (s) => s ? s.charAt(0).toLowerCase() + s.slice(1) : s;
  if (self) return lo(src) + 'Hierarchy';
  return lo(src) + 'Has' + cap(tgt);
}
// 速建气泡确认 → 建关系。
async function confirmLinkBubble(root) {
  const pl = state.pendingLink; if (!pl) return;
  const q = (sel) => { const el = root.querySelector(sel); return el ? el.value.trim() : ''; };
  const apiName = q('[data-lf="apiName"]');
  if (!validApiName(apiName)) { flashAll('关系 apiName 非法（字母/下划线开头）', true); return; }
  const cardinality = q('[data-lf="cardinality"]') || 'oneToMany';
  const displayName = q('[data-lf="displayName"]');
  const roleA = q('[data-lf="roleA"]'); const roleB = q('[data-lf="roleB"]');
  // 记录属性到属性映射（会话内；后端 LinkType 暂不存外键属性，refreshAll 后据此重挂锚点）。
  if (pl.sourceProperty || pl.targetProperty) {
    const mp = {};
    if (pl.sourceProperty) mp.sourceProperty = pl.sourceProperty;
    if (pl.targetProperty) mp.targetProperty = pl.targetProperty;
    state.linkProps[apiName] = mp;
  }
  // 先清 pendingLink，再 saveLink（其内部 refreshAll 会重渲 content；若不先清，气泡会闪回）。
  state.pendingLink = null;
  await saveLink({ apiName, objectTypeA: pl.source, objectTypeB: pl.target, cardinality, displayName, roleA, roleB });
}
function cancelLinkBubble() { state.pendingLink = null; refresh('content'); }

async function saveLink(body) {
  try {
    await apiJson(API + '/link-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    flashAll('已保存关系 ' + body.apiName);
    await loadAll(); pushSpecToComponent(); refreshAll();
    selectElement('link', body.apiName, true);
  } catch (e) { flashAll('保存关系失败：' + e.message, true); }
}

// ── 事件绑定 ──
function bind(root, view, host) {
  if (root.__ogBound === view) return;
  root.__ogBound = view;
  root.addEventListener('click', async (ev) => {
    const selRow = ev.target.closest('[data-sel-id]');
    if (selRow) { selectElement(selRow.getAttribute('data-sel-kind'), selRow.getAttribute('data-sel-id')); return; }
    const act = ev.target.closest('[data-act]'); if (!act) return;
    const a = act.getAttribute('data-act');
    const pi = act.hasAttribute('data-pi') ? +act.getAttribute('data-pi') : -1;
    if (a === 'publish') return doPublish();
    if (a === 'new-object') return doNewObject();
    if (a === 'new-interface') return doNewInterface();
    if (a === 'auto-layout') { if (state.el) state.el.autoLayout(); return; }
    if (a === 'add-prop') return addPropRow();
    if (a === 'del-prop') return delPropRow(pi);
    if (a === 'save-object') return doSaveObject(root);
    if (a === 'del-object') return doDelObjectType(act.getAttribute('data-id'));
    if (a === 'del-link') return doDelLink(act.getAttribute('data-id'));
    // ── UI3 关系直接操作 ──
    if (a === 'confirm-link') return confirmLinkBubble(root);
    if (a === 'cancel-link') return cancelLinkBubble();
    if (a === 'save-link') return doSaveLink(root, act.getAttribute('data-id'));
    // ── UI2 属性编辑深化 ──
    if (a === 'toggle-sub') return togglePropOpen(pi);
    if (a === 'add-sub') return addSubProp(pi);
    if (a === 'del-sub') return delSubProp(pi, +act.getAttribute('data-ci'));
    if (a === 'add-shared') return addSharedRef(root);
    if (a === 'batch-required') return batchSetRequired(true);
    if (a === 'batch-unrequired') return batchSetRequired(false);
    if (a === 'batch-del') return batchDelProps();
    if (a === 'batch-clear') { state.selRows.clear(); return refresh('property'); }
    // ── UI5 动能层：函数 / 动作 ──
    if (a === 'new-function') return doNewFunction();
    if (a === 'new-action') return doNewAction();
    if (a === 'new-from-template') return doNewFromTemplate();
    if (a === 'fn-add-input') return fnAddInput(root);
    if (a === 'fn-del-input') return fnDelInput(root, +act.getAttribute('data-i'));
    if (a === 'save-function') return doSaveFunction(root);
    if (a === 'eval-function') return doEvalFunction(root, act.getAttribute('data-id'));
    if (a === 'del-function') return doDelFunction(act.getAttribute('data-id'));
    if (a === 'ac-add-param') return acAdd(root, 'parameters', { name: 'param' + (((state.detail || {}).parameters || []).length + 1), type: 'object' });
    if (a === 'ac-del-param') return acDel(root, 'parameters', +act.getAttribute('data-i'));
    if (a === 'ac-add-logic') return acAdd(root, 'logic', { op: 'modifyObject', target: '' });
    if (a === 'ac-del-logic') return acDel(root, 'logic', +act.getAttribute('data-i'));
    if (a === 'ac-add-val') return acAdd(root, 'validations', { expression: '', message: '' });
    if (a === 'ac-del-val') return acDel(root, 'validations', +act.getAttribute('data-i'));
    if (a === 'ac-add-fx') return acAdd(root, 'sideEffects', { kind: 'notification' });
    if (a === 'ac-del-fx') return acDel(root, 'sideEffects', +act.getAttribute('data-i'));
    if (a === 'ac-add-sevar') { collectAction(root); const i = +act.getAttribute('data-i'); const s = (state.detail.sideEffects || [])[i]; if (s) { s._vars = s._vars || []; s._vars.push({ name: '', value: '' }); } return refresh('property'); }
    if (a === 'ac-del-sevar') { collectAction(root); const i = +act.getAttribute('data-i'), j = +act.getAttribute('data-j'); const s = (state.detail.sideEffects || [])[i]; if (s && s._vars) s._vars.splice(j, 1); return refresh('property'); }
    if (a === 'dryrun-action') return doRunAction(root, act.getAttribute('data-id'), true);
    if (a === 'exec-action') return doRunAction(root, act.getAttribute('data-id'), false);
    if (a === 'save-action') return doSaveAction(root);
    if (a === 'del-action') return doDelAction(act.getAttribute('data-id'));
  });
  // 属性区：input（apiName 即时校验）+ change（下拉/勾选即时入 state.detail）——只在 property 区绑。
  if (view === 'property') {
    root.addEventListener('input', (ev) => { if (syncKinetic(root)) return; onPropInput(ev, root); });
    root.addEventListener('change', (ev) => { const kindSel = ev.target.closest && ev.target.closest('[data-sef="kind"]'); if (syncKinetic(root)) { if (kindSel) refresh('property'); return; } onPropChange(ev, root); });
    bindDragReorder(root);
  }
}
// UI5：动作/函数编辑器的字段改动即时同步进 state.detail（不重渲，保光标）——
// 使保存/刷新不依赖"DOM 在正确时刻"，杜绝晚到 refresh 覆盖编辑。返回 true 表示已处理（属于动能层）。
function syncKinetic(root) {
  if (!state.sel) return false;
  if (state.sel.kind === 'function') { collectFn(root); return true; }
  if (state.sel.kind === 'action') { collectAction(root); return true; }
  return false;
}

// apiName 即时校验（不重渲整表，只切红框，保光标）。
function onPropInput(ev, root) {
  const el = ev.target;
  if (el.getAttribute && el.getAttribute('data-pf') === 'apiName') {
    const i = +el.getAttribute('data-pi');
    if (state.detail && state.detail.properties[i]) state.detail.properties[i].apiName = el.value;
    el.classList.toggle('bad', !validApiName(el.value));
  } else if (el.getAttribute && el.getAttribute('data-sf')) {
    // 子属性即时入 state
    const wrap = el.closest('[data-pi][data-ci]');
    if (wrap) {
      const pi = +wrap.getAttribute('data-pi'), ci = +wrap.getAttribute('data-ci');
      const p = state.detail && state.detail.properties[pi];
      if (p && p.children && p.children[ci]) p.children[ci][el.getAttribute('data-sf')] = el.value;
    }
  }
}
// 下拉/勾选/单选即时入 state.detail，并按需重渲（影响校验/展开的须重渲）。
function onPropChange(ev, root) {
  const el = ev.target;
  const pf = el.getAttribute && el.getAttribute('data-pf');
  const df = el.getAttribute && el.getAttribute('data-df');
  const d = state.detail; if (!d) return;
  if (df) { d[df] = el.value; if (df === 'displayName' || df === 'status') return; refresh('property'); return; }
  if (el.classList && el.classList.contains('o-rowsel')) {
    const i = +el.getAttribute('data-pi');
    if (el.checked) state.selRows.add(i); else state.selRows.delete(i);
    // 不 full refresh（会销毁其它复选框，打断连续勾选）——只切当前行高亮 + 更新批量条。
    const tr = el.closest('.o-prow'); if (tr) tr.classList.toggle('rsel', el.checked);
    updateBatchBar(root);
    return;
  }
  if (!pf) return;
  const i = +el.getAttribute('data-pi'); const p = d.properties[i]; if (!p) return;
  if (pf === 'apiName') { p.apiName = el.value; return refresh('property'); } // blur 后重渲（更新问题条/保存禁用/标题下拉）
  if (pf === 'baseType') { p.baseType = el.value; return refresh('property'); } // 类型变 struct/array → 需重渲展开按钮
  if (pf === 'semanticType') { p.semanticType = el.value || undefined; return; }
  if (pf === 'required') { p.required = el.checked; return; }
  if (pf === 'isIndexed') { p.isIndexed = el.checked; return; }
  if (pf === 'pk') { d.primaryKey = p.apiName; return refresh('property'); }
}

// HTML5 拖拽排序属性行。
function bindDragReorder(root) {
  const tb = root.querySelector('[data-role="prop-tbody"]'); if (!tb) return;
  let dragI = -1;
  tb.addEventListener('dragstart', (ev) => {
    const tr = ev.target.closest('tr[data-pi]'); if (!tr) return;
    dragI = +tr.getAttribute('data-pi'); ev.dataTransfer.effectAllowed = 'move';
  });
  tb.addEventListener('dragover', (ev) => { ev.preventDefault(); });
  tb.addEventListener('drop', (ev) => {
    ev.preventDefault();
    const tr = ev.target.closest('tr[data-pi]'); if (!tr || dragI < 0) return;
    const dropI = +tr.getAttribute('data-pi');
    if (dropI === dragI) return;
    const props = state.detail.properties;
    const [moved] = props.splice(dragI, 1);
    props.splice(dropI, 0, moved);
    state.selRows.clear();
    dragI = -1;
    refresh('property');
  });
}

async function doPublish() {
  const summary = window.prompt('发布摘要：', ''); if (summary === null) return;
  try { const r = await publish(summary); flashAll('已发布 v' + (r && r.version)); await loadAll(); refreshAll(); }
  catch (e) { flashAll('发布失败：' + e.message, true); }
}
async function doNewObject() {
  const apiName = window.prompt('新对象类型 apiName（如 Customer）：', ''); if (!apiName) return;
  try {
    await apiJson(API + '/object-types', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ apiName, displayName: apiName, properties: [] }) });
    flashAll('已建对象类型 ' + apiName); await loadAll(); pushSpecToComponent(); refreshAll(); selectElement('object', apiName);
  } catch (e) { flashAll('建对象类型失败：' + e.message, true); }
}
async function doNewInterface() {
  const apiName = window.prompt('新接口 apiName（如 Locatable）：', ''); if (!apiName) return;
  try {
    await apiJson(API + '/interfaces', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ apiName, displayName: apiName, properties: [] }) });
    flashAll('已建接口 ' + apiName); await loadAll(); pushSpecToComponent(); refreshAll();
  } catch (e) { flashAll('建接口失败：' + e.message, true); }
}
function addPropRow() {
  if (!state.detail) return;
  state.detail.properties = state.detail.properties || [];
  state.detail.properties.push({ apiName: 'field' + (state.detail.properties.length + 1), baseType: 'string' });
  state.selRows.clear();
  refresh('property');
}
async function delPropRow(i) {
  if (!state.detail || !state.detail.properties) return;
  const p = state.detail.properties[i];
  // UI4 守卫：删主键属性 → 警示（删后须重设主键才能保存）。
  if (p && p.apiName === state.detail.primaryKey) {
    const c = await openDialog({
      title: '删除主键属性', severity: 'warn',
      body: `<p>「<code>${esc(p.apiName)}</code>」是当前主键。</p><p class="o-dlgmuted">删除后须重设主键才能保存；已物化对象按主键组织，变更主键需迁移。</p>`,
      buttons: [{ label: '取消', id: '__cancel' }, { label: '仍然删除', id: 'delete', kind: 'danger' }],
    });
    if (c !== 'delete') return;
    state.detail.primaryKey = '';
  }
  state.detail.properties.splice(i, 1);
  state.selRows.clear();
  refresh('property');
}
// ── UI2：复合类型子属性 ──
function togglePropOpen(i) {
  const p = state.detail && state.detail.properties[i]; if (!p) return;
  p.__open = !p.__open; refresh('property');
}
function addSubProp(i) {
  const p = state.detail && state.detail.properties[i]; if (!p) return;
  p.children = p.children || [];
  p.children.push({ apiName: 'sub' + (p.children.length + 1), baseType: 'string' });
  p.__open = true; refresh('property');
}
function delSubProp(pi, ci) {
  const p = state.detail && state.detail.properties[pi]; if (!p || !p.children) return;
  p.children.splice(ci, 1); refresh('property');
}
// ── UI2：引用共享属性（继承 baseType + semanticType，标记 sharedProperty）──
function addSharedRef(root) {
  const sel = root.querySelector('[data-role="ref-shared"]');
  const apiName = sel && sel.value; if (!apiName) { flashAll('先选一个共享属性', true); return; }
  const sp = state.shared.find(s => s.apiName === apiName); if (!sp) return;
  state.detail.properties = state.detail.properties || [];
  if (state.detail.properties.some(p => p.apiName === sp.apiName)) { flashAll('已引用 ' + sp.apiName, true); return; }
  const p = { apiName: sp.apiName, baseType: sp.baseType, sharedProperty: sp.apiName };
  if (sp.semanticType) p.semanticType = sp.semanticType;
  state.detail.properties.push(p);
  refresh('property');
}
// ── UI2：批量 ──
function batchSetRequired(on) {
  if (!state.detail) return;
  state.selRows.forEach(i => { const p = state.detail.properties[i]; if (p) p.required = on; });
  refresh('property');
}
function batchDelProps() {
  if (!state.detail || !state.selRows.size) return;
  if (!window.confirm('批量删除 ' + state.selRows.size + ' 个属性？')) return;
  const idx = [...state.selRows].sort((a, b) => b - a); // 降序删，索引不错位
  idx.forEach(i => state.detail.properties.splice(i, 1));
  state.selRows.clear();
  refresh('property');
}
// 保存前最终同步（displayName 走 input 不触发 refresh，须最后取一次）+ 子属性打包进 constraints。
function collectDetail(root) {
  const d = state.detail; if (!d) return;
  const gv = (sel) => { const el = root.querySelector(sel); return el ? el.value : undefined; };
  const dn = gv('[data-df="displayName"]'); if (dn !== undefined) d.displayName = dn;
  // titleProperty/status/属性各字段已由 onPropChange 即时入 state.detail。
  // 复合类型子属性 → constraints.children（O1 后端 constraints jsonb 原样保留，前端还原）。
  (d.properties || []).forEach(p => {
    delete p.__open; // 清前端会话态
    if (COMPOSITE_TYPES.includes(p.baseType) && p.children && p.children.length) {
      p.constraints = Object.assign({}, p.constraints, { children: p.children });
    }
  });
}
async function doSaveObject(root) {
  collectDetail(root);
  const issues = collectPropIssues(state.detail.properties || [], state.detail);
  if (issues.length) { flashAll('无法保存：' + issues[0], true); return; }
  // UI4 守卫：主键变更且有物化对象 → 迁移警示。
  const origPk = state.detail.__origPk;
  if (origPk && state.detail.primaryKey && state.detail.primaryKey !== origPk) {
    const { matCount } = await computeObjectImpact(state.detail.apiName);
    if (matCount != null && matCount > 0) {
      const c = await openDialog({
        title: '变更主键', severity: 'warn',
        body: `<p>主键 <code>${esc(origPk)}</code> → <code>${esc(state.detail.primaryKey)}</code>。</p>
               <div class="o-imp o-impwarn">已物化 <b>${matCount.toLocaleString()}</b> 条对象——O2 对象表 <code>oo_${esc(state.detail.apiName)}</code> 按主键组织，变更主键需迁移数据。</div>`,
        buttons: [{ label: '取消', id: '__cancel' }, { label: '继续保存', id: 'save', kind: 'danger' }],
      });
      if (c !== 'save') return;
    }
  }
  try {
    // 落库前剥离纯前端字段（children 已进 constraints；__open/__origPk 前端会话态）。
    const payload = deepClone(state.detail);
    delete payload.__origPk;
    (payload.properties || []).forEach(p => { delete p.children; delete p.__open; });
    await saveObjectTypeFromDetail(payload);
    flashAll('已保存 ' + state.detail.apiName); state.selRows.clear();
    await loadAll(); pushSpecToComponent(); refreshAll();
    // 重新选中以拉最新 detail（forceReload：保存后须覆盖为持久态，含回读的 constraints.children 还原）。
    if (state.sel) selectElement('object', state.sel.id, true);
  }
  catch (e) { flashAll('保存失败：' + e.message, true); }
}
async function doDelLink(id) {
  const meta = (state.manifest.linkTypes || []).find(x => x.apiName === id) || {};
  const choice = await openDialog({
    title: '删除关系 ' + id, severity: 'warn',
    body: `<p>关系 <b>${esc(meta.objectTypeA || '')}</b> ▶ <b>${esc(meta.objectTypeB || '')}</b> 将被移除。</p>
           <p class="o-dlgmuted">删除后该关系已建的所有边（ol_edge）将不再可遍历（Search-Around 断链）。</p>`,
    buttons: [{ label: '取消', id: '__cancel' }, { label: '删除关系', id: 'delete', kind: 'danger' }],
  });
  if (choice !== 'delete') return;
  try { await apiJson(API + '/link-types/' + encodeURIComponent(id), { method: 'DELETE' }); flashAll('已删除 ' + id); state.sel = null; state.detail = null; await loadAll(); pushSpecToComponent(); refreshAll(); }
  catch (e) { flashAll('删除失败：' + e.message, true); }
}

// ══════════════ UI4 演进安全守卫 ══════════════

// 自包含专业对话框（替代 window.confirm）。severity: info|warn|danger。返回点击的 button id（backdrop/✕ → null）。
let _lastDialogValues = {};
function openDialog(opts) {
  return new Promise((resolve) => {
    const ov = document.createElement('div');
    ov.className = 'o-dlg-overlay';
    const sev = opts.severity || 'info';
    const icon = sev === 'info' ? 'ℹ' : '⚠';
    const btns = (opts.buttons || [{ label: '确定', id: 'ok', kind: 'primary' }])
      .map(b => `<button class="o-btn ${b.kind || ''}" data-dlg="${esc(b.id)}">${esc(b.label)}</button>`).join('');
    ov.innerHTML = `<style>${css()}</style><div class="o o-dlg o-dlg-${sev}" role="dialog">
      <div class="o-dlghd"><span class="o-dlgic">${icon}</span> ${esc(opts.title)}<button class="o-dlgx" data-dlg="__cancel" title="关闭">✕</button></div>
      <div class="o-dlgbody">${opts.body || ''}</div>
      <div class="o-dlgfoot">${btns}</div>
    </div>`;
    const done = (id) => {
      // 移除前快照对话框内带 data-k 的输入值（供调用方在 await 之后读取；否则 overlay 已 remove）。
      _lastDialogValues = {};
      ov.querySelectorAll('[data-k]').forEach(el => { _lastDialogValues[el.getAttribute('data-k')] = el.value; });
      ov.remove();
      resolve(id === '__cancel' ? null : id);
    };
    ov.addEventListener('click', (e) => {
      if (e.target === ov) return done('__cancel'); // 点背景关闭
      const b = e.target.closest('[data-dlg]'); if (b) done(b.getAttribute('data-dlg'));
    });
    document.body.appendChild(ov);
  });
}

// 影响面分析：被哪些关系引用 + 已物化对象数（O2；未物化→null）。
async function computeObjectImpact(apiName) {
  const rels = (state.manifest.linkTypes || []).filter(l => l.objectTypeA === apiName || l.objectTypeB === apiName);
  let matCount = null;
  try {
    const r = await apiJson(API + '/object-sets/aggregate', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ objectSet: { op: 'base', objectType: apiName }, aggregation: { kind: 'count' } }) });
    matCount = (r && typeof r.count === 'number') ? r.count : null;
  } catch { matCount = null; } // 未物化（oo_ 表不存在）
  return { rels, matCount };
}

// 删除对象类型：影响面对话框（关系引用 + 物化对象 + 状态机 + 「改为废弃更安全」）。
async function doDelObjectType(id) {
  const status = (state.detail && state.detail.apiName === id && state.detail.status) || 'experimental';
  const { rels, matCount } = await computeObjectImpact(id);
  const danger = status !== 'experimental' || (matCount != null && matCount > 0);
  const relList = rels.length
    ? `<div class="o-imp"><b>被 ${rels.length} 个关系引用（将一并删除以免悬空）：</b><ul>${rels.map(l => `<li><code>${esc(l.apiName)}</code> · ${l.objectTypeA === id ? 'A端' : 'B端'}</li>`).join('')}</ul></div>`
    : `<div class="o-imp o-impok">无关系引用</div>`;
  const matLine = matCount == null
    ? `<div class="o-imp">已物化对象：<b>未物化</b>（无 O2 数据）</div>`
    : `<div class="o-imp ${matCount > 0 ? 'o-impwarn' : ''}">已物化对象：<b>${matCount.toLocaleString()}</b> 条${matCount > 0 ? '（删除定义不清 oo_ 表数据，需另行清理）' : ''}</div>`;
  const stLine = `<div class="o-imp">当前状态：<b>${STATUS_LABEL[status] || status}</b>${status === 'experimental' ? '（试验态可安全删除）' : '（已启用，删除影响下游 OSDK/动作/函数引用）'}</div>`;
  const body = `<p class="o-dlgwarn">⚠ apiName 是跨版本稳定锚——删除后引用它的关系/动作/函数将断裂。</p>
    ${stLine}${relList}${matLine}
    <div class="o-dlgtip">💡 更安全：改为「废弃 Deprecated」而非删除——保留定义与数据，新数据不再写入，下游平滑迁移后再移除。</div>`;
  const choice = await openDialog({
    title: '删除对象类型 ' + id, severity: danger ? 'danger' : 'warn', body,
    buttons: [{ label: '取消', id: '__cancel' }, { label: '改为废弃', id: 'deprecate' }, { label: '仍然删除', id: 'delete', kind: 'danger' }],
  });
  if (choice === 'deprecate') return deprecateObjectType(id);
  if (choice !== 'delete') return;
  try {
    // 级联删引用它的关系（避免悬空边），再删对象类型定义。
    for (const l of rels) { try { await apiJson(API + '/link-types/' + encodeURIComponent(l.apiName), { method: 'DELETE' }); } catch { /* */ } }
    await apiJson(API + '/object-types/' + encodeURIComponent(id), { method: 'DELETE' });
    flashAll('已删除对象类型 ' + id + (rels.length ? ` 及 ${rels.length} 个关系` : ''));
    state.sel = null; state.detail = null; await loadAll(); pushSpecToComponent(); refreshAll();
  } catch (e) { flashAll('删除失败：' + e.message, true); }
}
// 废弃（更安全的替代）：置 status=deprecated + 保存。
async function deprecateObjectType(id) {
  try {
    const d = await apiJson(API + '/object-types/' + encodeURIComponent(id));
    d.status = 'deprecated';
    await saveObjectTypeFromDetail(d);
    flashAll('已将 ' + id + ' 改为废弃');
    await loadAll(); pushSpecToComponent(); refreshAll();
    if (state.sel && state.sel.id === id) selectElement('object', id, true);
  } catch (e) { flashAll('废弃失败：' + e.message, true); }
}
function pushSpecToComponent() { if (state.el && typeof state.el.setSpec === 'function' && state.spec) state.el.setSpec(state.spec); }

// ── toast ──
function flashAll(msg, err) {
  for (const host of state.hosts) {
    const root = hostRoot(host); if (!root) continue;
    let t = root.querySelector('.o-toast');
    if (!t) { t = document.createElement('div'); t.className = 'o-toast'; root.appendChild(t); }
    t.textContent = msg; t.classList.toggle('err', !!err); t.classList.add('show');
    setTimeout(() => t.classList.remove('show'), 3200);
    break;
  }
}

function css() {
  return `
  .o{--o-bg:var(--sapBackgroundColor,#0b1020);--o-fg:var(--sapTextColor,#e6ecf5);--o-muted:var(--sapContent_LabelColor,#94a3b8);--o-border:var(--sapList_BorderColor,#243049);--o-panel:var(--sapList_Background,#121a2e);--o-accent:var(--sapButton_Emphasized_Background,#22d3ee);--o-ok:#22c55e;--o-err:#ef4444;--o-mono:ui-monospace,Menlo,Consolas,monospace;color:var(--o-fg);font:13px/1.5 ui-sans-serif,system-ui,'PingFang SC',sans-serif;height:100%;box-sizing:border-box;position:relative}
  .ph,.o-empty2{color:var(--o-muted);padding:16px;text-align:center;font-size:12.5px}
  .o-btn{cursor:pointer;border:1px solid var(--o-border);background:var(--o-panel);color:var(--o-fg);border-radius:7px;padding:6px 11px;font-size:12.5px}
  .o-btn:hover{border-color:var(--o-accent)}
  .o-btn.xs{padding:3px 8px;font-size:11.5px}
  .o-btn.primary{background:var(--o-accent);border:none;color:#04283a;font-weight:700}
  .o-btn.ok{background:var(--o-ok);border:none;color:#052e16;font-weight:700}
  .o-btn.danger{color:var(--o-err)}
  .o-inp,.o-cin,.o-csel{width:100%;background:var(--o-panel);border:1px solid var(--o-border);color:var(--o-fg);border-radius:6px;padding:5px 8px;font-size:12.5px;box-sizing:border-box}
  label{display:block;color:var(--o-muted);font-size:11.5px;margin:8px 0 3px}
  code{color:var(--o-muted);font-family:var(--o-mono);font-size:11px}
  /* model */
  .o-model{padding:14px}
  .o-mtitle{font-size:14px;font-weight:700;margin-bottom:10px}
  .o-mtiles{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-bottom:10px}
  .o-tile{background:var(--o-panel);border:1px solid var(--o-border);border-radius:9px;padding:8px 10px;text-align:center}
  .o-tile b{display:block;font-size:20px;font-weight:800}.o-tile span{color:var(--o-muted);font-size:11px}
  .o-mver{margin:6px 0 12px;font-size:12px;color:var(--o-muted)}.o-draft{color:var(--o-err)}
  /* explorer */
  .o-explorer{padding:10px;overflow:auto}
  .o-search{margin-bottom:8px}
  .o-grp{margin-bottom:6px}
  .o-ghd{font-size:11.5px;font-weight:700;color:var(--o-muted);padding:4px 2px;display:flex;align-items:center;gap:6px}
  .o-gn{margin-left:auto;background:var(--o-panel);border-radius:10px;padding:0 7px;font-size:10.5px}
  .o-elist{list-style:none;margin:0;padding:0}
  .o-erow{display:flex;align-items:center;gap:8px;padding:5px 8px;border-radius:6px;cursor:pointer}
  .o-erow:hover{background:var(--o-panel)}
  .o-erow.sel{background:var(--o-panel);box-shadow:inset 2.5px 0 0 var(--o-accent)}
  .o-ename{flex:1;font-size:12.5px}
  .o-newbar{margin-top:10px;display:flex;gap:6px;flex-wrap:wrap}
  .o-tplhelp{margin-bottom:10px;max-height:160px;overflow:auto}
  .o-tplrow{font-size:11.5px;color:var(--o-muted,#94a3b8);padding:4px 0;border-bottom:1px solid var(--o-border,#243049)}
  .o-tplrow b{color:var(--o-fg,#e6ecf5)}
  /* UI5 动能层：函数/动作编辑器 */
  .o-code{width:100%;box-sizing:border-box;background:var(--o-bg,#0b1020);border:1px solid var(--o-border);color:var(--o-fg);border-radius:8px;padding:8px 10px;font-family:var(--o-mono);font-size:12px;line-height:1.5;resize:vertical}
  .o-codein{padding:5px 8px;font-size:11.5px}
  .o-feelhint{font-size:10px;color:var(--o-accent2,#a78bfa);border:1px solid var(--o-accent2,#a78bfa);border-radius:9px;padding:0 6px;font-weight:400}
  .o-fnlist{display:flex;flex-direction:column;gap:5px;margin-bottom:4px}
  .o-fnrow{display:flex;gap:5px;align-items:center}
  .o-fxblock{border:1px solid var(--o-border,#243049);border-radius:8px;padding:6px 7px;margin-bottom:7px}
  .o-fxtag{flex:1;font-size:11.5px;color:var(--o-accent,#22d3ee);font-weight:700}
  .o-fxflow{margin-top:7px;padding:7px 8px;border-left:2px solid var(--o-accent,#22d3ee);background:rgba(34,211,238,.05);border-radius:0 6px 6px 0}
  .o-fxflow>label{display:block;font-size:11px;color:var(--o-muted,#94a3b8);margin:6px 0 3px}
  .o-fxflow .o-hint{color:var(--o-muted,#94a3b8);font-weight:400;margin-left:4px}
  .o-phd3{font-size:11.5px;font-weight:700;margin:9px 0 5px;display:flex;align-items:center;gap:6px}
  .o-sevar{margin-bottom:4px}
  .o-fnrow>input,.o-fnrow>select{flex:1;min-width:0}
  .o-fnrow2{display:flex;gap:5px;align-items:center}
  .o-fnrow2>.o-codein{flex:2;min-width:0}
  .o-fnrow2>.o-cin{flex:1;min-width:0}
  /* content */
  .o-content{display:flex;flex-direction:column;height:100%;width:100%}
  .o-toolbar{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--o-border)}
  .o-title{font-size:13px}
  .o-dirty{font-size:11.5px;color:var(--o-muted)}.o-dirty.on{color:var(--o-err)}
  .o-sp{flex:1}
  .o-hint{padding:5px 10px;font-size:11px;color:var(--o-muted);border-bottom:1px solid var(--o-border)}
  .o-canvaswrap{flex:1;min-height:0}
  /* property */
  .o-prop{padding:12px;overflow:auto}
  .o-phd{font-size:13.5px;font-weight:700;margin-bottom:8px;display:flex;align-items:center;gap:8px}
  .o-phd2{font-size:12px;font-weight:700;margin:14px 0 6px;display:flex;align-items:center;gap:8px}
  .o-row2{display:grid;grid-template-columns:1fr 1fr;gap:8px}
  .o-ptable{width:100%;border-collapse:collapse;font-size:12px}
  .o-ptable th{color:var(--o-muted);font-weight:600;font-size:11px;text-align:left;padding:4px 5px;border-bottom:1px solid var(--o-border)}
  .o-ptable td{padding:3px 4px;border-bottom:1px solid var(--o-border)}
  .o-ptable td.o-c{text-align:center}
  .o-pactions{margin-top:12px}
  /* O8 运行结果面板（函数求值 / 动作试算执行） */
  .o-runbox{margin-top:10px}
  .o-runok{padding:8px 11px;border-radius:8px;background:rgba(34,197,94,.1);border:1px solid rgba(34,197,94,.35);color:var(--o-ok,#22c55e);font-size:12.5px}
  .o-runerr{padding:8px 11px;border-radius:8px;background:rgba(239,68,68,.1);border:1px solid rgba(239,68,68,.35);color:var(--o-err,#ef4444);font-size:12.5px}
  .o-runmeta{color:var(--o-muted,#94a3b8);font-size:11px;font-family:var(--o-mono,monospace)}
  .o-runok b{font-family:var(--o-mono,monospace)}
  /* UI2 属性编辑深化 */
  .o-pn{background:var(--o-panel);border-radius:10px;padding:0 7px;font-size:10.5px;color:var(--o-muted);font-weight:600}
  .o-cin.bad{border-color:var(--o-err);background:rgba(239,68,68,.08)}
  .o-prow.rsel{background:rgba(34,211,238,.06)}
  .o-prow .handle{cursor:grab;color:var(--o-muted);user-select:none}
  .o-prow[draggable=true]:active .handle{cursor:grabbing}
  .o-csel.sem{max-width:78px}
  .o-refname{font-size:12px;color:var(--o-accent2,#a78bfa)}
  .o-rty{font-size:11px;color:var(--o-muted)}
  .o-xbtn{background:none;border:none;color:var(--o-muted);cursor:pointer;font-size:11px;padding:0 3px}
  .o-refbar{display:flex;gap:6px;align-items:center;margin:6px 0;font-size:11.5px;color:var(--o-muted)}
  .o-refsel{flex:1}
  .o-batch{background:var(--o-panel);border:1px solid var(--o-border);border-radius:7px;padding:6px 9px;margin:6px 0;font-size:11.5px;display:flex;gap:6px;align-items:center;flex-wrap:wrap}
  .o-issues{margin-top:8px;padding:7px 10px;border-radius:7px;background:rgba(239,68,68,.08);border:1px solid rgba(239,68,68,.3);color:var(--o-err);font-size:11.5px}
  .o-okbar{margin-top:8px;padding:5px 10px;border-radius:7px;color:var(--o-ok);font-size:11.5px}
  .o-subrow td{background:rgba(148,163,184,.05);padding:0}
  .o-subwrap{padding:8px 10px 8px 30px}
  .o-sublabel{font-size:10.5px;color:var(--o-muted);margin-bottom:5px}
  .o-subrowl{display:flex;gap:5px;margin-bottom:4px;align-items:center}
  .o-cin.xs,.o-csel.xs{padding:3px 6px;font-size:11px}
  .o-btn.xs:disabled,.o-btn.primary:disabled{opacity:.45;cursor:not-allowed}
  .o-kv{display:flex;justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--o-border);font-size:12.5px}.o-kv span{color:var(--o-muted)}
  .o-pmuted{color:var(--o-muted);font-size:12px}
  /* UI3 关系直接操作 */
  .o-content{position:relative}
  .o-linkbubble{position:absolute;top:56px;left:50%;transform:translateX(-50%);width:300px;background:var(--o-panel);border:1px solid var(--o-accent);border-radius:12px;padding:14px 16px;box-shadow:0 12px 40px rgba(0,0,0,.45);z-index:20}
  .o-lbhd{font-size:12.5px;color:var(--o-muted);margin-bottom:8px}.o-lbhd b{color:var(--o-fg)}
  .o-lbfoot{display:flex;justify-content:flex-end;gap:8px;margin-top:12px}
  .o-lendpoints{font-size:12.5px;margin:4px 0 8px;padding:6px 10px;background:var(--o-panel);border:1px solid var(--o-border);border-radius:7px;display:flex;align-items:center;gap:8px}
  .o-larrow{color:var(--o-accent);font-weight:700}
  .o-lself{margin-left:auto;font-size:10.5px;color:var(--o-accent2,#a78bfa);border:1px solid var(--o-accent2,#a78bfa);border-radius:10px;padding:1px 8px}
  /* toast */
  .o-toast{position:absolute;right:12px;bottom:12px;background:var(--o-panel);border:1px solid var(--o-border);border-left:3px solid var(--o-accent);border-radius:8px;padding:9px 13px;font-size:12.5px;opacity:0;transform:translateY(6px);transition:.2s;pointer-events:none;max-width:320px}
  .o-toast.show{opacity:1;transform:none}.o-toast.err{border-left-color:var(--o-err)}
  /* UI4 演进安全对话框 */
  .o-dlg-overlay{position:fixed;inset:0;background:rgba(4,8,18,.6);display:flex;align-items:center;justify-content:center;z-index:1000;backdrop-filter:blur(2px)}
  .o-dlg{width:440px;max-width:92vw;max-height:86vh;overflow:auto;background:var(--o-panel);border:1px solid var(--o-border);border-radius:14px;box-shadow:0 24px 70px rgba(0,0,0,.55)}
  .o-dlg-danger{border-top:3px solid var(--o-err)}
  .o-dlg-warn{border-top:3px solid var(--sapCriticalElementColor, #f59e0b)}
  .o-dlg-info{border-top:3px solid var(--o-accent)}
  .o-dlghd{display:flex;align-items:center;gap:9px;padding:14px 16px;font-size:14px;font-weight:700;border-bottom:1px solid var(--o-border)}
  .o-dlg-danger .o-dlgic{color:var(--o-err)}.o-dlg-warn .o-dlgic{color:#f59e0b}.o-dlg-info .o-dlgic{color:var(--o-accent)}
  .o-dlgx{margin-left:auto;background:none;border:none;color:var(--o-muted);cursor:pointer;font-size:14px}
  .o-dlgbody{padding:14px 16px;font-size:12.5px;line-height:1.6}
  .o-dlgbody p{margin:0 0 8px}
  .o-dlgwarn{color:var(--o-err);font-weight:600}
  .o-dlgmuted{color:var(--o-muted)}
  .o-imp{margin:8px 0;padding:8px 11px;border-radius:8px;background:rgba(148,163,184,.08);border:1px solid var(--o-border)}
  .o-imp ul{margin:6px 0 0;padding-left:18px}.o-imp li{margin:2px 0}
  .o-impok{color:var(--o-ok)}
  .o-impwarn{border-color:rgba(245,158,11,.4);background:rgba(245,158,11,.08);color:var(--sapCriticalElementColor, #fbbf24)}
  .o-dlgtip{margin-top:10px;padding:9px 12px;border-radius:8px;background:rgba(34,211,238,.08);border:1px solid rgba(34,211,238,.3);color:var(--o-accent);font-size:12px}
  .o-dlgfoot{display:flex;justify-content:flex-end;gap:8px;padding:12px 16px;border-top:1px solid var(--o-border)}
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
