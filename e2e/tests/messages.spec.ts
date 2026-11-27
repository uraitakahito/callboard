// メッセージの一覧（/messages）を、本物の pagetray を相手に、本物の browser で見る。pagetray の e2e/tests/messages.spec.ts を移した物。
// HTML の字は crates/server/tests/messages.rs（Rust）が見るので、ここは browser でしか見えない物（script の動き・本文が HTML として
// 動かないこと・新着の知らせの中継）と、人が押してたどる道を見る。
// 「まだ届いていない n 件」は置き場の全部を数えるので、まだ届いていないにした記録は、試験の終わりまでに届いたにする。
import { expect, test } from '@playwright/test';
import { choice, collectProblems, fresh, messageRow, messageRows, openLive, placePage, putMessage, undelivered, type Message } from './helpers';

const message = (change: Partial<Message> = {}): Message => ({
  heading: 'request',
  from: 't-1.swe.1',
  to: 'tpm',
  text: '本文',
  sentAt: '2026-11-01T00:00:00Z',
  state: 'delivered',
  ...change,
});

test('頭の帯の「メッセージ」は callboard の一覧を開き、操作の帯の「ページ」は callboard のページの一覧を指す', async ({ page, request }) => {
  await putMessage(request, 'msg-link', 'm-1', message());
  await page.goto('/');
  await page.locator('header.bar').getByRole('link', { name: 'メッセージ', exact: true }).click();
  await expect(page).toHaveURL(/\/messages$/);
  await expect(page).toHaveTitle('メッセージの記録 ・ callboard');
  await expect(messageRow(page, 'msg-link', 'm-1')).toBeVisible();
  await expect(page.locator('#toolbar').getByRole('link', { name: 'ページ', exact: true })).toHaveAttribute('href', '/pages/');
});

test('行: 送った時刻・Key・見出し・送り手と受け手・状態の字・届いた時刻・渡した時・渡し直しの数。絞り込める。console の誤りも CSP の違反も無い', async ({ page, request }) => {
  const problems = collectProblems(page);
  await putMessage(
    request,
    'msg-rows',
    'm-1',
    message({ key: 'A-1', heading: 'answer', from: 'pm.1', to: 't-1.swe.1', text: 'りんごにする', deliveredAt: '2026-11-01T00:00:07Z', via: 'session-start', deliveries: 3 }),
  );
  await putMessage(request, 'msg-rows', 'm-2', message({ sentAt: '2026-11-02T00:00:00Z', state: 'expired', via: 'ttl', text: 'みかん' }));
  await putMessage(request, 'msg-rows', 'm-3', message({ sentAt: '2026-11-03T00:00:00Z', state: 'carried', to: 'pm.1', text: 'ぶどう' }));

  await openLive(page, '/messages?project=msg-rows');
  await expect(messageRows(page)).toHaveCount(3);
  await expect(messageRows(page).locator('.state')).toHaveText(['ほかの道具で運んだ', '期限切れ', '届いた']);
  const row = messageRow(page, 'msg-rows', 'm-1');
  // UTC の 00:00 は、日本の時刻で 09:00（callboard が日本の時刻で組む。script は時刻を書き替えない）。
  await expect(row.locator('.top time')).toHaveText('11-01 09:00');
  await expect(row.locator('.top')).toContainText('A-1');
  await expect(row.locator('.name')).toHaveText('answer');
  await expect(row.locator('.route')).toHaveText('pm.1 → t-1.swe.1');
  await expect(row.locator('.who')).toContainText('届いた 11-01 09:00');
  await expect(row.locator('.who')).toContainText('渡した時: session-start');
  // 決めてある名（ttl など）は、画面の字にして出す。決めていない名（上の session-start）は、書かれた字のまま。
  await expect(messageRow(page, 'msg-rows', 'm-2').locator('.who')).toHaveText('渡した時: 期限で消えた');
  await expect(row.locator('.who')).toContainText('渡し直し 2 度');
  await expect(row.locator('pre')).toHaveText('りんごにする');

  // 絞り込み: 候補を押すと合う行だけになり、もう一度押すと外れる。
  await choice(page, '受け手', 'pm.1').click();
  await expect(page).toHaveURL(/[?&]to=pm\.1/);
  await expect(messageRows(page)).toHaveCount(1);
  await expect(messageRows(page).locator('pre')).toHaveText('ぶどう');
  await choice(page, '受け手', 'pm.1').click();
  await expect(messageRows(page)).toHaveCount(3);
  await choice(page, '状態', '期限切れ').click();
  await expect(page).toHaveURL(/[?&]state=expired/);
  await expect(messageRows(page).locator('pre')).toHaveText('みかん');
  expect(problems).toEqual([]);
});

