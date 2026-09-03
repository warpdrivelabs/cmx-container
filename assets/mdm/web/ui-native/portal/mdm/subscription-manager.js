/**
 * MDM 分发订阅管理（native-page，方案C两级模型：目标端点 + 订阅）。
 *
 * 双区域布局（参考 mdm-base-dct-def-manager 的 workspace explorer 结构）：
 *  - explorer（侧栏区）：目标端点列表视图——卡片（中文名为主 + 系统标识 + 通道/启停/订阅数/积压徽章）
 *    + 新建端点按钮；点选切换 content 区过滤。
 *  - content（内容区）：工具栏（新建订阅 / 编辑端点 / 测试端点 / 启停端点）
 *    + cmx-filter-bar（字典/状态）+ cmx-revo-grid + cmx-pager。
 *    订阅行操作：编辑 / 启停 / 投递 / 补发 / 删除（仅停用态）；通道测试上移到端点级。
 *
 * 跨区域联动：同 workspace 的各区域 native CE 共享 scope 且同 pageId 共享模块实例
 * （cmx-native-pages-host 按 scope materialize）——state 按 host.workspace 键控，
 * explorer 与 content 两视图读写同一份 state；端点选择/增删/启停双区联动刷新。
 *
 *  - 新建/编辑订阅同一弹框（单条，不做批量向导）：新建态字典为 cmx-combo-box 下拉
 *    （可输过滤，排除该端点已订阅字典），选定字典后按 DCT meta 水合「字段映射」源字段
 *    下拉与过滤行字段建议；事件/过滤/字段映射片段两态共用同一套渲染/绑定/读取。
 *    提交 POST /mdm/subscriptions（upsert，新建 = 单条创建）。
 *  - 编辑端点弹框：基本信息 + 通道配置（webhook URL/秘钥[随机生成/掩码]/超时；rest_pull consumerId）
 *    + 投递策略；密钥轮换单点生效（双写旧列保证回滚安全）。
 *  - 补发小对话框 fromSeq/toSeq/force → POST /api/mdm/publish。
 *
 * 端点（全部 /api 前缀，行字段 snake_case）：
 *   GET/POST /mdm/endpoints             端点列表（聚合统计+secret 掩码***）/ upsert（conflict 提示不阻断）
 *   POST /mdm/endpoints/{delete,set-active,test}
 *   GET  /mdm/subscriptions             过滤（endpointId/targetSys/dictCode/channel/active）+ 分页 + 统计
 *   POST /mdm/subscriptions             upsert（瘦身：endpoint_id+dict_code+事件/过滤/映射）
 *   POST /mdm/subscriptions/{delete,set-active}
 *   POST /mdm/publish                   手动补发
 *   GET  /mdm/subscriptions/channels    通道枚举（注册表驱动，feature 开启自动出现）
 *   GET  /mdm/activations               字典下拉数据源（target_dict 去重）
 *
 * 多实例安全：state 按 workspace scope 隔离（WeakMap）；局部更新（不整页重绘）。
 * 契约：export default { defaultView:'content', views:{ async content(ctx), async explorer(ctx) } }；
 * CMX 能力经 globalThis.__cmxDataComp 取用（禁止裸 import）。
 */

const cmx = () => (typeof globalThis !== 'undefined' && globalThis.__cmxDataComp) || {}

// HTML 转义：优先用组件库挂载的权威 escHtml，缺省时本地兜底（覆盖 & < > " '）。
const { escHtml: esc } = globalThis.__cmxDataComp // 共享转义（cmx-data-comp/lib/cmx-page-helpers.js）

const { apiGet, apiPost } = globalThis.__cmxDataComp // 共享 fetch 封装（信封解包+结构化错误）

// 轻量 toast（成功/失败轻反馈，3s 自动消失）；校验警告 cmxWarn、异常 cmxError。
const { showCmxToast } = globalThis.__cmxDataComp // 共享 toast（cmx-data-comp/lib/cmx-toast.js）

// ── state：按 workspace scope 隔离（explorer 与 content 两视图共享同一份）────
const _scopeState = new WeakMap()
function initState() {
  return {
    coord: null, dbId: '', host: null,
    hosts: new Set(),     // explorer 与 content 分属两个 cmx-native-pages-host 实例，都要挂委托/观察器
    endpoints: [],        // [{id,target_sys,channel,name,...,sub_count,stat_*}]
    curEpId: null,        // 当前选中端点 id（null = 未选，content 空态）
    subs: [], subTotal: 0, subPage: 1, pageSize: 20,
    fDict: '', fActive: '',
    channels: [],         // [{type,label}]
    dicts: [],            // 激活映射 target_dict 去重（code 数组）
    dictNames: {},        // dict code → 中文名（dct/meta 顶层 dictName；拉取失败降级纯 code）
    dictNamesReady: null, // 中文名后台预取 promise（不阻塞首屏；新建弹框打开前 await）
    grid: null,
    gridBuilt: null,    // 列模型+监听就绪的 grid 元素（applyData 重建/重试判据）
    __gridRetry: 0,
    explorerRoot: null,   // 侧栏（explorer 视图）.epx 根
    contentRoot: null,    // 内容区（content 视图）.pg 根
    initPromise: null,    // 预取数据幂等闸（两视图谁先到谁触发，只跑一次）
  }
}
function scopeKeyOf(host) { return (host && host.workspace) || host }
function getState(host) {
  const k = scopeKeyOf(host)
  if (!k) return null
  if (!_scopeState.has(k)) _scopeState.set(k, initState())
  return _scopeState.get(k)
}

// 坐标四元组（module 回退 mdm，dbId 兼读 workspace.context）——照 master-list 版本。
function readCoord(ctx) {
  const p = (ctx && ctx.props) || {}
  const wctx = ctx && ctx.host && ctx.host.workspace && ctx.host.workspace.context
  const get = (k) => (wctx && typeof wctx.get === 'function' ? wctx.get(k) : undefined)
  return {
    domain: get('domain') || p.domain || p.domainCode || '',
    application: get('application') || p.application || p.applicationCode || '',
    module: get('module') || p.module || 'mdm',
    dbId: p.dbId || p.db_id || get('dbId') || get('db_id') || '',
  }
}
function coordQs(st, extra = {}) {
  const c = st.coord || {}
  return new URLSearchParams({ domain: c.domain || '', application: c.application || '', module: c.module || 'mdm', ...extra }).toString()
}
function coordCtx(st) {
  const c = st.coord || {}
  if (!c.domain && !c.application) return {}
  return { domain: c.domain, application: c.application, module: c.module || 'mdm', dbId: c.dbId }
}
// 字典下拉显示文案：code · 中文名（中文名缺失降级纯 code）
function dictLabel(st, code) {
  const n = st.dictNames && st.dictNames[code]
  return n ? `${code} · ${n}` : code
}

// 打开并列门户标签页（照 master-list 模式；找不到 openNode 时仅告警不报错）。
function openTab(host, st, caption, nativePage, context, opts = {}) {
  let app = null
  try { app = document.querySelector('cmx-portal-app') } catch { app = null }
  if (!app || typeof app.openNode !== 'function') {
    let n = host
    for (let i = 0; i < 6 && n; i++) {
      if (typeof n.openNode === 'function') { app = n; break }
      const r = n.getRootNode && n.getRootNode(); n = r && r.host
    }
  }
  if (!app || typeof app.openNode !== 'function') { console.warn('[subscription-manager] 未找到 portal-app.openNode'); return }
  const ctxKey = (context && context.subscriptionId) || ''
  const key = opts.single ? 'single' : (ctxKey || Date.now())
  const c = st.coord || {}
  app.openNode({
    id: `${nativePage}-${key}`, name: nativePage, caption, type: 'workspace-node',
    domainCode: c.domain || '', applicationCode: c.application || '',
    workspace: { content: { caption, views: [{ type: 'native_pages', native_page: nativePage, view: 'content' }] } },
  }, { initialContext: context })
}

