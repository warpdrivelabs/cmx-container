// src/model/types.ts
var CARDINALITY_LABEL = {
  oneToOne: "1:1",
  oneToMany: "1:N",
  manyToMany: "N:M"
};
var STATUS_LABEL = {
  experimental: "\u8BD5\u9A8C",
  active: "\u6FC0\u6D3B",
  deprecated: "\u5E9F\u5F03"
};

// src/model/OntologyModel.ts
function clone(v) {
  return JSON.parse(JSON.stringify(v));
}
var OntologyModel = class _OntologyModel {
  def;
  constructor(def) {
    this.def = def ? clone(def) : _OntologyModel.skeleton();
    this.def.nodes = this.def.nodes || [];
    this.def.edges = this.def.edges || [];
    this.syncInterfaceLinks();
  }
  /** 空骨架。 */
  static skeleton(name) {
    const def = { nodes: [], edges: [] };
    if (name != null) def.name = name;
    return def;
  }
  getDef() {
    return clone(this.def);
  }
  setDef(def) {
    const prevLayout = this.def && this.def._layout ? this.def._layout : void 0;
    const prevRoutes = this.def && this.def._edgeRoutes ? this.def._edgeRoutes : void 0;
    const prevGroups = this.def && this.def._groupCollapsed ? this.def._groupCollapsed : void 0;
    this.def = clone(def);
    this.def.nodes = this.def.nodes || [];
    this.def.edges = this.def.edges || [];
    if (!this.def._layout && prevLayout) {
      const ids = new Set(this.def.nodes.map((n) => n.id));
      const kept = {};
      for (const k of Object.keys(prevLayout)) {
        const v = prevLayout[k];
        if (v && ids.has(k)) kept[k] = v;
      }
      if (Object.keys(kept).length) this.def._layout = kept;
    }
    if (!this.def._edgeRoutes && prevRoutes) {
      const apis = new Set(this.def.edges.map((e) => e.apiName));
      const kept = {};
      for (const k of Object.keys(prevRoutes)) {
        const v = prevRoutes[k];
        if (v && apis.has(k)) kept[k] = v;
      }
      if (Object.keys(kept).length) this.def._edgeRoutes = kept;
    }
    if (!this.def._groupCollapsed && prevGroups) this.def._groupCollapsed = prevGroups;
    this.syncInterfaceLinks();
  }
  /** 设置某关系边的手动布线折点（含锚点）。 */
  setEdgeRoute(apiName, points) {
    this.def._edgeRoutes = this.def._edgeRoutes || {};
    this.def._edgeRoutes[apiName] = points;
  }
  /** 取某关系边的手动布线折点；无则 undefined。 */
  edgeRoute(apiName) {
    return this.def._edgeRoutes ? this.def._edgeRoutes[apiName] : void 0;
  }
  /** 清除某关系边的手动布线（回退自动布线）。 */
  clearEdgeRoute(apiName) {
    if (this.def._edgeRoutes) delete this.def._edgeRoutes[apiName];
  }
  // ── 分组折叠态（DAM 分域）──
  /** 当前所有分组容器折叠态（组键 → 是否收起）。 */
  groupCollapsed() {
    return this.def._groupCollapsed;
  }
  /** 设某组键收起/展开。 */
  setGroupCollapsed(key, collapsed) {
    this.def._groupCollapsed = this.def._groupCollapsed || {};
    this.def._groupCollapsed[key] = collapsed;
  }
  /** 切换某组键折叠态（默认收起，故首次切换 = 展开）。 */
  toggleGroup(key) {
    this.def._groupCollapsed = this.def._groupCollapsed || {};
    this.def._groupCollapsed[key] = !(this.def._groupCollapsed[key] ?? true);
  }
  /** 批量设置（工具栏「全部展开/收起」用）。 */
  setAllGroups(keys, collapsed) {
    this.def._groupCollapsed = this.def._groupCollapsed || {};
    for (const k of keys) this.def._groupCollapsed[k] = collapsed;
  }
  get nodes() {
    return this.def.nodes;
  }
  get edges() {
    return this.def.edges;
  }
  node(id) {
    return this.def.nodes.find((n) => n.id === id);
  }
  /** 仅普通关系边（排除接口实现边）。 */
  get linkEdges() {
    return this.def.edges.filter((e) => !e.isInterfaceLink);
  }
  /** 加对象类型节点。返回 apiName（须唯一，调用方保证；重名则返回 null）。 */
  addObjectType(apiName, displayName) {
    if (!apiName || this.node(apiName)) return null;
    const n = { id: apiName, kind: "object", properties: [] };
    if (displayName != null) n.displayName = displayName;
    n.status = "experimental";
    this.def.nodes.push(n);
    return apiName;
  }
  /** 加接口节点。 */
  addInterface(apiName, displayName) {
    if (!apiName || this.node(apiName)) return null;
    const n = { id: apiName, kind: "interface" };
    if (displayName != null) n.displayName = displayName;
    this.def.nodes.push(n);
    return apiName;
  }
  /** 删节点 + 关联边 + 布局提示 + 其它节点对它的 implements 引用。 */
  delNode(id) {
    this.def.nodes = this.def.nodes.filter((n) => n.id !== id);
    this.def.edges = this.def.edges.filter((e) => e.source !== id && e.target !== id);
    for (const n of this.def.nodes) {
      if (n.implements) n.implements = n.implements.filter((x) => x !== id);
    }
    if (this.def._layout) delete this.def._layout[id];
    this.syncInterfaceLinks();
  }
  /** 加普通关系边。返回 null 成功，否则拒绝原因。apiName 须唯一。 */
  addLink(apiName, source, target, cardinality = "oneToMany", roleA, roleB) {
    if (!this.node(source) || !this.node(target)) return "\u5173\u7CFB\u4E24\u7AEF\u5BF9\u8C61\u7C7B\u578B\u987B\u5B58\u5728";
    if (this.def.edges.some((e2) => e2.apiName === apiName && !e2.isInterfaceLink))
      return `\u5173\u7CFB apiName\u300C${apiName}\u300D\u5DF2\u5B58\u5728`;
    const e = { apiName, source, target, cardinality };
    if (roleA != null) e.roleA = roleA;
    if (roleB != null) e.roleB = roleB;
    this.def.edges.push(e);
    return null;
  }
  /** 删普通关系边（按 apiName）。 */
  delLink(apiName) {
    this.def.edges = this.def.edges.filter((e) => e.isInterfaceLink || e.apiName !== apiName);
  }
  /** 设对象类型实现某接口（幂等），并同步接口实现边。 */
  setImplements(objectApiName, interfaceApiName, on) {
    const n = this.node(objectApiName);
    if (!n || n.kind !== "object") return;
    n.implements = n.implements || [];
    const has = n.implements.includes(interfaceApiName);
    if (on && !has) n.implements.push(interfaceApiName);
    if (!on && has) n.implements = n.implements.filter((x) => x !== interfaceApiName);
    this.syncInterfaceLinks();
  }
  /** 由各对象类型的 implements 重建接口实现边（虚线挂接）。 */
  syncInterfaceLinks() {
    this.def.edges = this.def.edges.filter((e) => !e.isInterfaceLink);
    for (const n of this.def.nodes) {
      if (n.kind !== "object" || !n.implements) continue;
      for (const iface of n.implements) {
        if (!this.node(iface)) continue;
        this.def.edges.push({
          apiName: `${n.id}__implements__${iface}`,
          source: n.id,
          target: iface,
          isInterfaceLink: true
        });
      }
    }
  }
  // ── 属性操作（对象类型卡内）──
  addProperty(objectApiName, prop) {
    const n = this.node(objectApiName);
    if (!n || n.kind !== "object") return;
    n.properties = n.properties || [];
    n.properties.push(prop);
  }
  setProperties(objectApiName, props) {
    const n = this.node(objectApiName);
    if (!n || n.kind !== "object") return;
    n.properties = props;
  }
  setNodeMeta(id, patch) {
    const n = this.node(id);
    if (n) Object.assign(n, patch);
  }
  // ── 布局提示 ──
  setLayoutHint(id, x, y) {
    this.def._layout = this.def._layout || {};
    this.def._layout[id] = { x, y };
  }
  layoutHints() {
    return this.def._layout;
  }
  clearLayoutHints() {
    delete this.def._layout;
  }
  /** 本地结构校验（镜像后端）。返回 null 或首个违规信息。 */
  validate() {
    const ids = /* @__PURE__ */ new Set();
    for (const n of this.def.nodes) {
      if (ids.has(n.id)) return `\u7C7B\u578B apiName \u91CD\u590D\uFF1A${n.id}`;
      ids.add(n.id);
    }
    for (const e of this.def.edges) {
      if (e.isInterfaceLink) continue;
      if (!ids.has(e.source) || !ids.has(e.target))
        return `\u5173\u7CFB\u300C${e.apiName}\u300D\u7AEF\u70B9\u4E0D\u5B58\u5728\uFF1A${e.source} \u2192 ${e.target}`;
    }
    for (const n of this.def.nodes) {
      if (n.kind !== "object") continue;
      const props = n.properties || [];
      const pkCount = props.filter((p) => p.isPrimaryKey).length;
      if (props.length && pkCount === 0) return `\u5BF9\u8C61\u7C7B\u578B\u300C${n.id}\u300D\u7F3A\u4E3B\u952E`;
    }
    return null;
  }
};

