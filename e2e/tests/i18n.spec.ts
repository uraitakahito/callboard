// 画面の字の言語（AI-50）。サーバーは日本語の字を書き、英語の時はブラウザーの i18next（static/i18n.js）が差し替える。
// 言語は cookie（callboard-lang）→ ブラウザーの言語の並びの初めの 1 つ → 日本語。帯の切り替えで選べる。
import { expect, test, type APIRequestContext, type Browser, type Page } from '@playwright/test';
import { CASTLIST, collectProblems, openLive, placePage, putMessage, reply, reviewMarks } from './helpers';

/** 頭の帯の画面の link の字（出た順）。 */
const navTexts = (page: Page) => page.locator('header.bar nav a').allTextContents();

const JA_NAV = ['アクター', 'タイムライン', '相関図', '使用量', 'ページ', 'メッセージ'];
const EN_NAV = ['Actors', 'Timeline', 'Network', 'Usage', 'Pages', 'Messages'];

/** 鍵を見る画面: 7 つの画面と、プロフィール（seed のアクター se.1）。 */
async function screens(request: APIRequestContext): Promise<string[]> {
  const response = await request.get(`${CASTLIST}/api/graph`);
  expect(response.ok()).toBeTruthy();
  const se1 = (await response.json()).nodes.find((n: { name: string }) => n.name === 'se.1');
  expect(se1, 'se.1 が無い').toBeTruthy();
  return ['/', '/actors/', '/graph/timeline', '/graph/network', '/usage/', '/messages', '/pages/', `/actors/${se1.id}`];
}

/** ブラウザーの言語を決めた新しい画面。 */
async function open(browser: Browser, locale: string, javaScriptEnabled = true) {
  const context = await browser.newContext({ locale, javaScriptEnabled, baseURL: test.info().project.use.baseURL });
  const page = await context.newPage();
  return { context, page };
}

test('ブラウザーが英語なら、帯を英語に差し替え、lang を en にし、切り替えを出す。console の誤りも CSP の違反も無い', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  const problems = collectProblems(page);
  await page.goto('/');
  expect(await navTexts(page)).toEqual(EN_NAV);
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.locator('html')).not.toHaveClass(/i18n-wait/);
  await expect(page.locator('header.bar nav')).toHaveAttribute('aria-label', 'Screens');
  await expect(page.locator('#lang-switch')).toBeVisible();
  await expect(page.locator('#lang-switch button[data-lang="en"]')).toHaveAttribute('aria-pressed', 'true');
  expect(problems).toEqual([]);
  await context.close();
});

test('ブラウザーが日本語なら、字を差し替えず、隠しもしない', async ({ page }) => {
  const problems = collectProblems(page);
  await page.goto('/');
  expect(await navTexts(page)).toEqual(JA_NAV);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  await expect(page.locator('#lang-switch button[data-lang="ja"]')).toHaveAttribute('aria-pressed', 'true');
  expect(problems).toEqual([]);
});

test('ブラウザーの言語の並びは初めの 1 つだけを見る（fr・en の並びは日本語）', async ({ browser }) => {
  const { context, page } = await open(browser, 'fr-FR');
  await page.addInitScript(() => Object.defineProperty(navigator, 'languages', { get: () => ['fr-FR', 'en-US'] }));
  await page.goto('/');
  expect(await page.evaluate(() => navigator.languages)).toEqual(['fr-FR', 'en-US']);
  expect(await navTexts(page)).toEqual(JA_NAV);
  await context.close();
});

test('帯の切り替えは cookie に残り、読み直しても、ほかの画面でも効く', async ({ browser }) => {
  const { context, page } = await open(browser, 'ja-JP');
  await page.goto('/');
  await page.locator('#lang-switch button[data-lang="en"]').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  expect(await navTexts(page)).toEqual(EN_NAV);
  const cookie = (await context.cookies()).find((c) => c.name === 'callboard-lang');
  expect(cookie?.value).toBe('en');
  expect(cookie?.sameSite).toBe('Lax');
  await page.goto('/actors/');
  expect(await navTexts(page)).toEqual(EN_NAV);
  await page.locator('#lang-switch button[data-lang="ja"]').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  expect(await navTexts(page)).toEqual(JA_NAV);
  await context.close();
});

