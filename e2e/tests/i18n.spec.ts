// 画面の字の言語（AI-50）。サーバーは日本語の字を書き、英語の時はブラウザーの i18next（static/i18n.js）が差し替える。
// 言語は cookie（callboard-lang）→ ブラウザーの言語の並びの初めの 1 つ → 日本語。帯の切り替えで選べる。
import { expect, test, type Browser, type Page } from '@playwright/test';
import { collectProblems } from './helpers';

/** 頭の帯の画面の link の字（出た順）。 */
const navTexts = (page: Page) => page.locator('header.bar nav a').allTextContents();

const JA_NAV = ['アクター', 'タイムライン', '相関図', '使用量', 'ページ', 'メッセージ'];
const EN_NAV = ['Actors', 'Timeline', 'Network', 'Usage', 'Pages', 'Messages'];

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

test('サーバーが書いた日本語の字は、ブラウザーの i18next が引く日本語の字と同じ', async ({ page }) => {
  for (const path of ['/', '/actors/', '/graph/timeline', '/graph/network', '/usage/', '/messages', '/pages/']) {
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