// src/layout/layout.ts
var DEFAULT_LAYOUT = {
  colGap: 300,
  rowGap: 260,
  nodeW: 232,
  headH: 46,
  rowH: 22,
  maxRows: 6,
  pad: 0
};
var PREVIEW_LAYOUT = {
  colGap: 260,
  rowGap: 220,
  nodeW: 190,
  headH: 42,
  rowH: 20,
  maxRows: 4,
  pad: 30
};
function isLevelProp(p) {
  return !!(p.isLevel || p.children && p.children.length || p.baseType === "array" || p.baseType === "struct");
}
function countRenderRows(props, maxRows) {
  const scalars = props.filter((p) => !isLevelProp(p));
  const levels = props.filter((p) => isLevelProp(p));
  const shownScalars = Math.min(scalars.length, maxRows);
  const extra = scalars.length > maxRows ? 1 : 0;
  let rows = shownScalars + extra;
  for (const lv of levels) rows += 1 + countRenderRows(lv.children || [], maxRows);
  return rows;
}
function nodeHeight(n, cfg) {
  if (n.kind === "interface") return cfg.headH;
  const rows = countRenderRows(n.properties || [], cfg.maxRows);
  return cfg.headH + rows * cfg.rowH + 8;
}
function layout(def, cfg, hints) {
  const nodes = def.nodes || [];
  const pos = {};
  const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
  let maxX = 0;
  let maxY = 0;
  nodes.forEach((n, i) => {
    const gx = i % cols;
    const gy = Math.floor(i / cols);
    const h = nodeHeight(n, cfg);
    const def_x = cfg.pad + gx * cfg.colGap;
    const def_y = cfg.pad + gy * cfg.rowGap;
    const hint = hints ? hints[n.id] : void 0;
    const x = hint ? hint.x : def_x;
    const y = hint ? hint.y : def_y;
    pos[n.id] = { x, y, w: cfg.nodeW, h };
    maxX = Math.max(maxX, x + cfg.nodeW);
    maxY = Math.max(maxY, y + h);
  });
  return { pos, width: maxX + cfg.pad, height: maxY + cfg.pad };
}