test('JavaScript を切ると、英語のブラウザーでも日本語で読め、切り替えは出さない', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US', false);
  await page.goto('/graph/timeline');
  expect(await navTexts(page)).toEqual(JA_NAV);
  await expect(page.locator('#lang-switch')).toBeHidden();
  await expect(page.locator('body')).toBeVisible();
  await context.close();
});

test('i18n.js が届かなければ、日本語で見え、鍵の字は出ない', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  await page.route(/\/static\/i18n\.js/, (route) => route.fulfill({ status: 404, body: '' }));
  await page.goto('/');
  await expect(page.locator('body')).toBeVisible();
  expect(await navTexts(page)).toEqual(JA_NAV);
  await expect(page.locator('body')).not.toContainText('nav.');
  await context.close();
});

test('英語の訳が届かなければ、日本語のまま（lang も ja）', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  await page.route(/\/static\/locales\/en\.js/, (route) => route.fulfill({ status: 404, body: '' }));
  await page.goto('/');
  await expect(page.locator('body')).toBeVisible();
  expect(await navTexts(page)).toEqual(JA_NAV);
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  await context.close();
});

test('差し替えの途中で止まっても、CSS の守りが 1.5 秒で本文を見せる', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  // 印（i18n-wait）を付けるだけで、外さない i18n.js に替える。
  await page.route(/\/static\/i18n\.js/, (route) =>
    route.fulfill({ status: 200, contentType: 'application/javascript', body: "document.documentElement.classList.add('i18n-wait');" }),
  );
  // load まで待つと、遅い時に守りがもう効いてしまうので、DOM を読み終えた所で見る。
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('html')).toHaveClass(/i18n-wait/);
  const visibility = () => page.locator('body').evaluate((b) => getComputedStyle(b).visibility);
  expect(await visibility()).toBe('hidden');
  await expect.poll(visibility, { timeout: 3_000 }).toBe('visible');
  await context.close();
});

test('サーバーが書いた日本語の字は、ブラウザーの i18next が引く日本語の字と同じ', async ({ page, request }) => {
  for (const path of await screens(request)) {
    await page.goto(path);
    const mismatches = await page.evaluate(() => {
      const out: string[] = [];
      for (const element of document.querySelectorAll('[data-i18n]')) {
        const key = element.getAttribute('data-i18n')!;
        const raw = element.getAttribute('data-i18n-options');
        const want = (window as unknown as { callboardT: (k: string, o?: unknown) => string }).callboardT(key, raw ? JSON.parse(raw) : undefined);
        if (element.textContent !== want) out.push(`${key}: ${element.textContent} ≠ ${want}`);
      }
      return out;
    });
    expect(mismatches, path).toEqual([]);
  }
});

test('壊れた cookie でも止まらず、ブラウザーの言語で出し、切り替えも出る', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  const problems = collectProblems(page);
  await context.addCookies([{ name: 'callboard-lang', value: '%E0%A4', url: test.info().project.use.baseURL! }]);
  await page.goto('/');
  expect(await navTexts(page)).toEqual(EN_NAV);
  await expect(page.locator('#lang-switch')).toBeVisible();
  await page.locator('#lang-switch button[data-lang="ja"]').click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
  expect(problems).toEqual([]);
  await context.close();
});