function styleCss() {
  return `
  .pg { height:100%; overflow:hidden; display:flex; flex-direction:column; box-sizing:border-box; padding:12px 20px 16px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); }
  .pg-head { margin-bottom:10px; }
  .pg-title { font-size:20px; font-weight:600; color:var(--sapTitleColor); }
  /* explorer（侧栏区）视图 */
  .epx { height:100%; overflow:hidden; display:flex; flex-direction:column; box-sizing:border-box; padding:8px 8px 10px;
    background:var(--sapBackgroundColor); color:var(--sapTextColor);
    font-family:var(--sapFontFamily,'72','Segoe UI',Arial,sans-serif); font-size:12px; }
  .epx-hd { display:flex; justify-content:space-between; align-items:center; padding:2px 4px 8px; }
  .epx-title { font-size:13px; font-weight:600; color:var(--sapTitleColor); }
  .ep-list { flex:1; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:6px; }
  .ep-card { border:1px solid var(--sapList_BorderColor); border-radius:6px; padding:5px 9px; cursor:pointer;
    transition:border-color .12s, background .12s; background:var(--sapList_Background); }
  .ep-card:hover { border-color:var(--sapBrandColor,#0a6ed1); }
  .ep-card.sel { border-color:var(--sapBrandColor,#0a6ed1);
    background:color-mix(in srgb, var(--sapBrandColor,#0a6ed1) 8%, transparent); }
  .ep-name { font-weight:600; font-size:13px; display:flex; align-items:center; gap:6px; min-width:0;
    color:var(--sapTextColor); }
  .ep-name b:first-child { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; flex:0 1 auto; }
  .ep-sys { font-weight:400; font-size:11px; color:var(--sapContent_LabelColor); flex-shrink:0; max-width:92px;
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .ep-meta { font-size:11px; color:var(--sapContent_LabelColor); margin-top:2px; display:flex; gap:8px; align-items:center; flex-wrap:wrap; }
  .chip { border-radius:8px; padding:0 7px; font-size:11px; line-height:17px; display:inline-block; }
  .chip.ch { background:color-mix(in srgb, var(--sapBrandColor,#0a6ed1) 8%, transparent); color:var(--sapBrandColor,#0a6ed1); }
  .chip.on { background:color-mix(in srgb, var(--sapSuccessColor,#107e3e) 10%, transparent); color:var(--sapSuccessColor,#107e3e); }
  .chip.off { background:color-mix(in srgb, var(--sapContent_LabelColor,#6a6d70) 12%, transparent); color:var(--sapContent_LabelColor,#6a6d70); }
  .chip.backlog { background:color-mix(in srgb, var(--sapWarningColor,#e9730c) 10%, transparent); color:var(--sapWarningColor,#e9730c); }
  /* content 视图 */
  .sub-pane { flex:1; min-height:0; display:flex; flex-direction:column;
    background:var(--sapList_Background); border:1px solid var(--sapList_BorderColor); border-radius:8px; padding:12px 14px; }
  .sub-hd { display:flex; justify-content:space-between; align-items:center; gap:10px; margin-bottom:8px; flex-wrap:wrap; }
  .sub-hd-title { font-size:15px; font-weight:600; color:var(--sapTitleColor); }
  .sub-hd-title small { font-weight:400; color:var(--sapContent_LabelColor); margin-left:6px; }
  .tbl-wrap { flex:1; min-height:0; overflow:hidden; display:flex; flex-direction:column; margin-top:10px; }
  .tbl-wrap cmx-revo-grid { display:flex; width:100%; flex:1 1 0%; min-width:0; min-height:0; flex-direction:column; }
  cmx-toolbar, cmx-filter-bar { display:block; }
  .f-ipt { min-width:130px; }
  .hint { font-size:12px; color:var(--sapContent_LabelColor); }
  /* ── 弹框内容区：分区卡片（对齐 activation-mapper 范式）── */
  .sec-card { border:1px solid var(--sapList_BorderColor); border-radius:8px; overflow:hidden;
    background:color-mix(in srgb, var(--sapBackgroundColor) 92%, #000 0%); }
  .sec-card + .sec-card { margin-top:12px; }
  .sec-head { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:10px 14px;
    border-bottom:1px solid var(--sapList_BorderColor);
    background:color-mix(in srgb, var(--sapBackgroundColor) 75%, #000 0%); }
  .sec-head h4 { margin:0; font-size:13px; font-weight:600; display:flex; align-items:center; gap:8px; color:var(--sapTitleColor); }
  .sec-head .num { width:18px; height:18px; border-radius:50%; background:var(--neo-cyan,#00b4d8); color:#fff;
    font-size:11px; display:inline-flex; align-items:center; justify-content:center; flex:0 0 auto; }
  .sec-hint { font-size:11px; color:var(--sapContent_LabelColor); }
  .sec-body { padding:14px 16px; display:flex; flex-direction:column; gap:12px; }
  /* 表单网格与字段 */
  .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(190px,1fr)); gap:12px 16px; }
  .form-grid.two { grid-template-columns:1fr 1fr; }
  .form-grid.three { grid-template-columns:repeat(3,minmax(0,1fr)); }
  .f-item { display:flex; flex-direction:column; gap:5px; min-width:0; }
  .f-item > label { font-size:12px; color:var(--sapContent_LabelColor); display:flex; align-items:center; gap:6px; }
  .f-item > label .req { color:var(--neo-red,#c53030); }
  .f-item .help { font-size:11px; color:var(--sapContent_LabelColor); opacity:.85; }
  .f-item ui5-input, .f-item ui5-select { width:100%; display:block; }
  .f-item cmx-combo-box { width:100%; display:block; }
  .chk-row { display:flex; flex-wrap:wrap; gap:6px 20px; align-items:center; }
  .rule-row { display:flex; gap:6px; align-items:center; padding:6px 9px; border-radius:6px;
    background:color-mix(in srgb, var(--sapBackgroundColor) 85%, #000 0%);
    border:1px solid var(--sapList_BorderColor); }
  .rule-row + .rule-row { margin-top:6px; }
  .rule-row .sc-del:hover { color:var(--sapNegativeColor,#bb0000); }
  `
}

// 弹框内容区共享样式：弹框是 body 级独立 shadowRoot，页面 styleCss() 作用不到，须内嵌注入各弹框。
function dialogCss() {
  return `
  .sm-dlg { display:flex; flex-direction:column; flex:1 1 auto; min-height:0; font-size:13px; }
  .sm-scroll { flex:1; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:12px; padding:2px 6px 8px 0; }
  .sm-dlg label { font-size:12px; color:var(--sapContent_LabelColor); }
  /* 分区卡片（对齐 activation-mapper 范式） */
  .sec-card { border:1px solid var(--sapList_BorderColor); border-radius:8px; overflow:hidden;
    background:color-mix(in srgb, var(--sapBackgroundColor) 92%, #000 0%); }
  .sec-card + .sec-card { margin-top:12px; }
  .sec-head { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:10px 14px;
    border-bottom:1px solid var(--sapList_BorderColor);
    background:color-mix(in srgb, var(--sapBackgroundColor) 75%, #000 0%); }
  .sec-head h4 { margin:0; font-size:13px; font-weight:600; display:flex; align-items:center; gap:8px; color:var(--sapTitleColor); }
  .sec-head .num { width:18px; height:18px; border-radius:50%; background:var(--neo-cyan,#00b4d8); color:#fff;
    font-size:11px; display:inline-flex; align-items:center; justify-content:center; flex:0 0 auto; }
  .sec-hint { font-size:11px; color:var(--sapContent_LabelColor); }
  .sec-body { padding:14px 16px; display:flex; flex-direction:column; gap:12px; }
  /* 表单网格与字段 */
  .form-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(190px,1fr)); gap:12px 16px; }
  .form-grid.two { grid-template-columns:1fr 1fr; }
  .form-grid.three { grid-template-columns:repeat(3,minmax(0,1fr)); }
  .f-item { display:flex; flex-direction:column; gap:5px; min-width:0; }
  .f-item > label { font-size:12px; color:var(--sapContent_LabelColor); display:flex; align-items:center; gap:6px; }
  .f-item > label .req { color:var(--neo-red,#c53030); }
  .f-item .help { font-size:11px; color:var(--sapContent_LabelColor); opacity:.85; }
  .f-item ui5-input, .f-item ui5-select { width:100%; display:block; }
  .f-item cmx-combo-box { width:100%; display:block; }
  .hint { font-size:12px; color:var(--sapContent_LabelColor); }
  .help { font-size:11px; color:var(--sapContent_LabelColor); opacity:.85; }
  .chk-row { display:flex; flex-wrap:wrap; gap:6px 20px; align-items:center; }
  .cond-box { display:flex; flex-direction:column; gap:6px; }
  .rule-row { display:flex; gap:6px; align-items:center; padding:6px 9px; border-radius:6px;
    background:color-mix(in srgb, var(--sapBackgroundColor) 85%, #000 0%);
    border:1px solid var(--sapList_BorderColor); }
  .rule-row + .rule-row { margin-top:6px; }
  .rule-row .sc-field { min-width:150px; flex:1 1 150px; }
  .rule-row .sc-val { min-width:120px; flex:1 1 120px; }
  .rule-row .sc-del:hover { color:var(--sapNegativeColor,#bb0000); }
  /* 关键：滚动容器的直接子卡（.sec-card 带 overflow:hidden，min-height:auto 解析为 0）
     若允许 flex 收缩会被压到容器高度——内容裁切且 scrollHeight 不增长（滚动失效根因） */
  .sm-scroll > * { flex-shrink:0; }
  /* 字段映射表格（新建/编辑订阅弹框共用） */
  .fm-tbl { width:100%; border-collapse:collapse; font-size:12.5px; }
  .fm-tbl th { text-align:left; font-size:12px; font-weight:600; color:var(--sapContent_LabelColor); padding:5px 8px;
    border-bottom:1px solid var(--sapList_BorderColor); background:color-mix(in srgb, var(--sapBackgroundColor) 75%, #000 0%); }
  .fm-tbl td { padding:4px 8px; border-bottom:1px solid var(--sapList_BorderColor); vertical-align:middle; }
  .fm-tbl ui5-input { width:100%; }
  .fm-tbl cmx-combo-box { width:100%; }
  .fm-tbl .c-mask { width:52px; text-align:center; }
  .fm-tbl .c-op { width:40px; text-align:center; }
  .fm-del:hover { color:var(--sapNegativeColor,#bb0000); }
  /* 配置块与勾选行（事件/过滤 + 字段映射共用骨架） */
  .blk { display:flex; flex-direction:column; gap:8px; align-items:stretch; }
  .blk + .blk { margin-top:12px; }
  label.ck { display:flex; gap:5px; align-items:center; font-size:12.5px; cursor:pointer; }
  input[type="checkbox"] { accent-color:var(--sapBrandColor,#0a6ed1); }
  cmx-combo-box { display:block; }
  `
}

// ── 数据加载 ────────────────────────────────────────────────────────────────
async function loadEndpoints(st) {
  const d = (await apiGet('/api/mdm/endpoints?page=1&pageSize=200', st.dbId)) || {}
  st.endpoints = d.list || []
  if (st.curEpId != null && !st.endpoints.some((e) => Number(e.id) === st.curEpId)) st.curEpId = null
}
function curEp(st) { return st.endpoints.find((e) => Number(e.id) === st.curEpId) || null }
async function loadSubs(st) {
  if (st.curEpId == null) { st.subs = []; st.subTotal = 0; return }
  const q = { endpointId: String(st.curEpId), page: String(st.subPage), pageSize: String(st.pageSize) }
  if (st.fDict) q.dictCode = st.fDict.trim()
  if (st.fActive !== '') q.active = st.fActive
  const d = (await apiGet(`/api/mdm/subscriptions?${new URLSearchParams(q)}`, st.dbId)) || {}
  st.subs = d.list || []
  st.subTotal = Number(d.total) || 0
}

