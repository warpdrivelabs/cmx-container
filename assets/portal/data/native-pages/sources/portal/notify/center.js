// 通知中心 native_pages 页面：单 content 视图，按 props.center 展示 任务/消息/日志 通知列表。
// 支持：列表(未读高亮)、点击单条标记已读、全部已读、刷新；数据来自后端 /api/notifications。
// 由 shellbar 铃铛下拉选中某中心后打开（每个中心一个 tab，props.center 区分）。

const CENTERS = {
  task: { label: '任务中心', icon: 'task' },
  message: { label: '消息中心', icon: 'email' },
  log: { label: '日志中心', icon: 'history' },
}

const esc = (s) => String(s ?? '')
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

async function apiJson (url, options = {}) {
  const res = await fetch(url, {
    ...options,
    headers: { Accept: 'application/json', ...(options.headers || {}) },
    credentials: 'same-origin',
  })
  let j = null
  try { j = await res.json() } catch {}
  if (!res.ok || (j && typeof j.code === 'number' && j.code !== 0)) {
    throw new Error((j && (j.msg || j.error)) || `HTTP ${res.status}`)
  }
  return j && typeof j === 'object' && 'data' in j ? j.data : j
}

function fmtTime (ms) {
  if (!ms) return ''
  try {
    const d = new Date(Number(ms))
    const p = (n) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
  } catch { return '' }
}

function centerOf (ctx) {
  // props 由 native-host 经 JSON 属性注入，渲染时作为 ctx.props 传入。
  const p = ctx?.props?.center || ctx?.host?.__props?.center
  return (p && CENTERS[p]) ? p : 'task'
}

function styleCss () {
  return `
    .nc{--neo-cyan:#00b4d8;--neo-mint:#10b981;--neo-warn:#f59e0b;--neo-red:#e90b0b;
      display:flex;flex-direction:column;flex:1 1 auto;min-height:0;height:100%;width:100%;box-sizing:border-box;
      font:13px/1.5 var(--sapFontFamily,Arial,sans-serif);color:var(--sapTextColor,#1d2d3e);background:var(--sapBackgroundColor,#f5f6f7);overflow:hidden}
    .nc-head{flex:0 0 auto;display:flex;align-items:center;gap:8px;height:46px;box-sizing:border-box;padding:0 12px;
      border-bottom:1px solid color-mix(in srgb,var(--neo-cyan) 22%,var(--sapGroup_TitleBorderColor,#d9d9d9));
      background:color-mix(in srgb,var(--neo-cyan) 12%,var(--sapList_HeaderBackground,#eef2f6))}
    .nc-head ui5-icon{width:1.2rem;height:1.2rem;color:var(--neo-cyan)}
    .nc-title{font-weight:700;font-size:14px}
    .nc-count{font-size:11px;font-weight:700;color:#fff;background:var(--neo-red);border-radius:999px;padding:1px 7px;min-width:18px;text-align:center}
    .nc-count[data-zero="1"]{background:var(--sapNeutralBackground,#c8ccd0)}
    .nc-actions{margin-left:auto;display:flex;gap:6px}
    .nc-btn{border:1px solid color-mix(in srgb,var(--neo-cyan) 20%,transparent);border-radius:6px;background:var(--sapList_Background,#fff);
      color:var(--neo-cyan);font:inherit;font-size:12px;padding:4px 10px;cursor:pointer}
    .nc-btn:hover{background:color-mix(in srgb,var(--neo-cyan) 12%,var(--sapList_Background,#fff))}
    .nc-list{flex:1 1 auto;min-height:0;overflow:auto;padding:8px 10px 16px;display:flex;flex-direction:column;gap:6px}
    .nc-item{display:flex;gap:10px;padding:9px 12px;border:1px solid color-mix(in srgb,var(--neo-cyan) 10%,var(--sapList_BorderColor,#e5e5e5));
      border-radius:8px;background:var(--sapList_Background,#fff);cursor:pointer;transition:border-color .14s,box-shadow .14s,background .14s}
    .nc-item:hover{border-color:color-mix(in srgb,var(--neo-cyan) 35%,transparent);box-shadow:0 0 0 1px color-mix(in srgb,var(--neo-cyan) 8%,transparent)}
    .nc-item.unread{background:color-mix(in srgb,var(--neo-cyan) 6%,var(--sapList_Background,#fff));border-left:3px solid var(--neo-cyan)}
    .nc-dot{flex:0 0 auto;width:8px;height:8px;border-radius:50%;margin-top:6px;background:transparent}
    .nc-item.unread .nc-dot{background:var(--neo-red)}
    .nc-main{min-width:0;flex:1 1 auto}
    .nc-item-title{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .nc-item.unread .nc-item-title{font-weight:700}
    .nc-item-body{font-size:12px;color:var(--sapContent_LabelColor,#6a6d70);margin-top:2px;white-space:pre-wrap;word-break:break-word}
    .nc-item-meta{font-size:11px;color:var(--sapContent_LabelColor,#6a6d70);margin-top:4px;display:flex;gap:8px;align-items:center}
    .nc-level{font-size:10px;font-weight:700;border-radius:4px;padding:0 5px;border:1px solid currentColor}
    .nc-level[data-l="error"]{color:var(--neo-red)} .nc-level[data-l="warning"]{color:var(--neo-warn)}
    .nc-level[data-l="success"]{color:var(--neo-mint)} .nc-level[data-l="info"]{color:var(--neo-cyan)}
    .nc-empty{flex:1 1 auto;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;color:var(--sapContent_LabelColor,#6a6d70)}
    .nc-empty ui5-icon{width:1.6rem;height:1.6rem;color:color-mix(in srgb,var(--neo-cyan) 55%,var(--sapContent_LabelColor,#6a6d70))}
  `
}