test('i18next が届かなくても、日本語で見え、callboardT は日本語の字を返し、script の誤りは出ない', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route(/\/static\/vendor\/i18next-/, (route) => route.fulfill({ status: 404, body: '' }));
  await page.goto('/');
  await expect(page.locator('body')).toBeVisible();
  expect(await navTexts(page)).toEqual(JA_NAV);
  expect(await page.evaluate(() => (window as unknown as { callboardT: (k: string) => string }).callboardT('nav.actors'))).toBe('アクター');
  expect(errors).toEqual([]);
  await context.close();
});

test('cookie が英語で英語の訳が届かない時も、日本語を選び直せる', async ({ browser }) => {
  const { context, page } = await open(browser, 'ja-JP');
  await context.addCookies([{ name: 'callboard-lang', value: 'en', url: test.info().project.use.baseURL! }]);
  await page.route(/\/static\/locales\/en\.js/, (route) => route.fulfill({ status: 404, body: '' }));
  await page.goto('/');
  expect(await navTexts(page)).toEqual(JA_NAV);
  await page.locator('#lang-switch button[data-lang="ja"]').click();
  await expect.poll(async () => (await context.cookies()).find((c) => c.name === 'callboard-lang')?.value).toBe('ja');
  await context.close();
});

/**
 * 英語の時も日本語が残ってよい字（名指しの許し）。`text` は画面の字の 1 切れ（innerText を改行とタブで切った物）に当てる。
 * どれも、理由と、外す段（無ければ、ずっと残す物）を書く。
 */
const ALLOWED_JAPANESE: { text: RegExp; paths?: string[]; why: string; until: string }[] = [
  { text: /^日本語$/, why: '言語の切り替えの釦は、言語の名をその言語で書く', until: 'ずっと' },
];

/** `path` の画面で、許した字か。 */
const allowed = (path: string, text: string) =>
  ALLOWED_JAPANESE.some((a) => a.text.test(text) && (!a.paths || a.paths.includes(path)));

const JAPANESE = /[ぁ-んァ-ヶ一-龠]/;

test('英語のブラウザーでは、どの画面も字と属性と題を英語に差し替える（名指しで許した物のほか、日本語が残らない）', async ({ browser, request }) => {
  // 値を差し込む字（判定・返事の状態・メッセージの状態・渡した時）を出すように、字が英数字だけの記録を置く。待ちの数は
  // ほかの試験が数えるので、待ちにはしない（答えた・届いた）。
  const plan = await placePage(request, 'i18n-en', 'E-1', 'plan', '<!doctype html><title>plan</title><h1>plan</h1>', 'software-engineer');
  const review = await placePage(request, 'i18n-en', 'E-1', 'review', '<!doctype html><title>review</title><h1>review</h1>', 'staff-engineer');
  await reviewMarks(request, review, { verdict: 'concerns', targets: [plan] });
  await reply(request, plan, 'answered');
  await putMessage(request, 'i18n-en', 'm-1', {
    key: 'E-1',
    heading: 'hello',
    from: 'tpm',
    to: 'pm.1',
    text: 'text',
    sentAt: '2026-11-01T00:00:00Z',
    state: 'delivered',
    deliveredAt: '2026-11-01T00:01:00Z',
    via: 'wake',
    deliveries: 2,
  });

  const { context, page } = await open(browser, 'en-US');
  const problems = collectProblems(page);
  const found: Record<string, string[]> = {};
  for (const path of await screens(request)) {
    // メッセージとページの一覧は、知らせにつながると server の HTML を読み直して差し替える。読み直しが済んでから見る
    // （読み直した木も訳す。段 5）。
    if (path === '/messages' || path === '/pages/') await openLive(page, path);
    else await page.goto(path);
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
    const { attrs, texts } = await page.evaluate(() => {
      const japanese = /[ぁ-んァ-ヶ一-龠]/;
      const attrs: string[] = [];
      for (const element of document.querySelectorAll('[data-i18n-attr]')) {
        for (const pair of element.getAttribute('data-i18n-attr')!.split(';')) {
          const name = pair.split(':')[0].trim();
          if (japanese.test(element.getAttribute(name) ?? '')) attrs.push(`${pair}: ${element.getAttribute(name)}`);
        }
      }
      // 見えない字（隠した釦・畳んだ表・SVG の title）も見るので、textContent を要素ごとに切る。
      const texts: string[] = [];
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const parent = node.parentElement;
        if (parent && parent.closest('script, style, noscript')) continue;
        for (const piece of (node.textContent ?? '').split(/[\n\t]+/)) {
          if (japanese.test(piece)) texts.push(piece.trim());
        }
      }
      return { attrs, texts };
    });
    // 見える字の全部（innerText）も見る。
    const visible = (await page.locator('body').innerText()).split(/[\n\t]+/).map((t) => t.trim());
    const title = [await page.title()];
    const left = [...attrs, ...new Set([...texts, ...visible, ...title].filter((t) => JAPANESE.test(t) && !allowed(path, t)))];
    // 値の入っていない差し込み（{{名}}）も残らない。
    left.push(...visible.filter((t) => t.includes('{{')));
    if (left.length > 0) found[path] = left;
  }
  expect(found).toEqual({});
  expect(problems).toEqual([]);
  await context.close();
});