// src/layout/route.ts
var MARGIN = 14;
var STUB = 18;
var TURN = 14;
var r0 = (v) => Math.round(v);
function center(r) {
  return { x: r.x + r.w / 2, y: r.y + r.h / 2 };
}
function inflate(r, m) {
  return { x: r.x - m, y: r.y - m, w: r.w + 2 * m, h: r.h + 2 * m };
}
function chooseSides(a, b) {
  const ca = center(a);
  const cb = center(b);
  const dx = cb.x - ca.x;
  const dy = cb.y - ca.y;
  if (Math.abs(dx) >= Math.abs(dy)) return dx >= 0 ? { sa: "R", sb: "L" } : { sa: "L", sb: "R" };
  return dy >= 0 ? { sa: "B", sb: "T" } : { sa: "T", sb: "B" };
}
function anchorPt(r, s) {
  if (s === "R") return { x: r0(r.x + r.w), y: r0(r.y + r.h / 2) };
  if (s === "L") return { x: r0(r.x), y: r0(r.y + r.h / 2) };
  if (s === "T") return { x: r0(r.x + r.w / 2), y: r0(r.y) };
  return { x: r0(r.x + r.w / 2), y: r0(r.y + r.h) };
}
function stubOf(p, s) {
  if (s === "R") return { x: p.x + STUB, y: p.y };
  if (s === "L") return { x: p.x - STUB, y: p.y };
  if (s === "T") return { x: p.x, y: p.y - STUB };
  return { x: p.x, y: p.y + STUB };
}
function segHits(p, q, r) {
  const x0 = Math.min(p.x, q.x);
  const x1 = Math.max(p.x, q.x);
  const y0 = Math.min(p.y, q.y);
  const y1 = Math.max(p.y, q.y);
  return x0 < r.x + r.w && x1 > r.x && y0 < r.y + r.h && y1 > r.y;
}
function blocked(p, q, obst) {
  for (const r of obst) if (segHits(p, q, r)) return true;
  return false;
}
function ptInside(p, obst) {
  for (const r of obst) if (p.x > r.x && p.x < r.x + r.w && p.y > r.y && p.y < r.y + r.h) return true;
  return false;
}
function uniqSorted(a) {
  return [...new Set(a.map(r0))].sort((m, n) => m - n);
}
function astar(start, goal, xs, ys, obst) {
  const ixOf = /* @__PURE__ */ new Map();
  xs.forEach((v, i) => ixOf.set(v, i));
  const iyOf = /* @__PURE__ */ new Map();
  ys.forEach((v, i) => iyOf.set(v, i));
  if (!ixOf.has(start.x) || !iyOf.has(start.y) || !ixOf.has(goal.x) || !iyOf.has(goal.y)) return null;
  const K = (x, y) => x + ":" + y;
  const hEst = (x, y) => Math.abs(x - goal.x) + Math.abs(y - goal.y);
  const open = /* @__PURE__ */ new Map();
  const all = /* @__PURE__ */ new Map();
  const closed = /* @__PURE__ */ new Set();
  const sKey = K(start.x, start.y);
  const gKey = K(goal.x, goal.y);
  open.set(sKey, { x: start.x, y: start.y, g: 0, f: hEst(start.x, start.y), dir: "", prev: null });
  all.set(sKey, open.get(sKey));
  let guard = 0;
  while (open.size > 0 && guard++ < 4e4) {
    let curKey = null;
    let cur = null;
    for (const [k, n] of open) if (cur === null || n.f < cur.f) {
      cur = n;
      curKey = k;
    }
    if (curKey === null || cur === null) break;
    open.delete(curKey);
    closed.add(curKey);
    if (curKey === gKey) {
      const pts = [];
      let node = cur;
      while (node) {
        pts.push({ x: node.x, y: node.y });
        node = node.prev ? all.get(node.prev) ?? null : null;
      }
      pts.reverse();
      return pts.slice(1, -1);
    }
    const ix = ixOf.get(cur.x);
    const iy = iyOf.get(cur.y);
    if (ix === void 0 || iy === void 0) continue;
    const cand = [];
    const xr = xs[ix + 1];
    if (xr !== void 0) cand.push({ x: xr, y: cur.y, d: "H" });
    const xl = xs[ix - 1];
    if (xl !== void 0) cand.push({ x: xl, y: cur.y, d: "H" });
    const yd = ys[iy + 1];
    if (yd !== void 0) cand.push({ x: cur.x, y: yd, d: "V" });
    const yu = ys[iy - 1];
    if (yu !== void 0) cand.push({ x: cur.x, y: yu, d: "V" });
    for (const nb of cand) {
      const nKey = K(nb.x, nb.y);
      if (closed.has(nKey)) continue;
      if (blocked({ x: cur.x, y: cur.y }, { x: nb.x, y: nb.y }, obst)) continue;
      const step = Math.abs(nb.x - cur.x) + Math.abs(nb.y - cur.y);
      const turn = cur.dir !== "" && cur.dir !== nb.d ? TURN : 0;
      const ng = cur.g + step + turn;
      const prevN = all.get(nKey);
      if (prevN === void 0 || ng < prevN.g) {
        const nn = { x: nb.x, y: nb.y, g: ng, f: ng + hEst(nb.x, nb.y), dir: nb.d, prev: curKey };
        open.set(nKey, nn);
        all.set(nKey, nn);
      }
    }
  }
  return null;
}
function fallback(pa, sPt, gPt, pb) {
  if (Math.abs(gPt.x - sPt.x) >= Math.abs(gPt.y - sPt.y)) {
    const mx = r0((sPt.x + gPt.x) / 2);
    return [pa, sPt, { x: mx, y: sPt.y }, { x: mx, y: gPt.y }, gPt, pb];
  }
  const my = r0((sPt.y + gPt.y) / 2);
  return [pa, sPt, { x: sPt.x, y: my }, { x: gPt.x, y: my }, gPt, pb];
}
function selfLoop(r) {
  const ext = 30;
  const rx = r.x + r.w;
  const cy = r0(r.y + r.h / 2);
  const cx = r0(r.x + r.w / 2);
  return [
    { x: r0(rx), y: cy },
    { x: r0(rx + ext), y: cy },
    { x: r0(rx + ext), y: r0(r.y - ext) },
    { x: cx, y: r0(r.y - ext) },
    { x: cx, y: r0(r.y) }
  ];
}
function simplify(pts) {
  const out = [];
  for (const p of pts) {
    const b = out[out.length - 1];
    if (b && b.x === p.x && b.y === p.y) continue;
    const a = out[out.length - 2];
    if (a && b && (a.x === b.x && b.x === p.x || a.y === b.y && b.y === p.y)) {
      out[out.length - 1] = p;
      continue;
    }
    out.push(p);
  }
  return out;
}
function routeAnchored(pa, sa, pb, sb, endA, endB, others) {
  const sPt = stubOf(pa, sa);
  const gPt = stubOf(pb, sb);
  const inflated = others.map((r) => inflate(r, MARGIN));
  const obst = [...inflated, endA, endB];
  const xs = uniqSorted([sPt.x, gPt.x, r0((sPt.x + gPt.x) / 2), ...inflated.flatMap((r) => [r.x, r.x + r.w])]);
  const ys = uniqSorted([sPt.y, gPt.y, r0((sPt.y + gPt.y) / 2), ...inflated.flatMap((r) => [r.y, r.y + r.h])]);
  let mids = null;
  if (!ptInside(sPt, obst) && !ptInside(gPt, obst)) mids = astar(sPt, gPt, xs, ys, obst);
  const raw = mids ? [pa, sPt, ...mids, gPt, pb] : fallback(pa, sPt, gPt, pb);
  return simplify(raw);
}
function routeEdge(a, b, others) {
  if (Math.abs(a.x - b.x) < 0.5 && Math.abs(a.y - b.y) < 0.5 && a.w === b.w && a.h === b.h) return selfLoop(a);
  const { sa, sb } = chooseSides(a, b);
  return routeAnchored(anchorPt(a, sa), sa, anchorPt(b, sb), sb, a, b, others);
}
function toPath(pts, radius = 7) {
  if (pts.length < 2) return "";
  const p0 = pts[0];
  if (pts.length === 2) {
    const p1 = pts[1];
    return `M${p0.x},${p0.y} L${p1.x},${p1.y}`;
  }
  let d = `M${p0.x},${p0.y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const a = pts[i - 1];
    const b = pts[i];
    const c = pts[i + 1];
    const r = Math.min(radius, dist(a, b) / 2, dist(b, c) / 2);
    const t1 = towards(b, a, r);
    const t2 = towards(b, c, r);
    d += ` L${round1(t1.x)},${round1(t1.y)} Q${b.x},${b.y} ${round1(t2.x)},${round1(t2.y)}`;
  }
  const last = pts[pts.length - 1];
  d += ` L${last.x},${last.y}`;
  return d;
}
function polyMidpointOriented(pts) {
  if (pts.length === 0) return { x: 0, y: 0, vertical: false };
  if (pts.length === 1) return { x: pts[0].x, y: pts[0].y, vertical: false };
  const seg = [];
  let total = 0;
  for (let i = 1; i < pts.length; i++) {
    const d = dist(pts[i - 1], pts[i]);
    seg.push(d);
    total += d;
  }
  let acc = 0;
  for (let i = 1; i < pts.length; i++) {
    const d = seg[i - 1] ?? 0;
    if (acc + d >= total / 2) {
      const t = d ? (total / 2 - acc) / d : 0;
      const a = pts[i - 1];
      const b = pts[i];
      const vertical = Math.abs(b.y - a.y) > Math.abs(b.x - a.x);
      return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, vertical };
    }
    acc += d;
  }
  const m = pts[Math.floor(pts.length / 2)];
  return { x: m.x, y: m.y, vertical: false };
}
function polyMidpoint(pts) {
  const m = polyMidpointOriented(pts);
  return { x: m.x, y: m.y };
}
function dist(a, b) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}
function towards(from, to, d) {
  const len = dist(from, to) || 1;
  return { x: from.x + (to.x - from.x) * d / len, y: from.y + (to.y - from.y) * d / len };
}
function round1(v) {
  return Math.round(v * 10) / 10;
}

// src/render/svg.ts
function portHot(hot, node, prop, side) {
  return !!hot && hot.node === node && (hot.prop || null) === (prop || null) && hot.side === side;
}
function esc(s) {
  return String(s == null ? "" : s).replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]
  );
}
function propGlyph(p) {
  if (p.isPrimaryKey) return "\u25C7";
  if (p.isTitle) return "\u233E";
  return "\xB7";
}
function statusClass(n) {
  return `og-st-${n.status || "experimental"}`;
}
function propIsLevel(p) {
  return !!(p.isLevel || p.children && p.children.length || p.baseType === "array" || p.baseType === "struct");
}
function rowMidY(ctx, rowIdx) {
  return ctx.topY + rowIdx * ctx.rowH + ctx.rowH / 2;
}
function rowPorts(ctx, propApi, rowIdx) {
  const cy = rowMidY(ctx, rowIdx);
  const pa = esc(propApi);
  const hl = (side) => portHot(ctx.hot, ctx.nodeId, propApi, side) ? " hot" : "";
  return `<circle class="og-port${hl("L")}" data-port="${ctx.pid}" data-prop="${pa}" data-side="L" cx="${ctx.rx}" cy="${cy}" r="4.5"/><circle class="og-port${hl("R")}" data-port="${ctx.pid}" data-prop="${pa}" data-side="R" cx="${ctx.rx + ctx.rw}" cy="${cy}" r="4.5"/>`;
}
function scalarRow(ctx, p, rowIdx, depth) {
  const y = ctx.topY + rowIdx * ctx.rowH + ctx.rowH - 6;
  const x = ctx.rx + 14 + depth * 12;
  const glyph = propGlyph(p);
  const flags = (p.required ? '<tspan class="og-req">*</tspan>' : "") + (p.isIndexed ? ' <tspan class="og-idx">\u26A1</tspan>' : "");
  const sem = p.semanticType ? `<tspan class="og-sem"> ${esc(p.semanticType)}</tspan>` : "";
  const ty = p.baseType ? `<tspan class="og-ty">${esc(p.baseType)}</tspan>` : "";
  return `<text class="og-prow" x="${x}" y="${y}"><tspan class="og-pg">${glyph}</tspan> ${esc(p.apiName)}${flags} ${ty}${sem}</text>`;
}
function levelHeader(ctx, p, rowIdx, depth, innerRows) {
  const yText = ctx.topY + rowIdx * ctx.rowH + ctx.rowH - 6;
  const x = ctx.rx + 12 + depth * 12;
  const kids = (p.children || []).length;
  const kind = p.baseType === "struct" ? "struct" : "array";
  const label = p.entityName || p.displayName || p.apiName;
  const bandY = ctx.topY + rowIdx * ctx.rowH + 2;
  const bandH = (1 + innerRows) * ctx.rowH - 3;
  const bandX = ctx.rx + 6 + depth * 12;
  const bandW = ctx.rw - (6 + depth * 12) - 6;
  const band = `<rect class="og-level-band" x="${bandX}" y="${bandY}" width="${Math.max(bandW, 20)}" height="${bandH}" rx="6"/><rect class="og-level-bar" x="${bandX}" y="${bandY}" width="3" height="${bandH}" rx="1.5"/>`;
  const head = `<text class="og-level-hd" x="${x}" y="${yText}">\u25BE ${esc(label)} <tspan class="og-level-kind">${kind}\xB7${kids}</tspan> <tspan class="og-level-api">${esc(p.apiName)}</tspan></text>`;
  return band + head;
}
function renderProps(ctx, props, depth, startIdx) {
  const scalars = props.filter((p) => !propIsLevel(p));
  const levels = props.filter((p) => propIsLevel(p));
  let idx = startIdx;
  let body = "";
  let ports = "";
  const shownScalars = depth === 0 ? scalars.slice(0, ctx.maxRows) : scalars;
  const extra = depth === 0 ? scalars.length - shownScalars.length : 0;
  for (const p of shownScalars) {
    body += scalarRow(ctx, p, idx, depth);
    ports += rowPorts(ctx, p.apiName, idx);
    idx += 1;
  }
  if (extra > 0) {
    const y = ctx.topY + idx * ctx.rowH + ctx.rowH - 6;
    body += `<text class="og-more" x="${ctx.rx + 14 + depth * 12}" y="${y}">+${extra} more</text>`;
    idx += 1;
  }
  for (const lv of levels) {
    const inner = countRenderRows(lv.children || [], ctx.maxRows);
    body += levelHeader(ctx, lv, idx, depth, inner);
    ports += rowPorts(ctx, lv.apiName, idx);
    idx += 1;
    const sub = renderProps(ctx, lv.children || [], depth + 1, idx);
    body += sub.body;
    ports += sub.ports;
    idx = sub.nextIdx;
  }
  return { body, ports, nextIdx: idx };
}
function objectCard(n, r, cfg, sel, hot) {
  const color = n.color || "var(--og-node-bar)";
  const props = n.properties || [];
  const ctx = { rx: r.x, rw: r.w, topY: r.y + cfg.headH, rowH: cfg.rowH, maxRows: cfg.maxRows, pid: esc(n.id), nodeId: n.id, hot };
  const rendered = renderProps(ctx, props, 0, 0);
  const clipId = `og-clip-${n.id.replace(/[^\w-]/g, "_")}`;
  return `<g class="og-node og-object ${statusClass(n)} ${sel ? "sel" : ""}" data-node="${esc(n.id)}">
    <clipPath id="${clipId}"><rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" rx="10"/></clipPath>
    <rect class="og-card" x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" rx="10"/>
    <rect class="og-bar" x="${r.x}" y="${r.y}" width="${r.w}" height="6" fill="${esc(color)}" clip-path="url(#${clipId})"/>
    <text class="og-title" x="${r.x + 14}" y="${r.y + 26}">${esc(n.displayName || n.id)}</text>
    <text class="og-api" x="${r.x + 14}" y="${r.y + 40}">${esc(n.id)}</text>
    <line class="og-hr" x1="${r.x}" y1="${r.y + cfg.headH}" x2="${r.x + r.w}" y2="${r.y + cfg.headH}"/>
    ${rendered.body}${rendered.ports}
  </g>`;
}
function interfaceCard(n, r, sel, hot) {
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  const nid = esc(n.id);
  const hl = (side) => portHot(hot, n.id, null, side) ? " hot" : "";
  const ports = `<circle class="og-port${hl("T")}" data-port="${nid}" data-side="T" cx="${cx}" cy="${r.y}" r="4.5"/><circle class="og-port${hl("B")}" data-port="${nid}" data-side="B" cx="${cx}" cy="${r.y + r.h}" r="4.5"/><circle class="og-port${hl("L")}" data-port="${nid}" data-side="L" cx="${r.x}" cy="${cy}" r="4.5"/><circle class="og-port${hl("R")}" data-port="${nid}" data-side="R" cx="${r.x + r.w}" cy="${cy}" r="4.5"/>`;
  return `<g class="og-node og-interface ${sel ? "sel" : ""}" data-node="${esc(n.id)}">
    <rect class="og-ifcard" x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" rx="${r.h / 2}"/>
    <text class="og-iftag" x="${r.x + 14}" y="${r.y + 18}">\xABinterface\xBB</text>
    <text class="og-title" x="${r.x + 14}" y="${r.y + 36}">${esc(n.displayName || n.id)}</text>
    ${ports}
  </g>`;
}
function primaryKeyProp(n) {
  const p = n && n.properties ? n.properties.find((x) => x.isPrimaryKey) : void 0;
  return p ? p.apiName : null;
}
function normName(s) {
  return (s || "").toLowerCase().replace(/[\s_\-./:\\|()\[\]{}·（）【】]+/g, "");
}
function fkProp(src, e, tgt) {
  if (!src || !src.properties) return null;
  const targets = [tgt ? tgt.displayName : void 0, tgt ? tgt.id : void 0, e.roleA, e.displayName].map(normName).filter((s) => s.length >= 2);
  if (targets.length === 0) return null;
  const pk = primaryKeyProp(src);
  let best = null;
  for (const p of src.properties) {
    if (p.apiName === pk) continue;
    const c = normName(p.apiName);
    if (c.length < 2) continue;
    for (const t of targets) {
      if (c === t || c.includes(t) || t.includes(c)) {
        const score = Math.min(c.length, t.length);
        if (best === null || score > best.score) best = { name: p.apiName, score };
      }
    }
  }
  return best ? best.name : null;
}
function renderRowIndexOf(props, propApi, maxRows, depth, startIdx) {
  const scalars = props.filter((p) => !propIsLevel(p));
  const levels = props.filter((p) => propIsLevel(p));
  let idx = startIdx;
  const shown = depth === 0 ? scalars.slice(0, maxRows) : scalars;
  for (const p of shown) {
    if (p.apiName === propApi) return idx;
    idx += 1;
  }
  if (depth === 0 && scalars.length > shown.length) idx += 1;
  for (const lv of levels) {
    if (lv.apiName === propApi) return idx;
    idx += 1;
    const found = renderRowIndexOf(lv.children || [], propApi, maxRows, depth + 1, idx);
    if (found >= 0) return found;
    idx += countRenderRows(lv.children || [], maxRows);
  }
  return -1;
}
function propAnchor(rect, node, cfg, propApi, side) {
  if (!propApi || !node || !node.properties) return null;
  const idx = renderRowIndexOf(node.properties, propApi, cfg.maxRows, 0, 0);
  if (idx < 0) return null;
  const y = Math.round(rect.y + cfg.headH + idx * cfg.rowH + cfg.rowH / 2);
  const x = side === "R" ? Math.round(rect.x + rect.w) : Math.round(rect.x);
  return { x, y };
}
function sideMid(rect, side) {
  return { x: side === "R" ? Math.round(rect.x + rect.w) : Math.round(rect.x), y: Math.round(rect.y + rect.h / 2) };
}
function horizSide(self, other) {
  return other.x + other.w / 2 >= self.x + self.w / 2 ? "R" : "L";
}
function nearPt(a, b) {
  return Math.abs(a.x - b.x) <= 1.5 && Math.abs(a.y - b.y) <= 1.5;
}
function segLines(pts, apiName) {
  let out = "";
  for (let i = 1; i <= pts.length - 3; i++) {
    const a = pts[i];
    const b = pts[i + 1];
    const orient = a.y === b.y ? "H" : a.x === b.x ? "V" : "";
    if (!orient) continue;
    out += `<line class="og-seg og-seg-${orient}" data-edge-seg="${esc(apiName)}" data-seg-i="${i}" data-orient="${orient}" x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}"/>`;
  }
  return out;
}
function edgePath(e, from, to, srcNode, tgtNode, cfg, others, manualRoute, sel) {
  if (e.isInterfaceLink) {
    const d2 = toPath(routeEdge(from, to, others));
    return `<path class="og-edge og-iflink" d="${d2}"/>`;
  }
  const sSide = horizSide(from, to);
  const tSide = horizSide(to, from);
  const srcProp = e.sourceProperty ?? fkProp(srcNode, e, tgtNode) ?? primaryKeyProp(srcNode);
  const tgtProp = e.targetProperty ?? primaryKeyProp(tgtNode);
  const pa = propAnchor(from, srcNode, cfg, srcProp, sSide) ?? sideMid(from, sSide);
  const pb = propAnchor(to, tgtNode, cfg, tgtProp, tSide) ?? sideMid(to, tSide);
  const useManual = !!manualRoute && manualRoute.length >= 2 && nearPt(manualRoute[0], pa) && nearPt(manualRoute[manualRoute.length - 1], pb);
  const pts = useManual ? manualRoute : routeAnchored(pa, sSide, pb, tSide, from, to, others);
  const d = toPath(pts);
  const cls = `og-edge og-link${sel ? " sel" : ""}`;
  const label = e.displayName || e.apiName;
  const role = e.roleA ? ` \xB7 ${esc(e.roleA)}` : "";
  const mid = polyMidpointOriented(pts);
  const mx = Math.round(mid.x);
  const my = Math.round(mid.y);
  const labelText = `${esc(label)}${role}`;
  const labelEl = mid.vertical ? `<text class="og-elabel" x="${mx - 7}" y="${my}" text-anchor="middle" dominant-baseline="central" transform="rotate(-90 ${mx - 7} ${my})">${labelText}</text>` : `<text class="og-elabel" x="${mx}" y="${my - 6}" text-anchor="middle">${labelText}</text>`;
  const route = pts.map((p) => `${Math.round(p.x)},${Math.round(p.y)}`).join(" ");
  return `<g data-edge="${esc(e.apiName)}" data-route="${route}">
    <path class="${cls} og-hit" d="${d}"/>
    <path class="${cls}" d="${d}" marker-end="url(#og-arrow)"/>
    ${segLines(pts, e.apiName)}
    ${labelEl}
  </g>`;
}
function renderSvg(def, lay, cfg, st) {
  const nodeById = new Map(def.nodes.map((n) => [n.id, n]));
  const edges = (def.edges || []).map((e) => {
    const from = lay.pos[e.source];
    const to = lay.pos[e.target];
    if (!from || !to) return "";
    const others = def.nodes.filter((n) => n.id !== e.source && n.id !== e.target).map((n) => lay.pos[n.id]).filter((r) => !!r);
    const manualRoute = def._edgeRoutes ? def._edgeRoutes[e.apiName] : void 0;
    return edgePath(e, from, to, nodeById.get(e.source), nodeById.get(e.target), cfg, others, manualRoute, st.selectedEdgeApiName === e.apiName);
  }).join("");
  const nodes = def.nodes.map((n) => {
    const r = lay.pos[n.id];
    if (!r) return "";
    const sel = st.selectedNodeId === n.id;
    return n.kind === "interface" ? interfaceCard(n, r, sel, st.hotPort) : objectCard(n, r, cfg, sel, st.hotPort);
  }).join("");
  const w = Math.max(lay.width, 400);
  const h = Math.max(lay.height, 300);
  return `<svg class="og-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="og-arrow" viewBox="0 0 12 12" refX="10.5" refY="6" markerWidth="11" markerHeight="11" markerUnits="userSpaceOnUse" orient="auto">
        <path d="M2,2.2 L10.5,6 L2,9.8 L4.8,6 Z" class="og-mk"/>
      </marker>
    </defs>
    <g class="og-edges">${edges}</g>
    <g class="og-nodes">${nodes}</g>
  </svg>`;
}
function domainColor(seg) {
  let h = 0;
  for (let i = 0; i < seg.length; i++) h = (h * 31 + seg.charCodeAt(i)) % 360;
  return `hsl(${h},60%,60%)`;
}
function groupContainer(b) {
  const col = domainColor(b.key.split("/")[0] || b.key);
  const label = esc(b.label);
  const badge = `<g class="og-grp-badge"><rect x="${b.rect.x + b.rect.w - 44}" y="${b.rect.y + 7}" width="32" height="17" rx="8.5"/><text x="${b.rect.x + b.rect.w - 28}" y="${b.rect.y + 19}" text-anchor="middle">${b.count}</text></g>`;
  if (b.collapsed) {
    return `<g class="og-grp og-grp-collapsed" data-group-toggle="${esc(b.key)}" style="--gc:${col}">
      <rect class="og-grp-box" x="${b.rect.x}" y="${b.rect.y}" width="${b.rect.w}" height="${b.rect.h}" rx="12"/>
      <rect class="og-grp-bar" x="${b.rect.x}" y="${b.rect.y + 8}" width="5" height="${b.rect.h - 16}" rx="2.5"/>
      <text class="og-grp-caret" x="${b.rect.x + 18}" y="${b.rect.y + b.rect.h / 2 + 5}">\u25B8</text>
      <text class="og-grp-label" x="${b.rect.x + 36}" y="${b.rect.y + b.rect.h / 2 - 3}">${label}</text>
      <text class="og-grp-key" x="${b.rect.x + 36}" y="${b.rect.y + b.rect.h / 2 + 13}">${esc(b.key)}</text>
      ${badge}
    </g>`;
  }
  return `<g class="og-grp og-grp-open" style="--gc:${col}">
    <rect class="og-grp-region" x="${b.rect.x}" y="${b.rect.y}" width="${b.rect.w}" height="${b.rect.h}" rx="12"/>
    <g data-group-toggle="${esc(b.key)}">
      <rect class="og-grp-head" x="${b.rect.x}" y="${b.rect.y}" width="${b.rect.w}" height="28" rx="12"/>
      <text class="og-grp-caret" x="${b.rect.x + 16}" y="${b.rect.y + 19}">\u25BE</text>
      <text class="og-grp-label" x="${b.rect.x + 32}" y="${b.rect.y + 19}">${label}</text>
      ${badge}
    </g>
  </g>`;
}
function bundledEdge(a, b, count, others) {
  const pts = routeEdge(a, b, others);
  const d = toPath(pts);
  const mid = polyMidpoint(pts);
  const mx = Math.round(mid.x);
  const my = Math.round(mid.y);
  const cnt = count > 1 ? `<g class="og-bundle-cnt"><rect x="${mx - 11}" y="${my - 9}" width="22" height="16" rx="8"/><text x="${mx}" y="${my + 3}" text-anchor="middle">${count}</text></g>` : "";
  return `<path class="og-bundle" d="${d}" marker-end="url(#og-arrow)"/>${cnt}`;
}
function renderGrouped(def, gl, cfg, st) {
  const nodeById = new Map(def.nodes.map((n) => [n.id, n]));
  const containers = gl.boxes.map(groupContainer).join("");
  const cards = def.nodes.map((n) => {
    const r = gl.pos[n.id];
    if (!r) return "";
    const sel = st.selectedNodeId === n.id;
    return n.kind === "interface" ? interfaceCard(n, r, sel, st.hotPort) : objectCard(n, r, cfg, sel, st.hotPort);
  }).join("");
  const cardRects = Object.keys(gl.pos).map((id) => gl.pos[id]);
  const collapsedBoxRects = gl.boxes.filter((b) => b.collapsed).map((b) => b.rect);
  const obstacleRects = [...cardRects, ...collapsedBoxRects];
  const bundles = /* @__PURE__ */ new Map();
  let fineEdges = "";
  for (const e of def.edges || []) {
    const aK = gl.resolveKey(e.source);
    const bK = gl.resolveKey(e.target);
    if (aK === bK) continue;
    if (aK === e.source && bK === e.target) {
      const from = gl.pos[e.source];
      const to = gl.pos[e.target];
      if (!from || !to) continue;
      const others = obstacleRects.filter((r) => r !== from && r !== to);
      const manualRoute = def._edgeRoutes ? def._edgeRoutes[e.apiName] : void 0;
      fineEdges += edgePath(e, from, to, nodeById.get(e.source), nodeById.get(e.target), cfg, others, manualRoute, st.selectedEdgeApiName === e.apiName);
    } else {
      const k = [aK, bK].sort().join("|");
      const cur = bundles.get(k);
      if (cur) cur.count++;
      else bundles.set(k, { a: aK, b: bK, count: 1 });
    }
  }
  let bundleEdges = "";
  for (const b of bundles.values()) {
    const ra = gl.rectOf(b.a);
    const rb = gl.rectOf(b.b);
    if (ra && rb) {
      const others = obstacleRects.filter((r) => r !== ra && r !== rb);
      bundleEdges += bundledEdge(ra, rb, b.count, others);
    }
  }
  const w = Math.max(gl.width, 400);
  const h = Math.max(gl.height, 300);
  return `<svg class="og-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="og-arrow" viewBox="0 0 12 12" refX="10.5" refY="6" markerWidth="11" markerHeight="11" markerUnits="userSpaceOnUse" orient="auto">
        <path d="M2,2.2 L10.5,6 L2,9.8 L4.8,6 Z" class="og-mk"/>
      </marker>
    </defs>
    <g class="og-groups">${containers}</g>
    <g class="og-edges">${bundleEdges}${fineEdges}</g>
    <g class="og-nodes">${cards}</g>
  </svg>`;
}
function graphCss() {
  return `
  :host{
    display:block;width:100%;height:100%;
    /* \u8BBE\u8BA1\u4EE4\u724C\u5C42\uFF1A\u5168\u90E8\u6D3E\u751F\u81EA\u95E8\u6237 --sap* \u4E3B\u9898\u4EE4\u724C\uFF08UI5 \u5728 :root \u91CD\u5B9A\u4E49 --sap*\uFF0C\u4F5C\u4E3A\u7EE7\u627F\u5C5E\u6027\u7A7F\u900F shadow \u8FB9\u754C \u2192 \u81EA\u52A8\u7FFB\uFF0C\u96F6 JS\uFF09\u3002
       \u88F8 hex \u4E3A light-first \u964D\u7EA7\uFF08\u72EC\u7ACB\u90E8\u7F72\u65E0 UI5 \u8FD0\u884C\u65F6\u65F6\u547D\u4E2D\uFF09\u3002\u79D1\u6280\u611F\u5F3A\u8C03\u8272\u8D70 color-mix \u6D3E\u751F\u3002 */
    --og-bg:var(--sapBackgroundColor,#f5f6f7);
    --og-node:var(--sapList_Background,#ffffff);
    --og-node2:var(--sapList_Hover_Background,#eef3fb);
    --og-node-bar:var(--sapHighlightColor,#0a6ed1);
    --og-fg:var(--sapTextColor,#1c2530);
    --og-muted:var(--sapContent_LabelColor,#5a6b7b);
    --og-border:var(--sapList_BorderColor,#c9ced4);
    --og-accent:var(--sapHighlightColor,#0a6ed1);
    --og-accent2:color-mix(in srgb,var(--sapHighlightColor,#0a6ed1) 55%,#8b5cf6);
    --og-edge:var(--sapContent_LabelColor,#5a6b7b);
    --og-iface:color-mix(in srgb,var(--sapHighlightColor,#0a6ed1) 8%,var(--sapList_Background,#fff));
    --og-ok:var(--sapPositiveColor,#178a5a);
    --og-warn:var(--sapCriticalColor,#c26a00);
    --og-err:var(--sapNegativeColor,#d1394a);
    --og-mono:var(--sapContent_MonospaceFontFamily,ui-monospace,Menlo,Consolas,monospace);
    color-scheme:light dark;
  }
  .og-canvas{position:relative;width:100%;height:100%;overflow:auto;background:var(--og-bg,#0b1020)}
  .og-svg{display:block}
  .og-empty{padding:40px;text-align:center;color:var(--og-muted,#94a3b8);font-size:13px}
  /* \u5361\u7247 */
  .og-card{fill:var(--og-node,#121a2e);stroke:var(--og-border,#243049);stroke-width:1.2}
  .og-node.sel .og-card,.og-node.sel .og-ifcard{stroke:var(--og-accent,#22d3ee);stroke-width:2}
  .og-st-experimental .og-card{stroke-dasharray:5 3;stroke:var(--og-warn,#f59e0b)}
  .og-st-active .og-card{stroke:var(--og-ok,#22c55e)}
  .og-st-deprecated .og-card{stroke:var(--og-muted,#64748b)}
  .og-hr{stroke:var(--og-border,#243049);stroke-width:1}
  .og-title{fill:var(--og-fg,#e6ecf5);font-size:13px;font-weight:700}
  .og-api{fill:var(--og-muted,#94a3b8);font-size:10.5px;font-family:var(--og-mono,ui-monospace,Menlo,monospace)}
  .og-prow{fill:var(--og-fg,#cbd5e1);font-size:11.5px}
  .og-pg{fill:var(--og-accent,#22d3ee)}
  .og-ty{fill:var(--og-muted,#94a3b8);font-size:10px}
  .og-sem{fill:var(--og-accent2,#6366f1);font-size:10px}
  .og-req{fill:var(--og-err,#ef4444);font-weight:700}
  .og-idx{fill:var(--og-warn,#f59e0b)}
  .og-more{fill:var(--og-muted,#64748b);font-size:10.5px;font-style:italic}
  /* \u4E1A\u52A1\u5355\u636E\u5C42\u5757\uFF08\u5934/\u884C/\u660E\u7EC6\uFF1A\u5355\u5361\u5185\u5D4C\u5957\u5206\u5C42\uFF09 */
  .og-level-band{fill:color-mix(in srgb,var(--og-accent,#22d3ee) 7%,transparent);stroke:color-mix(in srgb,var(--og-accent,#22d3ee) 22%,transparent);stroke-width:1}
  .og-level-bar{fill:var(--og-accent2,#8b5cf6)}
  .og-level-hd{fill:var(--og-fg,#e6ecf5);font-size:11px;font-weight:700}
  .og-level-kind{fill:var(--og-accent2,#a78bfa);font-size:9.5px;font-weight:600}
  .og-level-api{fill:var(--og-muted,#64748b);font-size:9px;font-family:var(--og-mono,ui-monospace,Menlo,monospace)}
  .og-port{fill:var(--og-accent,#22d3ee);stroke:var(--og-node,#121a2e);stroke-width:1.5;cursor:crosshair;opacity:.6;transition:opacity .12s ease, r .12s ease}
  .og-port:hover{opacity:1;r:6.5}
  .og-port.hot{opacity:1;r:8;fill:var(--og-ok,#22c55e);stroke:var(--og-node,#121a2e);stroke-width:2;filter:drop-shadow(0 0 4px var(--og-ok,#22c55e))}
  .og-canvas.og-readonly .og-port{display:none}
  /* \u63A5\u53E3 */
  .og-ifcard{fill:var(--og-iface,#1e1b3a);stroke:var(--og-accent2,#8b5cf6);stroke-dasharray:4 3;stroke-width:1.2}
  .og-iftag{fill:var(--og-accent2,#a78bfa);font-size:10px;font-style:italic}
  /* \u8FB9 */
  .og-edge{fill:none}
  .og-link{stroke:var(--og-edge,#64748b);stroke-width:1.6}
  .og-link.sel{stroke:var(--og-accent,#22d3ee);stroke-width:2.4}
  .og-hit{stroke:transparent;stroke-width:12;cursor:pointer}
  .og-seg{stroke:transparent;stroke-width:12;fill:none}
  .og-seg-H{cursor:ns-resize}
  .og-seg-V{cursor:ew-resize}
  .og-seg-H:hover,.og-seg-V:hover{stroke:var(--og-accent,#22d3ee);opacity:.35}
  .og-iflink{stroke:var(--og-accent2,#8b5cf6);stroke-width:1.3;stroke-dasharray:4 3;opacity:.7}
  .og-mk{fill:var(--og-edge,#64748b);stroke:none}
  .og-elabel{fill:var(--og-muted,#94a3b8);font-size:10.5px}
  .og-rubber{stroke:var(--og-accent,#22d3ee);stroke-width:1.8;stroke-dasharray:5 4;fill:none}
  /* \u5206\u57DF\u6298\u53E0\u5BB9\u5668 */
  .og-grp{cursor:pointer}
  .og-grp-box{fill:var(--og-node,#121a2e);stroke:var(--gc,#22d3ee);stroke-width:1.4}
  .og-grp-collapsed:hover .og-grp-box{fill:var(--og-node2,#17223c);stroke-width:2}
  .og-grp-bar{fill:var(--gc,#22d3ee)}
  .og-grp-caret{fill:var(--gc,#22d3ee);font-size:12px}
  .og-grp-label{fill:var(--og-fg,#e6ecf5);font-size:13.5px;font-weight:700}
  .og-grp-key{fill:var(--og-muted,#94a3b8);font-size:10px;font-family:var(--og-mono,ui-monospace,Menlo,monospace)}
  .og-grp-badge rect{fill:color-mix(in srgb,var(--gc,#22d3ee) 22%,transparent);stroke:var(--gc,#22d3ee);stroke-width:.8}
  .og-grp-badge text{fill:var(--gc,#22d3ee);font-size:11px;font-weight:700}
  .og-grp-region{fill:color-mix(in srgb,var(--gc,#22d3ee) 6%,transparent);stroke:var(--gc,#22d3ee);stroke-width:1.1;stroke-dasharray:3 4}
  .og-grp-head{fill:color-mix(in srgb,var(--gc,#22d3ee) 14%,transparent)}
  .og-grp-open>g[data-group-toggle]:hover .og-grp-head{fill:color-mix(in srgb,var(--gc,#22d3ee) 26%,transparent)}
  .og-bundle{stroke:var(--og-edge,#64748b);stroke-width:1.8;fill:none;opacity:.85}
  .og-bundle-cnt rect{fill:var(--og-bg,#0b1020);stroke:var(--og-edge,#64748b);stroke-width:.8}
  .og-bundle-cnt text{fill:var(--og-muted,#94a3b8);font-size:10.5px;font-weight:700}
  `;
}