test('本文: HTML として動かない。長い本文は頭だけ見え、開くと全部', async ({ page, request }) => {
  const problems = collectProblems(page);
  const evil = '<img src=x onerror="window.__pwned = 1"><script>window.__pwned = 2</script><b>太字</b>';
  const long = `頭の行\n${'あ'.repeat(400)}終わり`;
  await putMessage(request, 'msg-text', 'm-1', message({ text: evil, from: '<i>from</i>' }));
  await putMessage(request, 'msg-text', 'm-2', message({ sentAt: '2026-11-02T00:00:00Z', text: long }));

  await page.goto('/messages?project=msg-text');
  const row = messageRow(page, 'msg-text', 'm-1');
  await expect(row.locator('pre')).toHaveText(evil);
  await expect(row.locator('.route')).toHaveText('<i>from</i> → tpm');
  await expect(page.locator('#list img, #list b, #list i, #list script')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__pwned)).toBeUndefined();

  const folded = messageRow(page, 'msg-text', 'm-2');
  await expect(folded.locator('summary')).toContainText('頭の行');
  await expect(folded.locator('pre')).toBeHidden();
  await folded.locator('summary').click();
  await expect(folded.locator('pre')).toBeVisible();
  await expect(folded.locator('pre')).toHaveText(long);
  expect(problems).toEqual([]);
});

test('新着の知らせ: 置かれると押さずに頭に入り、書き足されると状態と数が替わる。開いた本文は開いたまま', async ({ page, request }) => {
  const problems = collectProblems(page);
  const long = `長い本文\n${'あ'.repeat(400)}`;
  await putMessage(request, 'msg-live', 'm-1', message({ text: long }));
  await openLive(page, '/messages?project=msg-live');
  await expect(messageRows(page)).toHaveCount(1);
  await expect(undelivered(page)).toBeHidden();
  await messageRow(page, 'msg-live', 'm-1').locator('summary').click();
  // 画面ごと読み直したら消える印。
  await page.evaluate(() => ((window as any).__kept = true));

  // ほかの project の記録は、一覧の列に入らない（数は置き場の全部なので増える）。
  await putMessage(request, 'msg-live-other', 'm-1', message({ sentAt: '2026-11-02T00:00:00Z', state: 'waiting' }));
  await expect(undelivered(page)).toHaveText('まだ届いていない 1 件');
  await expect(messageRows(page)).toHaveCount(1);

  await putMessage(request, 'msg-live', 'm-2', message({ sentAt: '2026-11-03T00:00:00Z', state: 'waiting', text: '新しく置いた' }));
  await expect(messageRows(page)).toHaveCount(2);
  await expect(messageRows(page).first().locator('pre')).toHaveText('新しく置いた');
  await expect(messageRows(page).first().locator('.state')).toHaveText('まだ届いていない');
  await expect(undelivered(page)).toHaveText('まだ届いていない 2 件');
  await expect(page).toHaveTitle('まだ届いていない 2 件 ・ メッセージの記録 ・ callboard');
  // 絞り込みの列も新しくなる。
  await expect(choice(page, '状態', 'まだ届いていない').locator('.n')).toHaveText('2');
  // 開いていた本文は、差し替えても開いたまま。
  await expect(messageRow(page, 'msg-live', 'm-1').locator('details pre')).toBeVisible();

  // 書き足すと、押さずに状態が替わる。
  await putMessage(request, 'msg-live', 'm-2', message({ sentAt: '2026-11-03T00:00:00Z', state: 'delivered', deliveredAt: '2026-11-03T00:00:09Z', text: '新しく置いた' }));
  await expect(messageRow(page, 'msg-live', 'm-2').locator('.state')).toHaveText('届いた');
  await expect(messageRow(page, 'msg-live', 'm-2').locator('.who')).toContainText('届いた 11-03 09:00');
  await expect(undelivered(page)).toHaveText('まだ届いていない 1 件');
  await putMessage(request, 'msg-live-other', 'm-1', message({ sentAt: '2026-11-02T00:00:00Z', state: 'delivered' }));
  await expect(undelivered(page)).toBeHidden();
  await expect(page).toHaveTitle('メッセージの記録 ・ callboard');
  await expect(fresh(page)).toBeHidden();
  expect(await page.evaluate(() => (window as any).__kept)).toBe(true);
  expect(problems).toEqual([]);
});