// jsonb 容错（字符串则再解析）
function parseEvts(v) {
  if (Array.isArray(v)) return v
  if (typeof v === 'string' && v.trim()) { try { const p = JSON.parse(v); return Array.isArray(p) ? p : [] } catch { return [] } }
  return []
}
function parseObj(v) {
  if (v && typeof v === 'object') return v
  if (typeof v === 'string' && v.trim()) { try { const p = JSON.parse(v); return (p && typeof p === 'object') ? p : {} } catch { return {} } }
  return {}
}
function decorateSub(r) {
  const total = Number(r.stat_total_24h) || 0
  const ok = Number(r.stat_ok_24h) || 0
  const evts = parseEvts(r.event_types)
  return {
    ...r,
    success_text: total > 0 ? `${Math.round((ok / total) * 100)}%（${ok}/${total}）` : '-',
    active_text: r.active ? '● 启用' : '○ 停用',
    event_types_text: evts.length ? evts.join(' / ') : '全部',
    backlog_text: String(r.stat_backlog ?? 0),
  }
}

// ── 视图 HTML ───────────────────────────────────────────────────────────────
function explorerHtml(st) {
  return `<div class="epx">
    <div class="epx-hd"><span class="epx-title">目标端点</span>
      <ui5-button design="Emphasized" icon="add" id="smEpAdd">新建</ui5-button></div>
    <div class="ep-list" id="smEpList"></div>
  </div>`
}
function contentHtml(st) {
  return `<div class="pg">
    <div class="pg-head"><div class="pg-title">分发订阅管理</div></div>
    <div class="sub-pane">
      <div class="sub-hd">
        <div class="sub-hd-title" id="smSubTitle">—</div>
        <cmx-toolbar>
          <ui5-button design="Emphasized" icon="add" id="smSubAdd">新建订阅</ui5-button>
          <ui5-button design="Transparent" icon="edit" slot="actions" id="smEpEdit">编辑端点</ui5-button>
          <ui5-button design="Transparent" icon="paper-plane" slot="actions" id="smEpTest">测试端点</ui5-button>
          <ui5-button design="Transparent" icon="pause" slot="actions" id="smEpToggle">停用端点</ui5-button>
          <ui5-button design="Transparent" icon="refresh" slot="actions" id="smReload">刷新</ui5-button>
        </cmx-toolbar>
      </div>
      <cmx-filter-bar id="smFilter" show-search="false">
        <ui5-input id="smFDict" class="f-ipt" placeholder="字典（如 supplier）" value="${esc(st.fDict)}"></ui5-input>
        <ui5-select id="smFActive">
          <ui5-option value="" ${st.fActive === '' ? 'selected' : ''}>全部状态</ui5-option>
          <ui5-option value="true" ${st.fActive === 'true' ? 'selected' : ''}>启用</ui5-option>
          <ui5-option value="false" ${st.fActive === 'false' ? 'selected' : ''}>停用</ui5-option>
        </ui5-select>
        <ui5-button slot="actions" design="Default" icon="search" id="smSearch">查询</ui5-button>
        <ui5-button slot="actions" design="Transparent" icon="reset" id="smReset">重置</ui5-button>
      </cmx-filter-bar>
      <div class="tbl-wrap"><cmx-revo-grid id="smGrid"></cmx-revo-grid></div>
      <cmx-pager id="smPager" page-size="20" page-sizes="10,20,50,100"></cmx-pager>
    </div></div>`
}

// 侧栏端点卡片渲染（局部更新：只动 #smEpList innerHTML）
function renderEpList(st) {
  relocate(st)
  const box = st.explorerRoot && st.explorerRoot.isConnected && st.explorerRoot.querySelector('#smEpList')
  if (!box) return
  const nextHtml = st.endpoints.length ? st.endpoints.map((e) => {
    const backlog = Number(e.stat_backlog) || 0
    const name = e.name || e.target_sys
    return `<div class="ep-card ${Number(e.id) === st.curEpId ? 'sel' : ''}" data-ep="${e.id}" title="${esc(e.target_sys)}${e.name ? ' · ' + esc(e.name) : ''}">
      <div class="ep-name"><b>${esc(name)}</b>${e.name ? `<span class="ep-sys">${esc(e.target_sys)}</span>` : ''}</div>
      <div class="ep-meta">
        <span class="chip ch">${esc(e.channel)}</span>
        <span class="chip ${e.active ? 'on' : 'off'}">${e.active ? '● 启用' : '○ 停用'}</span>
        <span>${e.sub_count ?? 0} 订阅</span>
        ${backlog ? `<span class="chip backlog">积压 ${backlog}</span>` : ''}
      </div></div>`
  }).join('') : '<div class="hint" style="padding:8px 4px;">暂无端点——点上方「新建」先建目标端点</div>'
  if (box.__smLast === nextHtml) return
  box.__smLast = nextHtml
  box.innerHTML = nextHtml
  box.querySelectorAll('.ep-card').forEach((c) => {
    c.addEventListener('click', () => { st.curEpId = Number(c.dataset.ep); st.subPage = 1; renderEpList(st); reloadSubs(st) })
  })
}

// 内容区标题 + 工具栏按钮态
function renderSubHead(st) {
  relocate(st)
  const root = st.contentRoot && st.contentRoot.isConnected ? st.contentRoot : null; if (!root) return
  const ep = curEp(st)
  const t = root.querySelector('#smSubTitle')
  const titleHtml = ep
    ? `${esc(ep.name || ep.target_sys)} <small>· ${esc(ep.target_sys)} · ${esc(ep.channel)}${ep.active ? '' : ' · 已停用（全部投递暂停）'}</small>`
    : '请选择左侧栏目标端点'
  if (t && t.__smLast !== titleHtml) { t.__smLast = titleHtml; t.innerHTML = titleHtml }
  const toggle = root.querySelector('#smEpToggle')
  if (toggle) {
    const tt = ep && ep.active ? '停用端点' : '启用端点'
    if (toggle.textContent !== tt) { toggle.textContent = tt; toggle.icon = ep && ep.active ? 'pause' : 'play' }
  }
  for (const id of ['#smSubAdd', '#smEpEdit', '#smEpTest', '#smEpToggle']) {
    const el = root.querySelector(id); if (el) el.disabled = !ep
  }
}

// ── 订阅 grid（列模型 content bind 时一次建；数据 applyData 局部更新）────────
function buildListGrid(st) {
  const C = cmx()
  const wrap = st.contentRoot && st.contentRoot.querySelector('.tbl-wrap'); if (!wrap) return
  const grid = wrap.querySelector('cmx-revo-grid')
  if (!grid) return
  grid.setAttribute('data-cmx-fill-height', '')
  grid.setAttribute('data-cmx-options', '{"editable":false,"showTotals":false,"showRequiredMark":false}')
  grid.classList.add('cmx-grid-neo')
  st.grid = grid
  if (!(C.CmxColumnModel && C.CmxColumn)) return
  const cm = new C.CmxColumnModel({ datasetId: 'sm-list' })
  cm.setMembers([
    new C.CmxColumn({ id: 'name', caption: '名称', dataType: 'VARCHAR', width: '180px' }),
    new C.CmxColumn({ id: 'dict_code', caption: '字典', dataType: 'VARCHAR', width: '110px' }),
    new C.CmxColumn({ id: 'event_types_text', caption: '事件类型', dataType: 'VARCHAR', width: '150px' }),
    new C.CmxColumn({ id: 'active_text', caption: '状态', dataType: 'VARCHAR', width: '90px' }),
    new C.CmxColumn({ id: 'success_text', caption: '近24h成功率', dataType: 'VARCHAR', width: '130px' }),
    new C.CmxColumn({ id: 'backlog_text', caption: '积压', dataType: 'VARCHAR', width: '70px' }),
    new C.CmxColumn({ id: '_action', caption: '操作', dataType: 'VARCHAR', width: '330px', frozen: 'right', edit: { mode: 'readonly' },
      display: { mode: 'actions', actions: [
        { text: '编辑', actionRef: 'edit', icon: 'edit' },
        { text: '启用', actionRef: 'enable', icon: 'play', visible: (m) => !m.active },
        { text: '停用', actionRef: 'disable', icon: 'pause', visible: (m) => !!m.active },
        { text: '投递', actionRef: 'dispatch', icon: 'detail-view' },
        { text: '补发', actionRef: 'republish', icon: 'restart' },
        { text: '删除', actionRef: 'delete', icon: 'delete', variant: 'negative', visible: (m) => !m.active },
      ] } }),
  ])
  grid.setColumnModel(cm)
  grid.setOptions?.({ selectionMode: 'none', fillHeight: true, showRowIndex: true, showTotals: false, allowTextSelect: true, resize: true })
  // 动作格点击不在此绑：bindDelegates 已在 host shadowRoot 级委托 cmx-cell-link-click
  // （grid 直绑 + sr 委托 = composed 冒泡双触发，一次点击执行两次动作）
  st.gridBuilt = grid   // 列模型+监听就绪标记（applyData 据此判断是否需要重建/重试）
}