function itemHtml (it) {
  const lvl = it.level || 'info'
  return `<div class="nc-item ${it.read ? '' : 'unread'}" role="button" tabindex="0" data-id="${esc(it.id)}">
    <span class="nc-dot"></span>
    <div class="nc-main">
      <div class="nc-item-title">${esc(it.title)}</div>
      ${it.body ? `<div class="nc-item-body">${esc(it.body)}</div>` : ''}
      <div class="nc-item-meta">
        <span class="nc-level" data-l="${esc(lvl)}">${esc(lvl)}</span>
        <span>${esc(fmtTime(it.createdAt))}</span>
        ${it.read ? '' : '<span style="color:var(--neo-red)">● 未读</span>'}
      </div>
    </div>
  </div>`
}

function viewHtml (center, items) {
  const meta = CENTERS[center] || CENTERS.task
  const unread = items.filter((x) => !x.read).length
  const body = items.length
    ? items.map(itemHtml).join('')
    : `<cmx-empty-state icon="${meta.icon}" title="暂无通知" size="sm"></cmx-empty-state>`
  return `<div class="nc" data-center="${esc(center)}">
    <div class="nc-head">
      <ui5-icon name="${meta.icon}"></ui5-icon>
      <span class="nc-title">${esc(meta.label)}</span>
      <span class="nc-count" data-zero="${unread ? 0 : 1}">${unread > 99 ? '99+' : unread}</span>
      <span class="nc-actions">
        <button class="nc-btn" type="button" data-act="refresh">刷新</button>
        <button class="nc-btn" type="button" data-act="read-all">全部已读</button>
      </span>
    </div>
    <div class="nc-list">${body}</div>
  </div>`
}

async function loadItems (center) {
  const d = await apiJson(`/api/notifications?center=${encodeURIComponent(center)}`)
  return (d && d.items) || []
}

function bind (root, center, rerender) {
  root.querySelectorAll('[data-id]').forEach((el) => {
    const open = async () => {
      const id = el.getAttribute('data-id')
      try {
        await apiJson('/api/notifications/mark-read', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ center, id }),
        })
      } catch {}
      rerender()
    }
    el.addEventListener('click', open)
    el.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); open() } })
  })
  root.querySelector('[data-act="refresh"]')?.addEventListener('click', () => rerender())
  root.querySelector('[data-act="read-all"]')?.addEventListener('click', async () => {
    try {
      await apiJson('/api/notifications/mark-read', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ all: true, center }),
      })
    } catch {}
    rerender()
  })
}

async function mount (ctx) {
  const host = ctx.host
  const center = centerOf(ctx)
  const render = async () => {
    const root = host?.renderRoot || host?.shadowRoot?.querySelector('.native-page-root')
    if (!root || !root.isConnected) return
    let items = []
    try { items = await loadItems(center) } catch (e) { /* 显示空态 */ }
    root.innerHTML = `<style>${styleCss()}</style>${viewHtml(center, items)}`
    const wrap = root.querySelector('.nc')
    if (wrap) bind(wrap, center, () => { render() })
  }
  // 首帧：等 renderRoot 就绪再渲染
  const wait = (n = 0) => {
    const root = host?.renderRoot || host?.shadowRoot?.querySelector('.native-page-root')
    if (root && root.isConnected) { render(); return }
    if (n < 20) requestAnimationFrame(() => wait(n + 1))
  }
  requestAnimationFrame(() => wait())
  return `<style>${styleCss()}</style><div class="nc"><cmx-empty-state icon="synchronize" title="加载中…" size="sm"></cmx-empty-state></div>`
}

export default {
  defaultView: 'content',
  views: {
    async content (ctx) { return mount(ctx) },
  },
}