test('英語のブラウザーでは、file を上げられなかった訳も、誤りの鍵（code）から英語で出す', async ({ browser, request }) => {
  await placePage(request, 'i18n-up', 'U-1', 'plan', '<!doctype html><title>plan</title>');
  const { context, page } = await open(browser, 'en-US');
  await page.goto('/pages/?project=i18n-up&key=U-1');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  const box = page.locator('#upload');
  // 中身が拡張子と合わない file は、pagetray が 415（UnsupportedMediaType）で断る。callboard は型の名の鍵を足して返す。
  await box.locator('input[type=file]').setInputFiles({ name: 'fake.png', mimeType: 'image/png', buffer: Buffer.from('not a png') });
  await box.getByRole('checkbox').check();
  const answer = page.waitForResponse((r) => r.url().includes('/api/pagetray/projects/i18n-up/keys/U-1/files'));
  await box.locator('button[type=submit]').click();
  const json = await (await answer).json();
  expect(json.code).toBe('errors.kind.unsupportedMediaType');
  expect(json.values.detail).toBe(json.message);
  const say = box.locator('form > .say');
  // 呼ぶ先の訳（detail）は、呼ぶ先の言葉のまま添える（AI-48 で pagetray の訳が英語になるまで、日本語）。
  await expect(say).toHaveText(`Upload failed (415): This file type or content is not accepted (${json.message})`);
  await context.close();
});

test('英語のブラウザーでは、上げる道が 503 の時も、code の無い誤りの時も、訳を英語で組む', async ({ browser, request }) => {
  await placePage(request, 'i18n-up', 'U-2', 'plan', '<!doctype html><title>plan</title>');
  const { context, page } = await open(browser, 'en-US');
  await page.goto('/pages/?project=i18n-up&key=U-2');
  const box = page.locator('#upload');
  const say = box.locator('form > .say');
  const send = async (body: object, status: number) => {
    await page.route(/\/api\/pagetray\/projects\/i18n-up\/keys\/U-2\/files/, (route) =>
      route.request().method() === 'POST' ? route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) }) : route.fallback(),
    );
    await box.locator('input[type=file]').setInputFiles({ name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('text') });
    await box.getByRole('checkbox').check();
    await box.locator('button[type=submit]').click();
  };
  // 503（聞けなかった。置かれたかもしれない）: 訳の後に、確かめてから送り直す文も英語で付く。
  await send({ message: '1 秒で答えが無い', code: 'errors.kind.unavailable', values: { detail: 'timeout' } }, 503);
  await expect(say).toHaveText('Upload failed (503): The service is unavailable. Try again later (timeout). Check whether it appears in the file list above before sending again');
  await page.unrouteAll();
  // code の無い誤り（呼ぶ先の古い形）: 字は呼ぶ先の訳のまま、外の文は英語。
  await send({ error: 'bad' }, 400);
  await expect(say).toHaveText('Upload failed (400): bad');
  await context.close();
});