function applyData(st) {
  relocate(st)
  const root = st.contentRoot && st.contentRoot.isConnected ? st.contentRoot : null; if (!root) return
  const C = cmx()
  const curGrid = root.querySelector('#smGrid')
  if (!curGrid) return
  // grid 失联（宿主覆盖后是新元素）或列模型缺位（构建时组件库未就绪，buildListGrid 早退
  // 只置了 st.grid）→ 重建；重建后仍未就绪则退避重试（深链首帧 __cmxDataComp 可能晚到）
  if (curGrid !== st.grid || st.gridBuilt !== st.grid) { st.grid = null; buildListGrid(st) }
  const pager = root.querySelector('#smPager')
  if (pager) { pager.total = st.subTotal; pager.page = st.subPage; pager.pageSize = st.pageSize }
  const grid = st.grid
  if (!grid || st.gridBuilt !== grid || !C.CmxDataSet) {
    if ((st.__gridRetry || 0) < 30) { st.__gridRetry = (st.__gridRetry || 0) + 1; setTimeout(() => applyData(st), 300) }
    return
  }
  st.__gridRetry = 0
  const rows = st.subs.map(decorateSub)
  const ds = new C.CmxDataSet({ datasetId: 'sm-list' }); ds.setRows(rows); grid.setDataSet(ds)
  grid.refreshLayout?.()
}

async function reloadAll(st) { await loadEndpoints(st); renderEpList(st); await reloadSubs(st) }
async function reloadSubs(st) { await loadSubs(st); renderSubHead(st); applyData(st) }

// ── 行操作 ─────────────────────────────────────────────────────────────────
async function doAction(st, act, row) {
  const M = cmx()
  const id = Number(row.id)
  const label = row.name || row.dict_code || `#${id}`
  try {
    if (act === 'edit') { openSubEditDialog(st, st.subs.find((r) => Number(r.id) === id) || row) }
    else if (act === 'enable' || act === 'disable') {
      const to = act === 'enable'
      const ok = await M.cmxConfirm?.({ title: to ? '启用订阅' : '停用订阅', intent: (to ? 'normal' : 'danger'),
        confirmText: to ? '启用' : '停用',
        message: to ? `确认启用订阅「${label}」？` : `确认停用订阅「${label}」？停用期间该订阅不再产生新投递（存量积压同步停止重试）。` })
      if (ok === false) return
      await apiPost('/api/mdm/subscriptions/set-active', { id, active: to }, st.dbId)
      showCmxToast(to ? `订阅「${label}」已启用` : `订阅「${label}」已停用`)
      await reloadAll(st)
    }
    else if (act === 'dispatch') {
      openTab(st.host, st, `分发监控·${label}`, 'portal.mdm.dispatch-monitor',
        { subscriptionId: id, subscriptionName: label, ...coordCtx(st) })
    }
    else if (act === 'republish') { openPublishDialog(st, { subscriptionId: id, dictCode: row.dict_code, title: label }) }
    else if (act === 'delete') {
      const ok = await M.cmxConfirm?.({
        title: '删除订阅', intent: 'danger',
        message: `确认删除订阅「${label}」？删除后其投递流水将保留审计，不可恢复。`,
      })
      if (ok === false) return
      await apiPost('/api/mdm/subscriptions/delete', { id }, st.dbId)
      showCmxToast(`订阅「${label}」已删除（投递流水已保留）`)
      await reloadAll(st)
    }
  } catch (e) { M.cmxError?.(`操作失败：${e.message}`) }
}

// ── 补发小对话框（POST /mdm/publish 重建 pending 投递实例）────────────────
function openPublishDialog(st, preset) {
  const M = cmx()
  if (!customElements.get('cmx-floating-dialog')) { M.cmxError?.('弹框组件未就绪'); return }
  const dlg = document.createElement('cmx-floating-dialog')
  dlg.configure({
    title: `手动补发${preset && preset.title ? `·${preset.title}` : ''}`, icon: 'restart',
    confirmText: '补发', cancelText: '取消', dialogWidth: '440px',
    beforeClose: async (ctx) => {
      if (ctx.action !== 'confirm') return true
      const body = {}
      const subId = (wrap.querySelector('#pbSubId')?.value || '').trim()
      if (subId) body.subscriptionId = Number(subId)
      const dict = (wrap.querySelector('#pbDict')?.value || '').trim()
      if (dict) body.dictCode = dict
      const from = (wrap.querySelector('#pbFrom')?.value || '').trim()
      if (from !== '') body.fromSeq = Number(from)
      const to = (wrap.querySelector('#pbTo')?.value || '').trim()
      if (to !== '') body.toSeq = Number(to)
      body.force = !!wrap.querySelector('#pbForce')?.checked
      if (body.subscriptionId != null && !Number.isFinite(body.subscriptionId)) { M.cmxWarn?.('订阅 id 须为数字'); return false }
      if ((body.fromSeq != null && !Number.isFinite(body.fromSeq)) || (body.toSeq != null && !Number.isFinite(body.toSeq))) { M.cmxWarn?.('seq 范围须为数字'); return false }
      if (body.subscriptionId == null && !body.dictCode) { M.cmxWarn?.('请填写订阅 id 或字典（二选一）'); return false }
      try {
        const d = (await apiPost('/api/mdm/publish', body, st.dbId)) || {}
        const n = Number(d.created) || 0
        showCmxToast(n > 0 ? `补发完成：已创建 ${n} 条待投递实例` : '没有匹配的事件需要补发（停用订阅跳过；已投递且未勾选 force 的会跳过）')
        return true
      } catch (e) { M.cmxError?.(`补发失败：${e.message}`); return false }
    },
  })
  const wrap = document.createElement('div')
  wrap.style.cssText = 'display:flex;flex-direction:column;gap:10px;font-size:13px;'
  wrap.innerHTML = `
    <div class="hint">按订阅/字典 + 事件 seq 范围重建待投递实例（上限 5000 行）。不勾 force 时已送达的不重发；停用订阅/端点自动跳过。</div>
    <div style="display:flex;flex-direction:column;gap:4px;"><label style="font-size:12px;color:var(--sapContent_LabelColor);">订阅 id</label>
      <ui5-input id="pbSubId" placeholder="数字 id（可从列表行查看）" value="${esc(preset && preset.subscriptionId ? String(preset.subscriptionId) : '')}"></ui5-input></div>
    <div style="display:flex;flex-direction:column;gap:4px;"><label style="font-size:12px;color:var(--sapContent_LabelColor);">字典</label>
      <ui5-input id="pbDict" placeholder="如 supplier（与订阅 id 至少填一项）" value="${esc(preset && preset.dictCode ? String(preset.dictCode) : '')}"></ui5-input></div>
    <div style="display:flex;gap:10px;">
      <div style="flex:1;display:flex;flex-direction:column;gap:4px;"><label style="font-size:12px;color:var(--sapContent_LabelColor);">起始 seq</label>
        <ui5-input id="pbFrom" placeholder="可空"></ui5-input></div>
      <div style="flex:1;display:flex;flex-direction:column;gap:4px;"><label style="font-size:12px;color:var(--sapContent_LabelColor);">截止 seq</label>
        <ui5-input id="pbTo" placeholder="可空"></ui5-input></div>
    </div>
    <ui5-checkbox id="pbForce" text="强制重发已送达（force）"></ui5-checkbox>`
  dlg.setContent(wrap)
  document.body.appendChild(dlg)
  dlg.openModal().then(() => dlg.remove())
}

// ── 事件类型 / 过滤行 / 秘钥（共享片段）────────────────────────────────────
const EVT_TYPES = [
  { k: 'created', label: 'created 新增' },
  { k: 'updated', label: 'updated 变更' },
  { k: 'merged', label: 'merged 合并' },
]
const OPS = [['eq', '等于'], ['ne', '不等于'], ['in', '属于(逗号分隔)'], ['like', '模糊']]

// 32 位随机 hex（签名秘钥）
function randomSecret() {
  const buf = new Uint8Array(16)
  ;(globalThis.crypto || {}).getRandomValues ? crypto.getRandomValues(buf) : buf.forEach((_, i) => { buf[i] = Math.floor(Math.random() * 256) })
  return Array.from(buf, (b) => b.toString(16).padStart(2, '0')).join('')
}

function condRowHtml(c) {
  return `<div class="rule-row">
    <ui5-input class="sc-field" placeholder="字段名（可输关键字）" show-suggestions value="${esc(c.field)}" style="min-width:150px;flex:1 1 150px;"></ui5-input>
    <ui5-select class="sc-op" style="min-width:130px;">
      ${OPS.map(([v, t]) => `<ui5-option value="${v}" ${(c.op || 'eq') === v ? 'selected' : ''}>${t}</ui5-option>`).join('')}
    </ui5-select>
    <ui5-input class="sc-val" placeholder="值" value="${esc(c.value)}" style="min-width:120px;flex:1 1 120px;"></ui5-input>
    <ui5-button icon="add" class="sc-add" design="Transparent" title="加一行"></ui5-button>
    <ui5-button icon="delete" class="sc-del" design="Transparent" title="删本行"></ui5-button>
  </div>`
}