test('新着の知らせ: 下へ送って見ている時は、頭に入った数を印に出し、押すと頭へ戻る（広い幅と狭い幅）', async ({ page, request }) => {
  for (const [width, project] of [
    [1280, 'msg-down-wide'],
    [800, 'msg-down-narrow'],
  ] as const) {
    await page.setViewportSize({ width, height: 600 });
    for (let i = 0; i < 30; i += 1) {
      await putMessage(request, project, `m-${i}`, message({ sentAt: `2026-10-${String(i + 1).padStart(2, '0')}T00:00:00Z`, text: `記録 ${i}\n2 行目\n3 行目` }));
    }
    await openLive(page, `/messages?project=${project}`);
    await expect(messageRows(page)).toHaveCount(30);
    // 一覧の列を下へ送る（広い幅では一覧の列の枠、狭い幅では画面全体）。
    await page.evaluate(() => {
      const list = document.getElementById('list')!;
      const box = list.parentElement!;
      if (box.scrollHeight > box.clientHeight) box.scrollTop = 600;
      else window.scrollTo(0, list.getBoundingClientRect().top + window.scrollY + 600);
    });
    const seen = messageRow(page, project, 'm-20');
    const before = (await seen.boundingBox())!.y;
    await putMessage(request, project, 'new-1', message({ sentAt: '2026-11-20T00:00:00Z', text: '新しい 1' }));
    await putMessage(request, project, 'new-2', message({ sentAt: '2026-11-21T00:00:00Z', text: '新しい 2' }));
    await expect(fresh(page)).toHaveText('● 新着 2 件（上へ）');
    await expect(messageRows(page)).toHaveCount(32);
    // 見ていた行は、同じ所にある。
    expect(Math.abs((await seen.boundingBox())!.y - before)).toBeLessThanOrEqual(2);
    await fresh(page).click();
    await expect(fresh(page)).toBeHidden();
    await expect(messageRow(page, project, 'new-2')).toBeInViewport();
    await expect(page).toHaveURL(new RegExp(`project=${project}$`));
  }
});

test('ページの知らせでは読み直さない。メッセージの記録の知らせで、1 度だけ読み直す', async ({ page, request }) => {
  await putMessage(request, 'msg-pages', 'm-1', message());
  await openLive(page, '/messages?project=msg-pages');
  await expect(messageRows(page)).toHaveCount(1);
  let pulls = 0;
  page.on('request', (r) => {
    if (new URL(r.url()).pathname === '/messages') pulls += 1;
  });
  // ページを置いた知らせ（名の無い event）も同じ流れで来るが、メッセージの一覧は読み直さない。
  await placePage(request, 'msg-pages', 'K-1', 'plan', '<title>一枚目</title>');
  await putMessage(request, 'msg-pages', 'm-2', message({ sentAt: '2026-11-02T00:00:00Z' }));
  await expect(messageRows(page)).toHaveCount(2);
  expect(pulls).toBe(1);
});

