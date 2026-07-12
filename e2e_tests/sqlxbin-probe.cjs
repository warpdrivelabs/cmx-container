// 探测 sqlx-zmc-msgpack 装载链路:loadDocData binary 解码 → dsMap
const { chromium } = require('/Users/nanomesh/node_modules/playwright');
const BASE = 'http://localhost:8080';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await (await browser.newContext()).newPage();
  const log = (...a) => console.log('[E2E]', ...a);
  page.on('console', m => { const t = m.text(); if (/msgpack|装载|decode|error|fromJSON/i.test(t)) console.log('  [page]', t.slice(0,160)); });
  try {
    await page.goto(BASE + '/portal/', { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.waitForTimeout(2500);
    const us = 'input[type="text"], input[name*="user" i]';
    if (await page.locator(us).first().isVisible().catch(() => false)) {
      await page.locator(us).first().fill('admin');
      await page.locator('input[type="password"]').first().fill('cmxadmin');
      await page.keyboard.press('Enter'); await page.waitForTimeout(3000);
    }
    log('登录完成');

    const out = await page.evaluate(async () => {
      const C = globalThis.__cmxDataComp;
      if (!C || typeof C.loadDocData !== 'function') return { err: 'no loadDocData' };
      const def = { domain:'fi', application:'cmxfico', module:'gl', file:'cmxfico_doc_meta_v1.json', limit:50, dbId:'fico-db', binary:true, apiPath:'/api/doc/data/sqlx-zmc-msgpack' };
      try {
        const r = await C.loadDocData(null, def);
        const dsMap = r && r.dsMap;
        const pkg = r && r.pkg;
        return {
          ok: true,
          dsKeys: dsMap ? Object.keys(dsMap) : null,
          rootLen: dsMap && pkg && dsMap[pkg.datasetId] ? dsMap[pkg.datasetId].length : null,
          pkgDatasetId: pkg && pkg.datasetId,
          pkgCols: pkg && pkg.columns ? pkg.columns.length : null,
          pkgRows: pkg && pkg.rows ? pkg.rows.length : null,
        };
      } catch (e) { return { err: e && e.message }; }
    });
    log('loadDocData(binary) 结果:', JSON.stringify(out));

    const outJson = await page.evaluate(async () => {
      const C = globalThis.__cmxDataComp;
      try {
        const r = await C.loadDocData(null, { domain:'fi', application:'cmxfico', module:'gl', file:'cmxfico_doc_meta_v1.json', limit:50, dbId:'fico-db' });
        return { ok:true, rows: r.pkg && r.pkg.rows ? r.pkg.rows.length : null, dsKeys: Object.keys(r.dsMap||{}) };
      } catch (e) { return { err: e && e.message }; }
    });
    log('JSON 对照:', JSON.stringify(outJson));
    process.exitCode = 0;
  } catch (e) { console.error('[E2E] 异常:', e.message); process.exitCode = 2; }
  finally { await browser.close(); }
})();