// 过滤字段候选与字段映射源字段下拉的共同数据源（/api/dct/meta with_props；无坐标/失败静默降级纯手输）。
// 单条弹框同一时刻只有一个活动字典：metaFields 即当前字典字段，currentFields() 是水合唯一真源
// （避免调用方固化 dictCode 参数——新建态选完字典后再 addRow 水合会取到旧值）。
function makeMetaLoader(st, getWrap, refresh) {
  let metaFields = []
  async function loadMetaFields(dictCode) {
    if (!dictCode || !st.coord || !(st.coord.domain && st.coord.application)) { metaFields = []; return }
    try {
      const m = await apiGet(`/api/dct/meta?${coordQs(st, { dict: dictCode })}&with_props=true`, st.dbId)
      metaFields = ((m && m.columns) || []).map((c) => ({ name: c.name, caption: (c.caption && (c.caption.zh_CN || c.caption)) || c.name }))
    } catch { metaFields = [] }
    refresh()
  }
  function currentFields() { return metaFields.length ? metaFields : null }
  function refreshSuggestions() {
    const wrap = getWrap(); if (!wrap) return
    wrap.querySelectorAll('.sc-field').forEach((input) => {
      Array.from(input.children).forEach((c) => { if (c.tagName && c.tagName.toLowerCase() === 'ui5-suggestion-item') c.remove() })
      const q = String(input.value || '').toLowerCase()
      for (const f of metaFields) {
        if (q && !f.name.toLowerCase().includes(q) && !String(f.caption).toLowerCase().includes(q)) continue
        const o = document.createElement('ui5-suggestion-item')
        o.setAttribute('text', f.name === f.caption ? f.name : `${f.name} · ${f.caption}`)
        o.dataset.field = f.name
        input.appendChild(o)
      }
    })
  }
  function bindSuggest(input) {
    if (!input) return
    input.addEventListener('input', refreshSuggestions)
    input.addEventListener('suggestion-item-select', (ev) => {
      const it = ev.detail && ev.detail.item
      const v = (it && (it.dataset.field || it.getAttribute('data-field'))) || ''
      if (v) input.value = v
    })
  }
  function bindCondRow(row) {
    bindSuggest(row.querySelector('.sc-field'))
    row.querySelector('.sc-add')?.addEventListener('click', () => {
      const nr = document.createElement('div'); nr.innerHTML = condRowHtml({ field: '', op: 'eq', value: '' })
      const el = nr.firstElementChild; row.after(el); bindCondRow(el)
    })
    row.querySelector('.sc-del')?.addEventListener('click', () => {
      const box = row.closest('.cond-rows')
      if (box && box.children.length > 1) row.remove()
      else { row.querySelectorAll('input,select').forEach((el) => { el.value = '' }) }
    })
  }
  return { loadMetaFields, currentFields, refreshSuggestions, bindCondRow, bindSuggest }
}

// ── 订阅配置共享片段（新建/编辑订阅同一弹框，分区卡共用同一套渲染/绑定/读取）──
// 事件与过滤块。cfg = {eventTypes:[], conditions:[]}（分区卡 sec-head 已带标题）。
function evtFilterHtml(cfg) {
  const evts = (cfg && cfg.eventTypes) || []
  const conds = (cfg && cfg.conditions && cfg.conditions.length) ? cfg.conditions : [{}]
  return `<div class="blk">
    <div class="chk-row">
      ${EVT_TYPES.map((e) => `<label class="ck"><input type="checkbox" data-evt="${e.k}" ${!evts.length || evts.includes(e.k) ? 'checked' : ''}>${e.label}</label>`).join('')}
      <span class="hint">全不选 = 订阅全部事件类型</span>
    </div>
    <div class="cond-box">
      <div class="hint">行级过滤（字段取值为记录快照字段，多条件 AND；in 的值用逗号分隔）</div>
      <div class="cond-rows">${conds.map(condRowHtml).join('')}</div>
      <div><ui5-button icon="add" class="cond-add" design="Transparent">加条件</ui5-button></div>
    </div>
  </div>`
}
function bindEvtFilter(scope, meta) {
  scope.querySelectorAll('.cond-rows .rule-row').forEach(meta.bindCondRow)
  scope.querySelector('.cond-add')?.addEventListener('click', () => {
    const box = scope.querySelector('.cond-rows'); if (!box) return
    const nr = document.createElement('div'); nr.innerHTML = condRowHtml({ field: '', op: 'eq', value: '' })
    const el = nr.firstElementChild; box.appendChild(el); meta.bindCondRow(el); el.querySelector('.sc-field')?.focus?.()
  })
}
function readEvtFilter(scope) {
  const evts = []
  scope.querySelectorAll('[data-evt]').forEach((ck) => { if (ck.checked) evts.push(ck.dataset.evt) })
  const conditions = []
  scope.querySelectorAll('.cond-rows .rule-row').forEach((row) => {
    const f = ((row.querySelector('.sc-field') || {}).value || '').trim()
    const op = (row.querySelector('.sc-op') || {}).value || 'eq'
    const v = ((row.querySelector('.sc-val') || {}).value || '').trim()
    if (f && v !== '') conditions.push({ field: f, op, value: v })
  })
  return { evts, conditions }
}

// field_map 对象 ↔ 表格行。include 非空 → 仅投递列出字段；include 缺省但有
// rename/mask → 投递全部字段（行只承担改名/脱敏）。行集合 = include ∪ rename ∪ mask 键。
function fmRowsFromMap(fmObj) {
  const m = fmObj || {}
  const include = Array.isArray(m.include) ? m.include : null
  const rename = (m.rename && typeof m.rename === 'object') ? m.rename : {}
  const mask = Array.isArray(m.mask) ? m.mask : []
  const keys = []
  if (include) keys.push(...include)
  for (const k of Object.keys(rename)) if (!keys.includes(k)) keys.push(k)
  for (const k of mask) if (!keys.includes(k)) keys.push(k)
  return {
    onlyListed: !!include || keys.length === 0,   // 空配置默认勾「仅投递」（加行即生效）
    rows: keys.map((k) => ({ src: k, dst: rename[k] || '', mask: mask.includes(k) })),
  }
}
function fmRowHtml(r) {
  // 源字段为水合槽位：bindFieldMap/hydrateFieldCombos 用 cmx-combo-box（下拉 + 输入过滤）替换
  return `<tr class="fm-row">
    <td><div class="fm-src-slot" data-v="${esc(r.src || '')}"></div></td>
    <td><ui5-input class="fm-dst" placeholder="留空 = 原字段名" value="${esc(r.dst || '')}"></ui5-input></td>
    <td class="c-mask"><input type="checkbox" class="fm-mask" ${r.mask ? 'checked' : ''} title="投递时脱敏"></td>
    <td class="c-op"><ui5-button icon="delete" class="fm-del" design="Transparent" title="删本行"></ui5-button></td>
  </tr>`
}
// 字段下拉（list 模式自带输入过滤；数据 = DCT meta 字段清单，显示 name · 中文caption）
function applyFieldsToCombo(combo, C, fields) {
  if (!combo || typeof combo.setDataSet !== 'function' || !C.CmxDataSet || !fields) return
  const ds = new C.CmxDataSet({ datasetId: 'sm-fields' })
  ds.setRows(fields.map((f) => ({ id: f.name, name: f.name === f.caption ? f.name : `${f.name} · ${f.caption}` })))
  combo.setDataSet(ds)
}
function hydrateFieldCombos(scope, st, meta) {
  const C = cmx()
  const fields = meta.currentFields()
  if (!fields || !fields.length || !C.CmxDataSet) return
  // scope 兼容两种粒度：容器（批量水合）或 .fm-row 行自身（addRow 单行水合）
  const rows = (scope.classList && scope.classList.contains('fm-row')) ? [scope] : Array.from(scope.querySelectorAll('.fm-row'))
  rows.forEach((tr) => {
    const slot = tr.querySelector('.fm-src-slot')
    if (slot) {
      const combo = document.createElement('cmx-combo-box')
      combo.classList.add('fm-src')
      combo.setMode('list')
      combo.setPlaceholder('选择源字段（可输入过滤）')
      applyFieldsToCombo(combo, C, fields)
      const v = slot.dataset.v || ''
      slot.replaceWith(combo)
      if (v) { try { combo.setValue(v) } catch { /* id 不在候选中：置空，读取时走槽位兜底 */ } }
    } else {
      applyFieldsToCombo(tr.querySelector('.fm-src'), C, fields)
    }
  })
}
function fieldMapHtml(fmObj) {
  const { onlyListed, rows } = fmRowsFromMap(fmObj)
  return `<div class="blk">
    <label class="ck"><input type="checkbox" class="fm-only" ${onlyListed ? 'checked' : ''}>仅投递下列字段
      <span class="hint">（不勾 = 投递全部字段，表行只用于改名/脱敏）</span></label>
    <table class="fm-tbl">
      <thead><tr><th style="width:38%;">源字段</th><th>输出字段名</th><th class="c-mask">脱敏</th><th class="c-op"></th></tr></thead>
      <tbody>${rows.map(fmRowHtml).join('')}</tbody>
    </table>
    <div><ui5-button icon="add" class="fm-add" design="Transparent">添加字段</ui5-button></div>
    <div class="hint">不添加任何字段 = 原样投递全部字段</div>
  </div>`
}
function bindFieldMap(scope, st, meta) {
  const tbody = scope.querySelector('.fm-tbl tbody'); if (!tbody) return
  const bindRow = (tr) => {
    tr.querySelector('.fm-del')?.addEventListener('click', () => tr.remove())
  }
  scope.querySelectorAll('.fm-row').forEach(bindRow)
  hydrateFieldCombos(scope, st, meta)
  scope.querySelector('.fm-add')?.addEventListener('click', () => {
    const nr = document.createElement('tbody'); nr.innerHTML = fmRowHtml({})
    const el = nr.firstElementChild; tbody.appendChild(el)
    bindRow(el)
    hydrateFieldCombos(el, st, meta)
    const combo = el.querySelector('.fm-src')
    if (combo) { try { combo.focus?.() } catch { /* 无 focus 方法时跳过 */ } }
    else el.querySelector('.fm-src-slot')?.focus?.()
  })
}
function readFieldMap(scope) {
  const rows = []
  scope.querySelectorAll('.fm-row').forEach((tr) => {
    const srcEl = tr.querySelector('.fm-src')
    let src = ''
    if (srcEl) src = (typeof srcEl.getValue === 'function' ? srcEl.getValue() : (srcEl.value || '')) || ''
    if (!src) {
      // 未水合（meta 拉取失败/组件库缺 CmxDataSet）时槽位保留原值兜底，避免存量配置被静默清空
      const slot = tr.querySelector('.fm-src-slot')
      if (slot) src = slot.dataset.v || ''
    }
    if (!src || rows.some((r) => r.src === src)) return
    rows.push({ src, dst: ((tr.querySelector('.fm-dst') || {}).value || '').trim(), mask: !!(tr.querySelector('.fm-mask') || {}).checked })
  })
  if (!rows.length) return null
  const out = {}
  if (!!(scope.querySelector('.fm-only') || {}).checked) out.include = rows.map((r) => r.src)
  const rename = {}
  rows.forEach((r) => { if (r.dst) rename[r.src] = r.dst })
  if (Object.keys(rename).length) out.rename = rename
  const mask = rows.filter((r) => r.mask).map((r) => r.src)
  if (mask.length) out.mask = mask
  return Object.keys(out).length ? out : null
}