test('知らせにつなげなかった時（pagetray が止まっている間の 503）は、待ってからつなぎ直し、切れていた間の記録を入れる', async ({ page, request }) => {
  await putMessage(request, 'msg-retry', 'm-1', message());
  // 初めの 1 度だけ、callboard が「pagetray に聞けません」と答えた事にする。
  let asked = 0;
  await page.route('**/api/pagetray/events', async (route) => {
    asked += 1;
    if (asked === 1) {
      await route.fulfill({ status: 503, contentType: 'application/json', headers: { 'x-amzn-errortype': 'Unavailable' }, body: '{"message":"pagetray に聞けません（つながりません）"}' });
    } else {
      await route.continue();
    }
  });
  await page.goto('/messages?project=msg-retry');
  await expect(messageRows(page)).toHaveCount(1);
  await expect.poll(() => asked).toBe(1);
  // つながっていない間に置かれた記録。
  await putMessage(request, 'msg-retry', 'm-2', message({ sentAt: '2026-11-02T00:00:00Z', text: '切れていた間' }));
  // 3 秒待ってつなぎ直し、つながったら読み直す。
  await expect(messageRows(page)).toHaveCount(2, { timeout: 10_000 });
  expect(asked).toBe(2);
  // その後の知らせも届く。
  await putMessage(request, 'msg-retry', 'm-3', message({ sentAt: '2026-11-03T00:00:00Z' }));
  await expect(messageRows(page)).toHaveCount(3);
});

// --- 続きの頁 ---
// pagetray の画面（古い画面）と中身を比べていた試験は、pagetray が 2.0.0 で画面を外したので外した（比べる相手が無い。
// 外す前の比べの結果は、AI-32 の成果のページと、callboard の PR #7・#8 にある）。比べていた事のうち、続きの頁を辿る事は残す。

test('続きの link を辿ると、201 件が 200 件と 1 件の 2 頁に出る（境に同じ時刻の行が並んでも抜けない）', async ({ page, request }) => {
  for (let i = 0; i < 201; i += 1) {
    // 境に、同じ時刻の行を並べる。
    const minute = String(Math.floor(i / 3) % 60).padStart(2, '0');
    await putMessage(request, 'more-many', `m-${String(i).padStart(3, '0')}`, message({ sentAt: `2026-10-01T0${Math.floor(i / 180)}:${minute}:00Z`, to: `seat-${i % 7}`, text: `記録 ${i}` }));
  }
  await page.goto('/messages?project=more-many');
  await expect(messageRows(page)).toHaveCount(200);
  const seen = new Set(await messageRows(page).evaluateAll((rows) => rows.map((r) => r.getAttribute('data-id'))));
  await page.getByRole('link', { name: '続きを出す' }).click();
  await expect(page).toHaveURL(/\/messages\?project=more-many&at=[0-9a-f]+$/);
  await expect(messageRows(page)).toHaveCount(1);
  const last = await messageRows(page).first().getAttribute('data-id');
  expect(seen.has(last), `${last} が 2 度出た`).toBe(false);
  await expect(page.getByRole('link', { name: '続きを出す' })).toHaveCount(0);
});

test('同じ名の引数を 2 度書いた物と、値の形が違う引数と読めない続きの印は、400 の画面で訳を書く', async ({ request }) => {
  const twice = await request.get('/messages?to=a&to=b');
  expect(twice.status()).toBe(400);
  expect(await twice.text()).toContain('引数を読めません（to を 2 度書いています）');
  for (const query of [`?to=${'a'.repeat(201)}`, '?key=a%2Fb', '?at=zz']) {
    const now = await request.get(`/messages${query}`);
    expect(now.status(), query).toBe(400);
    expect(await now.text(), query).toContain('pagetray が引数を断りました');
  }
});