// src/layout/groupLayout.ts
var PAD = 16;
var HEADER = 34;
var GAP = 26;
var BOX_W = 220;
var BOX_H = 62;
function pathOf(n) {
  return n.groupPath && n.groupPath.length ? n.groupPath : ["\u672A\u5206\u7EC4"];
}
function newG(key, label, depth) {
  return { key, label, depth, subMap: /* @__PURE__ */ new Map(), subs: [], leaves: [] };
}
function buildTree(nodes) {
  const root = newG("", "ROOT", -1);
  for (const n of nodes) {
    const path = pathOf(n);
    let cur = root;
    for (let i = 0; i < path.length; i++) {
      const key = path.slice(0, i + 1).join("/");
      let child = cur.subMap.get(key);
      if (!child) {
        child = newG(key, path[i], i);
        cur.subMap.set(key, child);
        cur.subs.push(child);
      }
      cur = child;
    }
    cur.leaves.push(n);
  }
  return root;
}
function countLeaves(g) {
  let c = g.leaves.length;
  for (const s of g.subs) c += countLeaves(s);
  return c;
}
function isCollapsed(collapsed, key) {
  return collapsed[key] ?? true;
}
function layoutGroup(g, ox, oy, cfg, collapsed) {
  const count = countLeaves(g);
  const collapsedNow = isCollapsed(collapsed, g.key);
  if (collapsedNow) {
    const rect2 = { x: ox, y: oy, w: BOX_W, h: BOX_H };
    return { rect: rect2, boxes: [{ key: g.key, label: g.label, count, depth: g.depth, collapsed: true, rect: rect2 }], pos: {} };
  }
  const children = [];
  for (const sub of g.subs) {
    const sl = layoutGroup(sub, 0, 0, cfg, collapsed);
    children.push({ w: sl.rect.w, h: sl.rect.h, kind: "group", sub: sl });
  }
  for (const leaf of g.leaves) children.push({ w: cfg.nodeW, h: nodeHeight(leaf, cfg), kind: "leaf", node: leaf });
  const boxes = [];
  const pos = {};
  const childX = ox + PAD;
  const childY = oy + HEADER;
  const cols = Math.max(1, Math.ceil(Math.sqrt(children.length)));
  let x = childX;
  let y = childY;
  let rowH = 0;
  let col = 0;
  let maxRight = childX + BOX_W;
  let maxBottom = childY;
  for (const c of children) {
    if (c.kind === "leaf") {
      pos[c.node.id] = { x, y, w: c.w, h: c.h };
    } else {
      const dx = x;
      const dy = y;
      for (const b of c.sub.boxes) boxes.push({ ...b, rect: { x: b.rect.x + dx, y: b.rect.y + dy, w: b.rect.w, h: b.rect.h } });
      for (const id in c.sub.pos) {
        const r = c.sub.pos[id];
        pos[id] = { x: r.x + dx, y: r.y + dy, w: r.w, h: r.h };
      }
    }
    maxRight = Math.max(maxRight, x + c.w);
    maxBottom = Math.max(maxBottom, y + c.h);
    rowH = Math.max(rowH, c.h);
    col++;
    x += c.w + GAP;
    if (col >= cols) {
      col = 0;
      x = childX;
      y += rowH + GAP;
      rowH = 0;
    }
  }
  const rect = { x: ox, y: oy, w: maxRight - ox + PAD, h: Math.max(maxBottom - oy + PAD, HEADER + PAD) };
  boxes.unshift({ key: g.key, label: g.label, count, depth: g.depth, collapsed: false, rect });
  return { rect, boxes, pos };
}
function groupLayout(def, cfg, collapsed) {
  const root = buildTree(def.nodes || []);
  const boxes = [];
  const pos = {};
  const cols = Math.max(1, Math.ceil(Math.sqrt(root.subs.length)));
  let x = PAD;
  let y = PAD;
  let rowH = 0;
  let col = 0;
  let maxRight = PAD;
  let maxBottom = PAD;
  for (const sub of root.subs) {
    const sl = layoutGroup(sub, x, y, cfg, collapsed);
    for (const b of sl.boxes) boxes.push(b);
    for (const id in sl.pos) pos[id] = sl.pos[id];
    maxRight = Math.max(maxRight, sl.rect.x + sl.rect.w);
    maxBottom = Math.max(maxBottom, sl.rect.y + sl.rect.h);
    rowH = Math.max(rowH, sl.rect.h);
    col++;
    x += sl.rect.w + GAP * 1.5;
    if (col >= cols) {
      col = 0;
      x = PAD;
      y += rowH + GAP * 1.5;
      rowH = 0;
    }
  }
  const boxByKey = new Map(boxes.map((b) => [b.key, b]));
  const nodePath = /* @__PURE__ */ new Map();
  for (const n of def.nodes || []) nodePath.set(n.id, pathOf(n));
  const resolveKey = (nodeId) => {
    if (pos[nodeId]) return nodeId;
    const path = nodePath.get(nodeId) || ["\u672A\u5206\u7EC4"];
    for (let i = 0; i < path.length; i++) {
      const key = path.slice(0, i + 1).join("/");
      if (isCollapsed(collapsed, key)) return key;
    }
    return nodeId;
  };
  const rectOf = (key) => pos[key] || (boxByKey.get(key) ? boxByKey.get(key).rect : void 0);
  return { boxes, pos, resolveKey, rectOf, width: maxRight + PAD, height: maxBottom + PAD };
}
function groupingActive(def) {
  const keys = /* @__PURE__ */ new Set();
  for (const n of def.nodes || []) {
    const p = pathOf(n);
    keys.add(p.join("/"));
    if (keys.size >= 2) return true;
  }
  return false;
}
function allGroupKeys(def) {
  const keys = /* @__PURE__ */ new Set();
  for (const n of def.nodes || []) {
    const p = pathOf(n);
    for (let i = 0; i < p.length; i++) keys.add(p.slice(0, i + 1).join("/"));
  }
  return [...keys];
}