// ── 端点编辑弹框 ───────────────────────────────────────────────────────────
function openEpEditDialog(st, ep) {
  const C = cmx(); const M = C
  if (!customElements.get('cmx-floating-dialog')) { M.cmxError?.('弹框组件未就绪'); return }
  const isNew = !ep
  const cfg = parseObj(ep && ep.channel_config)
  const fm = {
    id: ep ? Number(ep.id) : null,
    name: (ep && ep.name) || '',
    target_sys: (ep && ep.target_sys) || '',
    channel: (ep && ep.channel) || 'webhook',
    description: (ep && ep.description) || '',
    active: ep ? !!ep.active : true,
    url: cfg.url || '',
    secret: cfg.secret || '',
    timeoutMs: (ep && ep.timeout_ms) != null ? ep.timeout_ms : (cfg.timeout_ms != null ? cfg.timeout_ms : 10000),
    consumerId: cfg.consumerId || cfg.consumer_id || '',
    retryMax: (ep && ep.retry_max) != null ? ep.retry_max : 8,
    batchSize: (ep && ep.batch_size) != null ? ep.batch_size : 50,
  }
  const dlg = document.createElement('cmx-floating-dialog')
  dlg.configure({
    title: isNew ? '新建目标端点' : `编辑端点·${fm.name || fm.target_sys}`, icon: 'settings',
    dialogWidth: '640px', dialogHeight: '78vh',
    confirmText: '保存', cancelText: '取消',
    beforeClose: (ctx) => {
      if (ctx.action !== 'confirm') return true
      doSave(false)   // 异步保存；失败由 doSave 内 cmxError 提示，此处保持弹框开着
      return false
    },
  })
  const wrap = document.createElement('div')
  wrap.style.cssText = 'flex:1;min-height:0;padding:6px 18px 14px;display:flex;flex-direction:column;'
  wrap.innerHTML = `<style>${dialogCss()}
    cmx-toolbar { display:block; }
  </style>
  <div class="sm-dlg">
   <div class="sm-scroll">
    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">1</span>基本信息</h4><span class="sec-hint">名称用于列表展示，建议中文</span></div>
      <div class="sec-body">
        <div class="form-grid two">
          <div class="f-item"><label>名称</label><ui5-input id="smName" placeholder="如：ERP 生产端点" value="${esc(fm.name)}"></ui5-input></div>
          <div class="f-item"><label><span class="req">*</span>目标系统</label><ui5-input id="smTarget" placeholder="如 erp-prod" value="${esc(fm.target_sys)}"></ui5-input></div>
          <div class="f-item"><label>通道</label>
            <ui5-select id="smChannel">${st.channels.map((c) => `<ui5-option value="${esc(c.type)}" ${fm.channel === c.type ? 'selected' : ''}>${esc(c.label || c.type)}</ui5-option>`).join('')}</ui5-select></div>
          <div class="f-item"><label>描述</label><ui5-input id="smDesc" placeholder="用途说明（可空）" value="${esc(fm.description)}"></ui5-input></div>
        </div>
        <ui5-checkbox id="smActive" text="启用（停用 = 级联停其下全部订阅，含存量积压）" ${fm.active ? 'checked' : ''}></ui5-checkbox>
      </div>
    </div>

    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">2</span>通道配置</h4><span class="sec-hint">该端点全部订阅共用，密钥轮换只改这里</span></div>
      <div class="sec-body"><div id="smChannelBox"></div></div>
    </div>

    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">3</span>投递策略</h4><span class="sec-hint">重试超限进入死信，可在「分发监控」批量处理</span></div>
      <div class="sec-body">
        <div class="form-grid three">
          <div class="f-item"><label>最大重试次数</label><ui5-input id="smRetry" value="${esc(String(fm.retryMax))}"></ui5-input></div>
          <div class="f-item"><label>批量大小</label><ui5-input id="smBatch" value="${esc(String(fm.batchSize))}"></ui5-input></div>
        </div>
      </div>
    </div>
   </div>
  </div>`
  dlg.setContent(wrap, { padding: false })
  document.body.appendChild(dlg)
  // 标准底座：原生 confirm=保存（beforeClose 接管）；左侧 extra 槽放 保存并测试 / 删除端点
  // （getFooterExtra 依赖 shadowRoot，须在 appendChild 连接之后取）
  const epExtra = dlg.getFooterExtra()
  const saveTestBtn = document.createElement('ui5-button')
  saveTestBtn.textContent = '保存并测试'
  saveTestBtn.addEventListener('click', () => doSave(true))
  epExtra.appendChild(saveTestBtn)
  if (!isNew && !fm.active) {
    const delBtn = document.createElement('ui5-button')
    delBtn.design = 'Negative'
    delBtn.textContent = '删除端点'
    delBtn.addEventListener('click', () => doDelete())
    epExtra.appendChild(delBtn)
  }

  function renderChannelBox() {
    const box = wrap.querySelector('#smChannelBox'); if (!box) return
    if (fm.channel === 'webhook') {
      box.innerHTML = `<div class="form-grid two">
        <div class="f-item" style="grid-column:1/-1;"><label><span class="req">*</span>URL</label><ui5-input id="smUrl" placeholder="https://erp.example.com/api/cmx" value="${esc(fm.url)}"></ui5-input></div>
        <div class="f-item"><label><span class="req">*</span>签名秘钥</label>
          <div style="display:flex;gap:6px;align-items:center;">
            <ui5-input id="smSecret" style="flex:1;" value="${esc(fm.secret)}" ${isNew ? '' : 'placeholder="*** 表示未变更"'}></ui5-input>
            <ui5-button icon="initialize" id="smGenSecret" design="Transparent" title="随机生成 32 位 hex">随机生成</ui5-button>
          </div>
          <span class="help">编辑时 *** 表示沿用库内原秘钥；重填则覆盖（对其下全部订阅即时生效）</span></div>
        <div class="f-item"><label>超时（ms）</label><ui5-input id="smTimeout" value="${esc(String(fm.timeoutMs))}"></ui5-input></div>
      </div>`
      box.querySelector('#smGenSecret')?.addEventListener('click', () => {
        const s = box.querySelector('#smSecret')
        if (s) s.value = randomSecret()
      })
    } else if (fm.channel === 'rest_pull') {
      box.innerHTML = `<div class="f-item"><label>消费者标识（consumerId）</label><ui5-input id="smConsumer" placeholder="如 wms-consumer" value="${esc(fm.consumerId)}"></ui5-input>
        <span class="help">rest_pull 仅登记拉取消费者（游标见「分发监控」），不主动投递、无需连通性测试</span></div>`
    } else {
      box.innerHTML = `<div class="hint">通道 ${esc(fm.channel)} 未启用，仅登记配置，不会产生投递。</div>`
    }
  }
  renderChannelBox()
  wrap.querySelector('#smChannel')?.addEventListener('change', (e) => {
    fm.channel = e.target.value || 'webhook'
    renderChannelBox()
  })

  function collect() {
    const val = (sel) => ((wrap.querySelector(sel) || {}).value || '').trim()
    const name = val('#smName')
    const target = val('#smTarget')
    if (!target) return { err: '请填写目标系统' }
    const channel = (wrap.querySelector('#smChannel') || {}).value || fm.channel || 'webhook'
    let cc = {}
    if (channel === 'webhook') {
      const url = val('#smUrl'); const secret = val('#smSecret')
      if (!url) return { err: 'webhook 通道需填写 URL' }
      if (!secret) return { err: 'webhook 通道需填写签名秘钥' }
      cc = { url, secret, timeout_ms: Number(val('#smTimeout')) || 10000 }
    } else if (channel === 'rest_pull') {
      cc = { consumerId: val('#smConsumer') }
    }
    return {
      body: {
        id: fm.id || undefined,
        name, target_sys: target, channel,
        description: val('#smDesc'),
        active: !!(wrap.querySelector('#smActive') || {}).checked,
        channel_config: cc,
        retry_max: Number(val('#smRetry')) || 8,
        timeout_ms: (cc && cc.timeout_ms) || 10000,
        batch_size: Number(val('#smBatch')) || 50,
      },
    }
  }

  let saving = false
  async function doSave(withTest) {
    if (saving) return
    const r = collect()
    if (r.err) { M.cmxWarn?.(r.err); return }
    saving = true
    try {
      const saved = (await apiPost('/api/mdm/endpoints', r.body, st.dbId)) || {}
      const newId = Number(saved.id) || fm.id
      if (saved.conflict && isNew) M.cmxWarn?.(`已存在同 (目标系统, 通道) 的端点（id=${(saved.conflict.endpoints || [])[0]?.id}），请确认是否应复用`)
      showCmxToast(isNew ? `端点已创建（#${newId}）` : '端点已保存（凭证对其下全部订阅即时生效）')
      if (withTest) {
        try {
          const t = (await apiPost('/api/mdm/endpoints/test', { id: newId }, st.dbId)) || {}
          if (t.ok) showCmxToast(`测试通过（${t.latencyMs ?? '-'} ms）`)
          else M.cmxError?.(`测试失败：${t.detail || '未知原因'}`)
        } catch (e) { M.cmxError?.(`测试失败：${e.message}`) }
      }
      dlg.close('confirm', { force: true })
      st.curEpId = newId
      await reloadAll(st)
    } catch (e) { M.cmxError?.(`保存失败：${e.message}`) } finally { saving = false }
  }

  async function doDelete() {
    const ok = await M.cmxConfirm?.({
      title: '删除端点', intent: 'danger',
      message: `确认删除端点「${fm.name || fm.target_sys}」？其下停用订阅将连带删除（投递流水保留审计）。`,
    })
    if (ok === false) return
    try {
      await apiPost('/api/mdm/endpoints/delete', { id: fm.id }, st.dbId)
      showCmxToast('端点已删除')
      dlg.close('confirm', { force: true })
      st.curEpId = null
      await reloadAll(st)
    } catch (e) { M.cmxError?.(`删除失败：${e.message}`) }
  }
  dlg.openModal().then(() => dlg.remove())
}