/** callboardT に鍵と値を直に渡した字。 */
const t = (page: Page, key: string, values?: unknown) =>
  page.evaluate(([k, v]) => (window as unknown as { callboardT: (k: string, o?: unknown) => string }).callboardT(k as string, v), [key, values] as const);

/** サーバーが配る形の値: 入れ子の字（$t）・入れ子の中の入れ子・名の並び（$join）・万の数・数で言い分ける字。 */
const NESTED = {
  retry: ['errors.unreachableRetry', { service: 'castlist', why: { $t: 'upstream.returned', values: { status: 404 } } }],
  deep: ['list.costNote.unreadable', { why: { $t: 'errors.unreachable', values: { service: 'pagetray', why: { $t: 'upstream.unreachable' } } } }],
  join: ['params.choice', { name: 'kind', choices: { $join: ['ai', 'human'], sep: 'common.listSep' } }],
  range: ['usage.byContext.bar', { range: { $t: 'common.tokenRange', values: { low: '0', high: { $t: 'common.tokens', values: { man: '2.5', k: '25' } } } }, cost: '$0.11', turns: '1', count: 1 }],
} as const;

test('入れ子の字・名の並び・万の数は、callboardT が選んだ言語で先に組む（i18next が無い時の仮の物も）', async ({ browser }) => {
  const want = {
    en: {
      retry: 'Cannot reach castlist (returned 404). Wait a moment and reload.',
      deep: 'Cannot read cost and longest context (Cannot reach pagetray (cannot connect)). The list is shown as is.',
      join: 'kind must be one of ai, human',
      range: '0–25k: $0.11 (1 turn)',
    },
    ja: {
      retry: 'castlist に聞けません（404 が返りました）。少し待ってから開き直してください。',
      deep: '額といちばん長い文脈を読めません（pagetray に聞けません（つながりません））。一覧は、そのまま出しています。',
      join: 'kind は ai・human のどれか',
      range: '0〜2.5 万: $0.11（1 回）',
    },
  };
  for (const [locale, lang, blockI18next] of [['en-US', 'en', false], ['ja-JP', 'ja', false], ['en-US', 'ja', true]] as const) {
    const { context, page } = await open(browser, locale);
    // i18next が届かない時は、仮の callboardT が日本語で組む。
    if (blockI18next) await page.route(/\/static\/vendor\/i18next-/, (route) => route.fulfill({ status: 404, body: '' }));
    await page.goto('/');
    for (const [name, [key, values]] of Object.entries(NESTED)) {
      expect(await t(page, key, values), `${locale} ${name}`).toBe(want[lang][name as keyof typeof NESTED]);
    }
    await context.close();
  }
});

test('英語のブラウザーでは、使用量の画面の万の数と入れ子の字を英語で組む', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  await page.goto('/usage/');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  // 段の幅（入れ子の万の数）。
  await expect(page.locator('#by-context p.legend')).toContainText('bands of 25k');
  await expect(page.locator('#by-context svg.chart')).toHaveAttribute('aria-label', /bands of 25k/);
  // 目盛りの万の数と、棒の title（段の範囲は入れ子の中の入れ子、回の数は数で言い分ける）。
  await expect(page.locator('#by-context svg text.tick', { hasText: /^200k$/ })).toHaveCount(1);
  const titles = await page.locator('#by-context svg rect.bar title').allTextContents();
  expect(titles.length).toBeGreaterThan(0);
  for (const title of titles) expect(title).toMatch(/^[\d,.]+k?–([\d,.]+k)?: \$[\d,.]+ \(\d+ turns?\)$/);
  await context.close();
});