// src/interaction/pointer.ts
var DRAG_THRESHOLD = 5;
var InteractionController = class {
  model;
  cb;
  mode = "idle";
  activeId = null;
  activeProp = null;
  activeSide = null;
  connectStart = { x: 0, y: 0 };
  segEdge = null;
  segI = 0;
  segOrient = "";
  segRoute = [];
  startX = 0;
  startY = 0;
  grabDX = 0;
  grabDY = 0;
  moved = false;
  pointerId = -1;
  /** connectOnly：分域折叠视图用——仅允许从属性锚点拉线建关系；节点重定位/线段手动布线（容器自动布局）留 M2。 */
  connectOnly = false;
  constructor(model, cb) {
    this.model = model;
    this.cb = cb;
  }
  /** 切换 connectOnly（分域折叠开、扁平图关）。 */
  setConnectOnly(v) {
    this.connectOnly = v;
  }
  toSvgPoint(clientX, clientY) {
    const svg = this.cb.getSvg();
    if (!svg) return { x: clientX, y: clientY };
    const rect = svg.getBoundingClientRect();
    const vb = svg.viewBox.baseVal;
    const sx = vb && vb.width ? vb.width / rect.width : 1;
    const sy = vb && vb.height ? vb.height / rect.height : 1;
    return { x: (clientX - rect.left) * sx, y: (clientY - rect.top) * sy };
  }
  onPointerDown(ev) {
    const target = ev.target;
    const segEl = this.connectOnly ? null : target.closest("[data-edge-seg]");
    if (segEl) {
      const grp = segEl.closest("[data-edge]");
      const routeStr = grp ? grp.getAttribute("data-route") : null;
      const route = routeStr ? routeStr.trim().split(/\s+/).map((s) => {
        const c = s.split(",");
        return { x: Number(c[0] ?? 0), y: Number(c[1] ?? 0) };
      }) : [];
      if (route.length >= 4) {
        this.mode = "segdrag";
        this.segEdge = segEl.getAttribute("data-edge-seg");
        this.segI = parseInt(segEl.getAttribute("data-seg-i") || "0", 10);
        this.segOrient = segEl.getAttribute("data-orient") || "";
        this.segRoute = route;
        this.moved = false;
        const pt = this.toSvgPoint(ev.clientX, ev.clientY);
        this.startX = pt.x;
        this.startY = pt.y;
        this.pointerId = ev.pointerId;
        this.safeCapture(ev);
        ev.preventDefault();
        return;
      }
    }
    const portEl = target.closest("[data-port]");
    const nodeEl = target.closest("[data-node]");
    if (portEl) {
      this.mode = "connect";
      this.activeId = portEl.getAttribute("data-port");
      this.activeProp = portEl.getAttribute("data-prop");
      this.activeSide = portEl.getAttribute("data-side");
      this.connectStart = {
        x: parseFloat(portEl.getAttribute("cx") || "0"),
        y: parseFloat(portEl.getAttribute("cy") || "0")
      };
      this.pointerId = ev.pointerId;
      this.safeCapture(ev);
      ev.preventDefault();
      ev.stopPropagation();
      return;
    }
    if (nodeEl) {
      if (this.connectOnly) return;
      const id = nodeEl.getAttribute("data-node");
      if (!id) return;
      this.mode = "drag";
      this.activeId = id;
      this.moved = false;
      this.pointerId = ev.pointerId;
      const pt = this.toSvgPoint(ev.clientX, ev.clientY);
      this.startX = pt.x;
      this.startY = pt.y;
      const pos = this.cb.getLayout().pos[id];
      this.grabDX = pos ? pt.x - pos.x : 0;
      this.grabDY = pos ? pt.y - pos.y : 0;
      this.safeCapture(ev);
      ev.preventDefault();
    }
  }
  safeCapture(ev) {
    try {
      ev.currentTarget.setPointerCapture?.(ev.pointerId);
    } catch {
    }
  }
  onPointerMove(ev) {
    if (this.mode === "idle" || ev.pointerId !== this.pointerId) return;
    const pt = this.toSvgPoint(ev.clientX, ev.clientY);
    if (this.mode === "drag" && this.activeId) {
      const dx = pt.x - this.startX;
      const dy = pt.y - this.startY;
      if (!this.moved && Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD) return;
      this.moved = true;
      this.cb.onNodeDrag(this.activeId, Math.max(0, pt.x - this.grabDX), Math.max(0, pt.y - this.grabDY));
    } else if (this.mode === "connect" && this.activeId) {
      this.cb.onHotPort(this.hitPort(pt.x, pt.y, { node: this.activeId, prop: this.activeProp, side: this.activeSide }));
      this.cb.onRubber(this.connectStart, { x: pt.x, y: pt.y });
    } else if (this.mode === "segdrag" && this.segEdge) {
      const dx = pt.x - this.startX;
      const dy = pt.y - this.startY;
      if (!this.moved && Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD) return;
      this.moved = true;
      const route = this.segRoute.map((p) => ({ x: p.x, y: p.y }));
      const a = route[this.segI];
      const b = route[this.segI + 1];
      const oa = this.segRoute[this.segI];
      const ob = this.segRoute[this.segI + 1];
      if (a && b && oa && ob) {
        if (this.segOrient === "H") {
          a.y = oa.y + dy;
          b.y = ob.y + dy;
        } else {
          a.x = oa.x + dx;
          b.x = ob.x + dx;
        }
      }
      this.cb.onSegmentDrag(this.segEdge, route);
    }
  }
  onPointerUp(ev) {
    if (this.mode === "idle" || ev.pointerId !== this.pointerId) return;
    if (this.mode === "drag" && this.activeId) {
      const pt = this.toSvgPoint(ev.clientX, ev.clientY);
      const moved = Math.abs(pt.x - this.startX) + Math.abs(pt.y - this.startY) >= DRAG_THRESHOLD;
      if (moved && this.moved) this.cb.onNodeDragEnd(this.activeId);
      else this.cb.onNodeSelect(this.activeId);
    } else if (this.mode === "connect" && this.activeId) {
      const pt = this.toSvgPoint(ev.clientX, ev.clientY);
      this.cb.onRubber(null, null);
      const tp = this.hitPort(pt.x, pt.y);
      if (tp) {
        const src = { node: this.activeId, prop: this.activeProp, side: this.activeSide };
        this.cb.onConnect(src, tp);
      }
    } else if (this.mode === "segdrag" && this.segEdge) {
      if (this.moved) this.cb.onSegmentDragEnd(this.segEdge);
    }
    this.reset(ev);
  }
  /** 吸附到最近的锚点（阈值内）；exclude 排除自身锚点（高亮时用）。返回 {node,prop,side} 或 null。 */
  hitPort(x, y, exclude) {
    const svg = this.cb.getSvg();
    if (!svg) return null;
    let best = null;
    let bestD = 26;
    svg.querySelectorAll("[data-port]").forEach((el) => {
      const cx = parseFloat(el.getAttribute("cx") || "NaN");
      const cy = parseFloat(el.getAttribute("cy") || "NaN");
      if (Number.isNaN(cx) || Number.isNaN(cy)) return;
      const node = el.getAttribute("data-port") || "";
      const prop = el.getAttribute("data-prop");
      const side = el.getAttribute("data-side");
      if (exclude && exclude.node === node && (exclude.prop || null) === (prop || null) && exclude.side === side) return;
      const d = Math.hypot(cx - x, cy - y);
      if (d < bestD) {
        bestD = d;
        best = { node, prop, side };
      }
    });
    return best;
  }
  onPointerCancel(ev) {
    this.cb.onRubber(null, null);
    this.reset(ev);
  }
  reset(ev) {
    try {
      ev.currentTarget?.releasePointerCapture?.(this.pointerId);
    } catch {
    }
    this.mode = "idle";
    this.activeId = null;
    this.activeProp = null;
    this.activeSide = null;
    this.segEdge = null;
    this.moved = false;
    this.pointerId = -1;
  }
};