// ── 订阅新建/编辑弹框（同一弹框两态：名称/事件/过滤/映射/启停；字典仅新建可选）──
// sub 传 null = 新建：固定挂当前选中端点，字典下拉（排除该端点已订阅），选字典即水合字段候选。
async function openSubEditDialog(st, sub) {
  const C = cmx(); const M = C
  if (!customElements.get('cmx-floating-dialog')) { M.cmxError?.('弹框组件未就绪'); return }
  const isNew = !sub
  const ep = curEp(st)
  if (isNew && !ep) { M.cmxWarn?.('请先在左侧栏选择目标端点'); return }
  const fConds = parseObj(sub && sub.filter).conditions
  const fm = {
    id: sub ? Number(sub.id) : null,
    endpoint_id: sub ? Number(sub.endpoint_id) : Number(ep.id),
    dict_code: (sub && sub.dict_code) || '',
    name: (sub && sub.name) || '',
    description: (sub && sub.description) || '',
    active: sub ? !!sub.active : true,
    eventTypes: parseEvts(sub && sub.event_types),
    conditions: (Array.isArray(fConds) ? fConds : []).map((c) => ({ field: c.field || '', op: c.op || 'eq', value: c.value == null ? '' : String(c.value) })),
    fmMap: parseObj(sub && sub.field_map),   // 结构化 field_map（表格行渲染）
  }
  const dlg = document.createElement('cmx-floating-dialog')
  dlg.configure({
    title: isNew ? `新建订阅·${ep.name || ep.target_sys}` : `编辑订阅·${fm.name || fm.dict_code}`, icon: 'settings',
    dialogWidth: '720px', dialogHeight: '80vh',
    confirmText: '保存', cancelText: '取消',
    beforeClose: (ctx) => {
      if (ctx.action !== 'confirm') return true
      doSaveSub()   // 异步保存；失败保持弹框开着（doSaveSub 内提示）
      return false
    },
  })
  const wrap = document.createElement('div')
  wrap.style.cssText = 'flex:1;min-height:0;padding:6px 18px 14px;display:flex;flex-direction:column;'
  wrap.innerHTML = `<style>${dialogCss()}</style>
  <div class="sm-dlg">
   <div class="sm-scroll">
    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">1</span>基本信息</h4><span class="sec-hint">凭证与投递策略在所属端点上统一维护</span></div>
      <div class="sec-body">
        <div class="form-grid two">
          <div class="f-item"><label>名称</label><ui5-input id="smName" placeholder="缺省自动：<字典> → <端点>" value="${esc(fm.name)}"></ui5-input></div>
          <div class="f-item"><label><span class="req">*</span>字典</label>${isNew ? '<cmx-combo-box id="smDict"></cmx-combo-box>' : `<ui5-input value="${esc(fm.dict_code)}" readonly></ui5-input>`}</div>
          <div class="f-item" style="grid-column:1/-1;"><label>描述</label><ui5-input id="smDesc" placeholder="用途说明（可空）" value="${esc(fm.description)}"></ui5-input></div>
        </div>
        <ui5-checkbox id="smActive" text="启用（停用后不再产生新投递，存量积压同步停止重试）" ${fm.active ? 'checked' : ''}></ui5-checkbox>
      </div>
    </div>

    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">2</span>事件与过滤</h4><span class="sec-hint">多条件 AND；in 的值用逗号分隔</span></div>
      <div class="sec-body">${evtFilterHtml(fm)}</div>
    </div>

    <div class="sec-card">
      <div class="sec-head"><h4><span class="num">3</span>字段映射</h4><span class="sec-hint">留空 = 原样投递全部字段</span></div>
      <div class="sec-body">${fieldMapHtml(fm.fmMap)}</div>
    </div>
   </div>
  </div>`
  dlg.setContent(wrap, { padding: false })
  document.body.appendChild(dlg)

  const combo = wrap.querySelector('#smDict')
  let dictVal = fm.dict_code
  const meta = makeMetaLoader(st, () => wrap, () => meta.refreshSuggestions())
  if (combo && C.CmxDataSet) {
    // 字典中文名后台预取（loadLookups）：打开前等它就绪，保证下拉首帧即「code · 中文名」
    if (st.dictNamesReady) { try { await st.dictNamesReady } catch { /* 降级纯 code */ } }
    combo.setMode('list')
    combo.setPlaceholder('选择字典（可输入过滤）')
    const ds = new C.CmxDataSet({ datasetId: 'sm-dicts' })
    ds.setRows(st.dicts.map((d) => ({ id: d, name: dictLabel(st, d) })))
    combo.setDataSet(ds)
    // 新建态候选排除该端点已订阅字典（(endpoint, dict) 唯一，重复建必报错；失败降级全量候选）
    // setDataSet 是一次性拷贝同步（_syncInnerDsFromExternal），ds 后续变更须重绑 combo 才生效
    apiGet(`/api/mdm/subscriptions?endpointId=${fm.endpoint_id}&page=1&pageSize=200`, st.dbId)
      .then((d) => {
        const taken = new Set(((d && d.list) || []).map((s) => s.dict_code))
        const avail = st.dicts.filter((d2) => !taken.has(d2))
        if (!avail.length) combo.setPlaceholder('该端点已订阅全部可订阅字典')
        ds.setRows(avail.map((d2) => ({ id: d2, name: dictLabel(st, d2) })))
        combo.setDataSet(ds)
      }).catch(() => {})
    combo.addEventListener('cmx-combo-value-change', (e) => {
      dictVal = (e.detail && e.detail.id) || ''
      // 字典选定即联动：DCT meta 字段清单 → 过滤行建议（loadMetaFields 内 refresh）+ 源字段下拉水合
      if (dictVal) meta.loadMetaFields(dictVal).then(() => hydrateFieldCombos(wrap, st, meta))
    })
  }
  bindEvtFilter(wrap, meta)
  bindFieldMap(wrap, st, meta)
  if (fm.dict_code) meta.loadMetaFields(fm.dict_code).then(() => hydrateFieldCombos(wrap, st, meta))

  let saving = false
  async function doSaveSub() {
    if (saving) return
    const val = (sel) => ((wrap.querySelector(sel) || {}).value || '').trim()
    const dict = isNew ? ((combo && combo.getValue && combo.getValue()) || dictVal) : fm.dict_code
    if (isNew && !dict) { M.cmxWarn?.('请选择字典'); return }
    const ef = readEvtFilter(wrap)
    // 名称留空兜底为「<字典> → <端点>」（placeholder 承诺的缺省名；编辑态清空同样恢复默认）
    const epName = ep ? (ep.name || ep.target_sys) : ''
    const defName = dict && epName ? `${dict} → ${epName}` : ''
    const body = {
      id: fm.id || undefined,
      endpoint_id: fm.endpoint_id,
      dict_code: dict,
      name: val('#smName') || defName, description: val('#smDesc'),
      active: !!(wrap.querySelector('#smActive') || {}).checked,
      event_types: ef.evts,
      filter: ef.conditions.length ? { conditions: ef.conditions, logic: 'and' } : null,
      field_map: readFieldMap(wrap),
    }
    saving = true
    try {
      const saved = (await apiPost('/api/mdm/subscriptions', body, st.dbId)) || {}
      showCmxToast(isNew ? `订阅已创建（#${saved.id}）。自新事件起分发，需补投历史请用「补发」` : '订阅已保存')
      dlg.close('confirm', { force: true })
      await reloadAll(st)
    } catch (e) { M.cmxError?.(`保存失败：${e.message}`) } finally { saving = false }
  }
  dlg.openModal().then(() => dlg.remove())
}

// ── 事件绑定与视图生命周期 ──────────────────────────────────────────────────
// 深链 fire-and-forget 双渲染会整体覆盖 host.shadowRoot 子树，直接在元素上 addEventListener
// 的监听随之丢失（曾致按钮无响应/侧栏空列表）。因此所有交互改为 shadowRoot 级事件委托
// （覆盖不丢）+ MutationObserver 检测 DOM 被覆盖后自动重渲染（含 grid 列模型重建）。

