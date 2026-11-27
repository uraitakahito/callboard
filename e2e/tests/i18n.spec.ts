// 画面の字の言語（AI-50）。サーバーは日本語の字を書き、英語の時はブラウザーの i18next（static/i18n.js）が差し替える。
// 言語は cookie（callboard-lang）→ ブラウザーの言語の並びの初めの 1 つ → 日本語。帯の切り替えで選べる。
import { expect, test, type APIRequestContext, type Browser, type Page } from '@playwright/test';
import { CASTLIST, collectProblems, placePage, putMessage, reply, reviewMarks } from './helpers';

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
  { text: /^記録なし$/, paths: ['/graph/network'], why: '相関図の点の model の字は network.js が組む', until: '段 5' },
  { text: /^\d+ のアクター・\d+ の線（.+ の時点）$/, paths: ['/graph/network'], why: '相関図の数の字は network.js が組む', until: '段 5' },
];

/** `path` の画面で、許した字か。 */
const allowed = (path: string, text: string) =>
  ALLOWED_JAPANESE.some((a) => a.text.test(text) && (!a.paths || a.paths.includes(path)));

/** 題に残ってよい日本語（メッセージとページの一覧は、新着を読み直した時に server の題を写す。段 5 で直す）。 */
const TITLE_UNTIL_STAGE5 = ['/messages', '/pages/'];

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
  // 新着の知らせの道を止める: メッセージとページの一覧の script は、知らせがつながると server の HTML を読み直して差し替えるが、
  // 読み直した木をまだ訳さない（段 5 で直す）。ここでは、サーバーが書いた字（段 4）が英語になるかを見る。段 5 でこの止めを外す。
  await page.route(/\/api\/pagetray\/events$/, (route) => route.fulfill({ status: 204, body: '' }));
  const found: Record<string, string[]> = {};
  for (const path of await screens(request)) {
    await page.goto(path);
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
    const title = TITLE_UNTIL_STAGE5.includes(path) ? [] : [await page.title()];
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
  await expect(say).toHaveText('Upload failed (415): This file type or content is not accepted');
  await context.close();
});
