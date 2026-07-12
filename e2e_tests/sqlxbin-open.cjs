// 真实打开"会计凭证(Sqlx 二进制)"菜单页,捕获 alert/console/网格行数
const { chromium } = require('/Users/nanomesh/node_modules/playwright');
const BASE = 'http://localhost:8080';
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await (await browser.newContext()).newPage();
  const log = (...a) => console.log('[E2E]', ...a);
  const alerts = [];
  page.on('dialog', async d => { alerts.push(d.message()); await d.accept().catch(()=>{}); });
  page.on('console', m => { const t=m.text(); if(/凭证|装载|msgpack|error|Sqlx|协调|loadDoc|fromJSON/i.test(t)) console.log('  [page]', t.slice(0,150)); });
  page.on('pageerror', e => console.log('  [pageerror]', (e.message||'').slice(0,160)));
  page.on('response', r => { if(r.url().includes('/api/doc/data')) log('NET', r.status(), r.url().split('/api/doc/data/')[1].split('?')[0]); });
  try {
    await page.goto(BASE + '/portal/', { waitUntil:'domcontentloaded', timeout:30000 });
    await page.waitForTimeout(2500);
    const us='input[type="text"], input[name*="user" i]';
    if (await page.locator(us).first().isVisible().catch(()=>false)) {
      await page.locator(us).first().fill('admin');
      await page.locator('input[type="password"]').first().fill('cmxadmin');
      await page.keyboard.press('Enter'); await page.waitForTimeout(3000);
    }
    log('登录完成');
    const cand = ['Sqlx 二进制','Sqlx','二进制'];
    let clicked=false;
    for (const t of cand) {
      const n = page.getByText(t, { exact:false }).first();
      if (await n.isVisible().catch(()=>false)) { await n.click(); clicked=true; log('点开菜单:', t); break; }
    }
    if(!clicked) log('未找到菜单项(候选:'+cand.join('/')+')');
    await page.waitForTimeout(5000);
    const grids = await page.evaluate(() => {
      const out=[];
      (function walk(r){ for(const el of r.querySelectorAll('*')){ if(el.tagName && el.tagName.toLowerCase()==='cmx-revo-grid'){ out.push({ id: el.id, rows: (el._rows&&el._rows.length) }); } if(el.shadowRoot) walk(el.shadowRoot);} })(document);
      return out;
    });
    log('alerts:', JSON.stringify(alerts));
    log('grids:', JSON.stringify(grids));
    process.exitCode=0;
  } catch(e){ console.error('[E2E] 异常:', e.message); process.exitCode=2; }
  finally { await browser.close(); }
})();