// 渲染前重定位视图根：宿主 DOM 重建后旧引用 detached，写入无效。开销可忽略。
function relocate(st) {
  for (const host of st.hosts) {
    const sr = host && (host.renderRoot || host.shadowRoot)
    if (!sr) continue
    const epx = sr.querySelector('.epx')
    if (epx) st.explorerRoot = epx
    const pg = sr.querySelector('.pg')
    if (pg) st.contentRoot = pg
  }
}

// 视图根再定位 + 局部重渲染（委托层与覆盖观察器共用）。
// explorer 与 content 分属两个 host，逐 host 找各自视图根。
function rerender(st) {
  for (const host of st.hosts) {
    const sr = host && (host.renderRoot || host.shadowRoot)
    if (!sr) continue
    const epx = sr.querySelector('.epx')
    if (epx) { st.explorerRoot = epx; renderEpList(st) }
    const pg = sr.querySelector('.pg')
    if (pg) {
      st.contentRoot = pg
      renderSubHead(st)
      applyData(st)   // 内部自检 grid 失联/列模型缺位并重建+退避重试
    }
  }
}

function bindDelegates(st, host) {
  // 必须挂 shadowRoot（稳定层）：renderRoot 容器会被框架重建，挂那里委托/观察器随重建丢失
  const sr = host && (host.shadowRoot || host.renderRoot)
  if (!sr || sr.__smDelegated) return
  sr.__smDelegated = true
  st.hosts.add(host)
  const M = cmx()

  // 统一 click 分发（ui5 组件的 composed 事件 target 会 retarget 到宿主元素，closest 可用）
  sr.addEventListener('click', (e) => {
    const t = e.target instanceof Element ? e.target : null
    if (!t) return
    const epCard = t.closest('.ep-card')
    if (epCard && epCard.dataset.ep) {
      st.curEpId = Number(epCard.dataset.ep); st.subPage = 1
      renderEpList(st); reloadSubs(st)
      return
    }
    if (t.closest('#smEpAdd')) { openEpEditDialog(st, null); return }
    if (t.closest('#smEpEdit')) { const ep = curEp(st); if (ep) openEpEditDialog(st, ep); return }
    if (t.closest('#smEpTest')) {
      const ep = curEp(st); if (!ep) return
      apiPost('/api/mdm/endpoints/test', { id: Number(ep.id) }, st.dbId)
        .then((t2) => {
          if (t2 && t2.ok) showCmxToast(`测试通过（${t2.latencyMs ?? '-'} ms）${t2.detail ? '·' + t2.detail : ''}`)
          else M.cmxError?.(`测试失败：${(t2 && t2.detail) || '未知原因'}`)
        })
        .catch((e2) => M.cmxError?.(`测试失败：${e2.message}`))
      return
    }
    if (t.closest('#smEpToggle')) { toggleEndpoint(st); return }
    if (t.closest('#smSubAdd')) { openSubEditDialog(st, null); return }
    if (t.closest('#smReload')) { reloadAll(st); return }
    if (t.closest('#smSearch')) { doSearch(st); return }
    if (t.closest('#smReset')) {
      st.fDict = ''; st.fActive = ''; st.subPage = 1
      const i1 = sr.querySelector('#smFDict'); if (i1) i1.value = ''
      const s2 = sr.querySelector('#smFActive'); if (s2) s2.value = ''
      reloadSubs(st)
      return
    }
  })

  // 字典输入回车搜索（keydown 不 composed，绑 sr 冒泡可及）
  sr.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target && e.target.id === 'smFDict') doSearch(st)
  })

  // pager / grid 自定义事件（bubbles 到 shadowRoot；composed 与否均在此被捕获）
  sr.addEventListener('page-change', (e) => {
    const d = e.detail || {}
    if (d.pageSize && d.pageSize !== st.pageSize) { st.pageSize = d.pageSize; st.subPage = 1 }
    else st.subPage = d.page || 1
    reloadSubs(st)
  })
  sr.addEventListener('cmx-cell-link-click', (e) => {
    const d = e.detail || {}
    const grid = st.grid
    const ds = grid && grid._ds
    const row = (ds && ds.rows && !isNaN(parseInt(d.rowId, 10))) ? ds.rows[parseInt(d.rowId, 10)] : null
    const rec = row ? (row.toPlainObject ? row.toPlainObject() : row) : null
    if (!rec || rec.id == null) return
    doAction(st, d.actionRef, rec)
  })

  // DOM 覆盖自动重渲染（fire-and-forget 第二份渲染覆盖后补数据/监听语义）
  const mo = new MutationObserver(() => {
    clearTimeout(sr.__smMoT)
    sr.__smMoT = setTimeout(() => rerender(st), 150)
  })
  mo.observe(sr, { childList: true, subtree: true })
}

async function toggleEndpoint(st) {
  const M = cmx()
  const ep = curEp(st); if (!ep) return
  const to = !ep.active
  const msg = to
    ? `确认启用端点「${ep.name || ep.target_sys}」？其下各订阅按自身启停状态恢复投递。`
    : `确认停用端点「${ep.name || ep.target_sys}」？其下全部订阅（含存量积压）立即停止投递，级联即时生效。`
  const ok = await M.cmxConfirm?.({ title: to ? '启用端点' : '停用端点', message: msg, intent: (to ? 'normal' : 'danger'),
    confirmText: to ? '启用' : '停用' })
  if (ok === false) return
  try {
    await apiPost('/api/mdm/endpoints/set-active', { id: Number(ep.id), active: to }, st.dbId)
    showCmxToast(to ? '端点已启用' : '端点已停用（全部投递暂停）')
    await reloadAll(st)
  } catch (e) { M.cmxError?.(`操作失败：${e.message}`) }
}

function doSearch(st) {
  const root = st.contentRoot
  if (!root) return
  st.fDict = (root.querySelector('#smFDict') || {}).value || ''
  st.fActive = (root.querySelector('#smFActive') || {}).value || ''
  st.subPage = 1
  reloadSubs(st)
}

function whenRendered(host, sel, cb, t) {
  // 用 setTimeout 而非 rAF：IAB/后台标签 rAF 被冻结，监听注册会无限期延迟（曾致整页交互失效）
  const n = t == null ? 600 : t
  const root = host && (host.renderRoot || host.shadowRoot)
  if (root && root.querySelector(sel)) { cb(root); return }
  if (n <= 0) return
  setTimeout(() => whenRendered(host, sel, cb, n - 1), 50)
}

// 预取数据（幂等：两视图谁先到谁触发，只跑一次）
function ensureInit(st) {
  if (!st.initPromise) {
    st.initPromise = (async () => {
      await Promise.all([loadLookups(st), loadEndpoints(st)])
      if (st.curEpId == null && st.endpoints.length) st.curEpId = Number(st.endpoints[0].id)
      await loadSubs(st)
    })()
  }
  return st.initPromise
}

// 预取下拉数据源：通道枚举 + 激活映射字典去重（失败均静默降级）。
async function loadLookups(st) {
  try {
    const d = (await apiGet('/api/mdm/subscriptions/channels', st.dbId)) || {}
    const list = (d && d.list) || []
    st.channels = list.map((c) => ({ type: c.type || c, label: c.label || c.type || c }))
  } catch { st.channels = [] }
  if (!st.channels.length) st.channels = [{ type: 'webhook', label: 'webhook' }, { type: 'rest_pull', label: 'rest_pull' }]
  try {
    const acts = (await apiGet('/api/mdm/activations', st.dbId)) || []
    const seen = []
    for (const a of acts) { const d = a.target_dict || a.targetDict; if (d && !seen.includes(d)) seen.push(d) }
    st.dicts = seen.sort()
    // 字典中文名（dct/meta 顶层 dictName，不带 with_props 轻量）：后台并发补拉不阻塞首屏，
    // 新建弹框打开时 await st.dictNamesReady 保证下拉已带名称；无坐标/失败降级纯 code
    if (st.dicts.length && st.coord && st.coord.domain && st.coord.application) {
      st.dictNamesReady = Promise.all(st.dicts.map((d) =>
        apiGet(`/api/dct/meta?${coordQs(st, { dict: d })}`, st.dbId)
          .then((m) => { if (m && m.dictName) st.dictNames[d] = m.dictName })
          .catch(() => {})
      ))
    }
  } catch { st.dicts = [] }
}

export default {
  defaultView: 'content',
  views: {
    async content(ctx) {
      const host = ctx && ctx.host
      const st = getState(host)
      if (!st) return '<div style="padding:16px">subscription-manager 初始化失败：无 workspace scope</div>'
      st.host = host
      st.coord = st.coord || readCoord(ctx)
      st.dbId = st.dbId || st.coord.dbId || ((ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || '')
      // 先等数据、后挂委托+渲染：避免"绑定回调跑在数据前→空列表无人补刷"竞态
      try {
        await ensureInit(st)
      } catch (e) { console.error('[subscription-manager] init fail', e); cmx().cmxError?.(`初始化失败：${e.message}`) }
      if (host) whenRendered(host, '.pg', () => { bindDelegates(st, host); rerender(st) })
      return `<style>${styleCss()}</style>${contentHtml(st)}`
    },
    async explorer(ctx) {
      const host = ctx && ctx.host
      const st = getState(host)
      if (!st) return '<div style="padding:12px">端点列表初始化失败</div>'
      st.host = st.host || host
      st.coord = st.coord || readCoord(ctx)
      st.dbId = st.dbId || st.coord.dbId || ((ctx && ctx.props && (ctx.props.dbId || ctx.props.db_id)) || '')
      // 与 content 同序：先数据后渲染，防竞态空列表
      try { await ensureInit(st) } catch { /* content 视图统一报错 */ }
      if (host) whenRendered(host, '.epx', () => { bindDelegates(st, host); rerender(st) })
      return `<style>${styleCss()}</style>${explorerHtml(st)}`
    },
  },
}