test('英語のメッセージの一覧は、新着を受けて読み直した後も英語のままで、変わらない絞り込みの列は差し替えない', async ({ browser, request }) => {
  const message = (deliveredAt: string, deliveries: number) => ({
    key: 'L-1',
    heading: 'live',
    from: 'tpm',
    to: 'pm.1',
    text: 'text',
    sentAt: '2026-11-02T00:00:00Z',
    state: 'delivered' as const,
    deliveredAt,
    deliveries,
  });
  await putMessage(request, 'i18n-live', 'm-1', message('2026-11-02T00:01:00Z', 1));
  const { context, page } = await open(browser, 'en-US');
  const problems = collectProblems(page);
  // 変わらない絞り込みの列は、差し替えない（前は、訳した今の列と日本語の次の列を比べて、毎回差し替えていた）。知らせに
  // つながった時の初めの読み直しは、開いた時と同じ HTML を読む（何も変わらない）。つなぐのを止めておき、列の子（初めの
  // link。差し替えは #filters の中身を替えるので、#filters そのものの印は消えない）に印を付けてから、つながせる。
  let release = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route(/\/api\/pagetray\/events$/, async (route) => {
    await gate;
    await route.continue();
  });
  const pulled = page.waitForResponse((r) => new URL(r.url()).pathname === '/messages' && r.request().resourceType() === 'fetch');
  await page.goto('/messages?project=i18n-live');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  // 印は HTML に出ない JS の値で付ける（属性にすると、innerHTML の比べに入って「違う」になる）。
  await page.locator('#filters a').first().evaluate((el) => ((el as unknown as { kept: boolean }).kept = true));
  release();
  await (await pulled).finished();
  expect(await page.locator('#filters a').first().evaluate((el) => (el as unknown as { kept?: boolean }).kept === true)).toBe(true);
  // この project に置くと、読み直した一覧に行が入り、英語のまま。
  await putMessage(request, 'i18n-live', 'm-2', message('2026-11-02T00:05:00Z', 1));
  await expect(page.locator('#list li[data-id="i18n-live/m-2"]')).toBeVisible();
  for (const selector of ['#toolbar', '#filters', '#list']) {
    expect(await page.locator(selector).innerText(), selector).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
  }
  expect(await page.title()).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
  expect(problems).toEqual([]);
  await context.close();
});

test('英語のページの一覧は、新着を受けて読み直した後も、一覧・待ちの釦・題が英語のまま', async ({ browser, request }) => {
  const { context, page } = await open(browser, 'en-US');
  const problems = collectProblems(page);
  await openLive(page, '/pages/?project=i18n-live');
  const pulled = page.waitForResponse((r) => new URL(r.url()).pathname === '/pages/' && r.request().resourceType() === 'fetch');
  const path = await placePage(request, 'i18n-live', 'L-1', 'confirm', '<!doctype html><title>confirm</title>');
  await reply(request, path, 'waiting');
  try {
    await (await pulled).finished();
    await expect(page.locator('#list li[data-path]')).toHaveCount(1);
    // 待ちの数の釦（数は中の span。子ごと写す）も英語のまま。
    await expect(page.locator('#waiting')).toBeVisible();
    for (const selector of ['#toolbar', '#filters', '#list']) {
      expect(await page.locator(selector).innerText(), selector).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
    }
    expect(await page.title()).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
  } finally {
    // ほかの試験の待ちの数を替えないように、途中で落ちても答えたに戻す。
    await reply(request, path, 'answered');
  }
  expect(problems).toEqual([]);
  await context.close();
});

test('英語の相関図は、network.js が組む字（数・状態・記録なし）も英語', async ({ browser }) => {
  const { context, page } = await open(browser, 'en-US');
  await page.goto('/graph/network');
  await expect(page.locator('#network-status')).toContainText(/Actors: \d+, edges: \d+ \(as of /);
  expect(await page.locator('#network-controls').innerText()).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
  await context.close();
});