// src/element/cmx-ontology-graph.ts
var CmxOntologyGraph = class extends HTMLElement {
  static get observedAttributes() {
    return ["data-spec", "readonly"];
  }
  root;
  model = new OntologyModel();
  selectedNodeId = null;
  selectedEdgeApiName = null;
  rubber = { from: null, to: null };
  hotPort = null;
  interaction;
  _readonly = false;
  _bootstrapped = false;
  constructor() {
    super();
    this.root = this.attachShadow({ mode: "open" });
    this.interaction = new InteractionController(this.model, {
      getLayout: () => this.currentLayout(),
      getSvg: () => this.root.querySelector("svg"),
      onNodeDrag: (id, x, y) => {
        this.model.setLayoutHint(id, x, y);
        for (const e of this.model.edges) if (e.source === id || e.target === id) this.model.clearEdgeRoute(e.apiName);
        this.paint();
      },
      onNodeDragEnd: (id) => {
        this.emit("spec-change", { spec: this.model.getDef() });
        void id;
      },
      onNodeSelect: (id) => this.selectNode(id),
      onRubber: (from, to) => {
        if (!from) this.hotPort = null;
        this.rubber = { from, to };
        this.paint();
      },
      onHotPort: (port) => {
        this.hotPort = port;
      },
      onConnect: (src, tgt) => this.requestConnect(src, tgt),
      onSegmentDrag: (apiName, points) => {
        this.model.setEdgeRoute(apiName, points);
        this.paint();
      },
      onSegmentDragEnd: (apiName) => {
        void apiName;
        this.emit("spec-change", { spec: this.model.getDef() });
      }
    });
  }
  connectedCallback() {
    if (this._readonly !== this.hasAttribute("readonly")) this._readonly = this.hasAttribute("readonly");
    if (!this._bootstrapped) {
      this._bootstrapped = true;
      const raw = this.getAttribute("data-spec");
      if (raw) {
        try {
          this.model.setDef(JSON.parse(raw));
        } catch {
        }
      }
    }
    this.render();
  }
  attributeChangedCallback(name, _old, value) {
    if (name === "readonly") {
      this._readonly = value != null;
      if (this.isConnected) this.render();
    } else if (name === "data-spec" && value != null && this._bootstrapped) {
      try {
        this.model.setDef(JSON.parse(value));
        this.render();
      } catch {
      }
    }
  }
  // ── 公共 API ──
  setSpec(def) {
    this.model.setDef(def);
    this.selectedNodeId = null;
    this.selectedEdgeApiName = null;
    if (this.isConnected) this.render();
  }
  getSpec() {
    return this.model.getDef();
  }
  getModel() {
    return this.model;
  }
  validate() {
    return this.model.validate();
  }
  /** 加对象类型（宿主命名后调）。返回 apiName 或 null（重名）。 */
  addObjectType(apiName, displayName) {
    const id = this.model.addObjectType(apiName, displayName);
    if (id) {
      this.render();
      this.selectNode(id);
      this.emit("spec-change", { spec: this.model.getDef() });
      this.emit("node-add", { nodeId: id, kind: "object" });
    }
    return id;
  }
  addInterface(apiName, displayName) {
    const id = this.model.addInterface(apiName, displayName);
    if (id) {
      this.render();
      this.selectNode(id);
      this.emit("spec-change", { spec: this.model.getDef() });
      this.emit("node-add", { nodeId: id, kind: "interface" });
    }
    return id;
  }
  /** 加关系边（宿主速建气泡确认后调）。返回 null 成功或拒绝原因。 */
  addLink(apiName, src, tgt, card, roleA, roleB) {
    const err = this.model.addLink(apiName, src, tgt, card, roleA, roleB);
    if (err) {
      this.emit("connect-rejected", { src, tgt, reason: err });
      return err;
    }
    this.render();
    this.emit("spec-change", { spec: this.model.getDef() });
    this.emit("link-added", { apiName, source: src, target: tgt });
    return null;
  }
  delNode(id) {
    this.model.delNode(id);
    if (this.selectedNodeId === id) this.selectedNodeId = null;
    this.render();
    this.emit("spec-change", { spec: this.model.getDef() });
    this.emit("node-del", { nodeId: id });
  }
  delLink(apiName) {
    this.model.delLink(apiName);
    if (this.selectedEdgeApiName === apiName) this.selectedEdgeApiName = null;
    this.render();
    this.emit("spec-change", { spec: this.model.getDef() });
  }
  /** 重排：**不重置**已有位置，仅在现有各图元位置基础上重画（未定位的新节点走网格）。 */
  autoLayout() {
    this.render();
    this.emit("spec-change", { spec: this.model.getDef() });
  }
  /** 工具栏「全部展开/收起」——批量设置分域容器折叠态（无分组时无副作用）。 */
  setAllGroups(collapsed) {
    this.model.setAllGroups(allGroupKeys(this.model.getDef()), collapsed);
    this.render();
    this.emit("spec-change", { spec: this.model.getDef() });
  }
  selectNode(id) {
    this.selectedNodeId = id;
    this.selectedEdgeApiName = null;
    this.paint();
    const n = this.model.node(id);
    this.emit("type-select", { nodeId: id, node: n ? JSON.parse(JSON.stringify(n)) : null });
  }
  /** 供宿主编辑节点后回写模型并重画。 */
  refresh() {
    this.render();
  }
  // ── 内部 ──
  cfg() {
    return this._readonly ? PREVIEW_LAYOUT : DEFAULT_LAYOUT;
  }
  currentLayout() {
    return layout(this.model.getDef(), this.cfg(), this.model.layoutHints());
  }
  /** 拉线落点 → 请求宿主补关系元数据（不直接建，交速建气泡）。属性锚点带出源/靶属性。 */
  requestConnect(src, tgt) {
    this.rubber = { from: null, to: null };
    this.paint();
    const detail = { source: src.node, target: tgt.node };
    if (src.prop) detail.sourceProperty = src.prop;
    if (tgt.prop) detail.targetProperty = tgt.prop;
    this.emit("link-add", detail);
  }
  emit(name, detail) {
    this.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true }));
  }
  renderState() {
    return {
      selectedNodeId: this.selectedNodeId,
      selectedEdgeApiName: this.selectedEdgeApiName,
      readonly: this._readonly,
      maxRows: this.cfg().maxRows,
      hotPort: this.hotPort
    };
  }
  /** 当前是否走分域折叠视图（对象数少/无分组 → 回退扁平图）。 */
  grouped() {
    return this.model.nodes.length > 0 && groupingActive(this.model.getDef());
  }
  /** 构建画布 SVG（分域折叠 或 扁平）。 */
  buildSvg() {
    if (!this.model.nodes.length) return '<div class="og-empty">\u7A7A\u672C\u4F53 \xB7 \u53CC\u51FB\u753B\u5E03\u6216\u70B9\u300C+ \u5BF9\u8C61\u7C7B\u578B\u300D\u5F00\u59CB</div>';
    const def = this.model.getDef();
    if (this.grouped()) {
      const gl = groupLayout(def, this.cfg(), this.model.groupCollapsed() || {});
      return renderGrouped(def, gl, this.cfg(), this.renderState());
    }
    return renderSvg(def, this.currentLayout(), this.cfg(), this.renderState());
  }
  render() {
    const grouped = this.grouped();
    const cls = `og-canvas${this._readonly ? " og-readonly" : ""}${grouped ? " og-grouped" : ""}`;
    this.root.innerHTML = `<style>${graphCss()}</style><div class="${cls}" part="canvas">${this.buildSvg()}</div>`;
    if (grouped) this.bindGrouped();
    else if (this._readonly) this.bindReadonlySelect();
    else this.bindInteractions();
  }
  paint() {
    const canvas = this.root.querySelector(".og-canvas");
    if (!canvas) {
      this.render();
      return;
    }
    canvas.classList.toggle("og-connecting", !!this.rubber.from);
    let svg = this.buildSvg();
    if (this.rubber.from && this.rubber.to) {
      const r = `<path class="og-rubber" d="M${this.rubber.from.x},${this.rubber.from.y} L${this.rubber.to.x},${this.rubber.to.y}"/>`;
      svg = svg.replace("</svg>", r + "</svg>");
    }
    canvas.innerHTML = svg;
  }
  bindInteractions() {
    const canvas = this.root.querySelector(".og-canvas");
    if (!canvas || canvas.__ogBound) return;
    canvas.__ogBound = true;
    this.interaction.setConnectOnly(false);
    canvas.addEventListener("pointerdown", (e) => this.interaction.onPointerDown(e));
    canvas.addEventListener("pointermove", (e) => this.interaction.onPointerMove(e));
    canvas.addEventListener("pointerup", (e) => this.interaction.onPointerUp(e));
    canvas.addEventListener("pointercancel", (e) => this.interaction.onPointerCancel(e));
    canvas.addEventListener("click", (e) => {
      const el = e.target.closest("[data-edge]");
      if (el) {
        this.selectedEdgeApiName = el.getAttribute("data-edge");
        this.selectedNodeId = null;
        this.paint();
        const e2 = this.model.edges.find((x) => x.apiName === this.selectedEdgeApiName);
        this.emit("edge-select", { apiName: this.selectedEdgeApiName, edge: e2 ? JSON.parse(JSON.stringify(e2)) : null });
      }
    });
  }
  bindReadonlySelect() {
    const canvas = this.root.querySelector(".og-canvas");
    if (!canvas || canvas.__ogRoBound) return;
    canvas.__ogRoBound = true;
    canvas.addEventListener("click", (e) => {
      const edgeEl = e.target.closest("[data-edge]");
      if (edgeEl) {
        this.selectedEdgeApiName = edgeEl.getAttribute("data-edge");
        this.selectedNodeId = null;
        this.paint();
        return;
      }
      const nodeEl = e.target.closest("[data-node]");
      if (nodeEl) {
        const id = nodeEl.getAttribute("data-node");
        if (id) this.selectNode(id);
      }
    });
  }
  /** 分域折叠视图交互：容器折叠切换 + 节点/边选中 + **从属性锚点拉线建关系**（节点重定位留 M2）。 */
  bindGrouped() {
    const canvas = this.root.querySelector(".og-canvas");
    if (!canvas || canvas.__ogGrpBound) return;
    canvas.__ogGrpBound = true;
    this.interaction.setConnectOnly(true);
    canvas.addEventListener("pointerdown", (e) => this.interaction.onPointerDown(e));
    canvas.addEventListener("pointermove", (e) => this.interaction.onPointerMove(e));
    canvas.addEventListener("pointerup", (e) => this.interaction.onPointerUp(e));
    canvas.addEventListener("pointercancel", (e) => this.interaction.onPointerCancel(e));
    canvas.addEventListener("click", (e) => {
      const tgl = e.target.closest("[data-group-toggle]");
      if (tgl) {
        const key = tgl.getAttribute("data-group-toggle");
        if (key) {
          this.model.toggleGroup(key);
          this.render();
          this.emit("spec-change", { spec: this.model.getDef() });
        }
        return;
      }
      const edgeEl = e.target.closest("[data-edge]");
      if (edgeEl) {
        this.selectedEdgeApiName = edgeEl.getAttribute("data-edge");
        this.selectedNodeId = null;
        this.paint();
        const e2 = this.model.edges.find((x) => x.apiName === this.selectedEdgeApiName);
        this.emit("edge-select", { apiName: this.selectedEdgeApiName, edge: e2 ? JSON.parse(JSON.stringify(e2)) : null });
        return;
      }
      const nodeEl = e.target.closest("[data-node]");
      if (nodeEl) {
        const id = nodeEl.getAttribute("data-node");
        if (id) this.selectNode(id);
      }
    });
  }
};
function defineOntologyGraph() {
  if (typeof customElements !== "undefined" && !customElements.get("cmx-ontology-graph")) {
    customElements.define("cmx-ontology-graph", CmxOntologyGraph);
  }
}

// src/index.ts
var VERSION = "1.0.0";
defineOntologyGraph();
export {
  CARDINALITY_LABEL,
  CmxOntologyGraph,
  DEFAULT_LAYOUT,
  OntologyModel,
  PREVIEW_LAYOUT,
  STATUS_LABEL,
  VERSION,
  defineOntologyGraph,
  graphCss,
  layout,
  nodeHeight,
  renderSvg
};
