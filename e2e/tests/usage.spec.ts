// 使用量（/usage/）を、本物の castlist（2.1.0 から）を相手に、本物の browser で見る。数の行は seed.setup.ts が castlist の API
// （PutUsage）で置く: se.1 の本体（Opus。3 回・$1.50。段 1 に 1 回・段 9 に 2 回）と手伝い（Opus。Explore 1 回・$0.10 と
// general-purpose 1 回・$0.15）、other.1 の本体（Haiku。1 回・$0.01）。席の最新の effort は se.1 が high・other.1 が low。
// 見本のアクターに役割のラベル（tpm.role）は無いので、役割は「（無し）」の 1 行。課題は tpm.key（t-23zs・t-99）。
import { expect, test, type Page } from '@playwright/test';
import { CASTLIST, collectProblems } from './helpers';

/** 表（役割ごと `roles`・課題ごと `subjects`）の、名の行の、数の欄。 */
const cells = (page: Page, section: string, name: string) =>
  page
    .locator(`#${section} tbody tr`)
    .filter({ has: page.locator('td:first-child', { hasText: name }) })
    .locator('td.num');

test('使用量は、見本の数の行を、全体・役割ごと・課題ごとの表で出す（script は使わない）', async ({ page }) => {
  const problems = collectProblems(page);
  // 頭の帯の link から開く。期間が無いので、直近 7 日。
  await page.goto('/');
  await page.locator('header nav a', { hasText: '使用量' }).click();
  await expect(page).toHaveURL(/\/usage\/$/);
  await expect(page).toHaveTitle('使用量 ・ callboard');
  await expect(page.locator('#window')).toContainText('期間が無いので、直近 7 日');
  // 全体: 席・回・額・手伝いの回。
  await expect(page.locator('#total dd')).toHaveText(['2', '6', '$1.76', '2（額 $0.25。全体の額の 14.2%）']);
  // 役割ごと: 席・回（本体）・額（本体）・比・中央・上から 1 割・いちばん長い文脈・20 万以上の回・その額・手伝いの額。
  // 本体の段は 0 に 1 回・1 に 1 回・9 に 2 回（手伝いの段は混ぜない）。2 番目は段 1、4 番目は段 9。
  await expect(page.locator('#roles tbody tr')).toHaveCount(1);
  await expect(cells(page, 'roles', '（無し）')).toHaveText(['2', '4', '$1.51', '85.8%', '2.5 万〜', '22.5 万〜', '23 万', '50.0%', '66.2%', '$0.25']);
  // 課題ごと（額の大きい順）: 席・回（本体と手伝い）・額・いちばん長い文脈。
  await expect(page.locator('#subjects tbody tr td:first-child')).toHaveText(['t-23zs', 't-99']);
  await expect(cells(page, 'subjects', 't-23zs')).toHaveText(['1', '5', '$1.75', '23 万']);
  await expect(cells(page, 'subjects', 't-99')).toHaveText(['1', '1', '$0.01', '1 万']);
  await expect(page.locator('main')).toContainText('額は、使った量を API の値段に直した目安で、払う額ではありません。');
  expect(await page.locator('body script').count()).toBe(0);

  // ラベルで絞る（castlist が絞る）。form から送る。
  await page.locator('input[name="label"]').first().fill('tpm.key:t-99');
  await page.getByRole('button', { name: '絞り込む' }).click();
  await expect(page).toHaveURL(/label=tpm\.key(:|%3A)t-99/);
  await expect(page.locator('#total dd').first()).toHaveText('1');
  await expect(page.locator('#subjects tbody tr td:first-child')).toHaveText(['t-99']);

  // 期間の外には、記録が無い。31 日を越える期間は 400。
  await page.goto('/usage/?from=2000-01-01T00:00&to=2000-01-02T00:00');
  await expect(page.locator('.empty')).toHaveText('この期間に、数の記録はありません。');
  await expect(page.locator('#window')).toContainText('2000-01-01 00:00:00');
  const long = await page.request.get('/usage/?from=2026-01-01T00:00&to=2026-03-01T00:00');
  expect(long.status()).toBe(400);
  expect(await long.text()).toContain('期間は 31 日までです');
  expect(problems).toEqual([]);
});

test('使用量の図と、一覧の額の列と、プロフィールの使用量の節を出す（script は使わない）', async ({ page, request }) => {
  const problems = collectProblems(page);
  // 使用量の画面の 2 つの図。期間が無いので直近 7 日（日ごと）。
  await page.goto('/usage/');
  const byContext = page.locator('#by-context svg.chart');
  await expect(byContext).toHaveAttribute('role', 'img');
  await expect(byContext).toHaveAttribute('aria-label', /^文脈の長さの段ごとの額（本体の回）。/);
  // 本体の段は 0（other.1）・1・9（se.1）の 3 つ。手伝いの段は混ぜない。
  await expect(byContext.locator('rect.bar.main')).toHaveCount(3);
  await expect(byContext.locator('line.limit')).toHaveCount(1);
  const byDay = page.locator('#by-day svg.chart');
  await expect(page.locator('#by-day h2')).toHaveText('日ごとの額（日本時間）');
  // 行はみな今の時の物: 今日の棒に、本体（$1.51）と手伝い（$0.25）を積む。
  await expect(byDay.locator('rect.bar.main')).toHaveCount(1);
  await expect(byDay.locator('rect.bar.helper')).toHaveCount(1);
  await expect(byDay.locator('rect.bar.main title')).toContainText('本体 $1.51（本体と手伝いの計 $1.76）');
  expect(await page.locator('body script').count()).toBe(0);

  // 一覧の行の、額といちばん長い文脈。数の無い席は「—」。
  await page.goto('/actors/?label=tpm.key:t-23zs');
  const row = (name: string) => page.locator('table.actors tbody tr').filter({ has: page.locator('a.name', { hasText: new RegExp(`^${name.replace('.', '\\.')}$`) }) });
  await expect(row('se.1').locator('td.num')).toHaveText(['$1.75', '23 万']);
  await expect(row('se.2').locator('td.num')).toHaveText(['—', '—']);

  // プロフィールの使用量の節。要約の出来事（前と後の文脈の長さ付き）を、castlist の API で置いてから開く。
  const event = await request.put(`${CASTLIST}/api/events/e-se1-compact`, {
    data: { issuer: 'tpm', by: 'claude.session:s-se1', type: 'compacted', detail: { preTokens: 180_000, postTokens: 30_000 }, sentAt: new Date().toISOString() },
  });
  expect(event.ok(), await event.text()).toBeTruthy();
  await row('se.1').locator('a.name').click();
  await expect(page).toHaveTitle('se.1 ・ callboard');
  const usage = page.locator('#usage');
  await expect(usage.locator('h2')).toHaveText('使用量（直近 31 日）');
  await expect(usage.locator('dd').first()).toHaveText('$1.75（本体 $1.50・手伝い $0.25）');
  const line = usage.locator('svg.chart');
  await expect(line).toHaveAttribute('aria-label', /^文脈の長さの段々の線/);
  await expect(line.locator('path.context')).toHaveCount(1);
  await expect(line.locator('line.compact')).toHaveCount(1);
  await expect(line.locator('line.compact title')).toContainText('（18 万 → 3 万）');
  expect(await page.locator('body script').count()).toBe(0);
  expect(problems).toEqual([]);
});

// 絞りの候補（t-nd3w）: 役割と課題は選ぶ一覧、ほかのラベルは候補付きの欄。候補は、表示と同じ期間に稼働した席（castlist の
// ListActors）のラベルから作る。見本の席のラベルは project と tpm.key だけなので、役割と課題の一覧は「すべて」だけ。
test('ラベルの絞りは、役割と課題の一覧と、候補付きの欄で選べる（script は使わない）', async ({ page }) => {
  const problems = collectProblems(page);
  await page.goto('/usage/');
  const role = page.getByLabel('役割');
  const subject = page.getByLabel('課題');
  await expect(role.locator('option')).toHaveText(['すべて']);
  await expect(subject.locator('option')).toHaveText(['すべて']);
  // ほかのラベルの欄の候補: 席の多い順（pm.1・tpm.1・se.1・se.2・other.1 の 5 席が project:castlist、t-23zs は 3 席）。
  const field = page.getByLabel('ほかのラベル');
  await expect(field).toHaveAttribute('list', 'label-choices');
  const choices = page.locator('datalist#label-choices option');
  expect(await choices.evaluateAll((os) => os.map((o) => [(o as HTMLOptionElement).value, o.textContent]))).toEqual([
    ['project:castlist', 'project:castlist（5 席）'],
    ['tpm.key:t-23zs', 'tpm.key:t-23zs（3 席）'],
    ['tpm.key:t-99', 'tpm.key:t-99（1 席）'],
  ]);
  // 候補の値を欄に入れて送ると絞れる（castlist が絞る）。絞っても候補は減らない。
  await field.fill('tpm.key:t-99');
  await page.getByRole('button', { name: '絞り込む' }).click();
  await expect(page).toHaveURL(/label=tpm\.key(:|%3A)t-99/);
  await expect(page.locator('#subjects tbody tr td:first-child')).toHaveText(['t-99']);
  await expect(page.locator('datalist#label-choices option')).toHaveCount(3);
  await expect(page.getByLabel('ほかのラベル').first()).toHaveValue('tpm.key:t-99');

  // URL の役割は、一覧で選ばれている（候補に無い値も残す）。一覧の値を「すべて」に戻して送ると、その条件は外れる。
  await page.goto('/usage/?label=tpm.role:auditor&label=tpm.key:t-99');
  await expect(page.getByLabel('役割')).toHaveValue('tpm.role:auditor');
  await expect(page.locator('.empty')).toHaveText('この期間に、数の記録はありません。');
  await page.getByLabel('役割').selectOption('');
  await page.getByRole('button', { name: '絞り込む' }).click();
  await expect(page).not.toHaveURL(/tpm\.role/);
  await expect(page.locator('#subjects tbody tr td:first-child')).toHaveText(['t-99']);
  expect(await page.locator('body script').count()).toBe(0);
  expect(problems).toEqual([]);
});

test('英語の画面では、絞りの欄の名と「すべて」と席の数を英語で出す', async ({ browser }) => {
  const context = await browser.newContext({ locale: 'en-US', baseURL: test.info().project.use.baseURL });
  const page = await context.newPage();
  const problems = collectProblems(page);
  await page.goto('/usage/?label=tpm.role:auditor');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByLabel('Role')).toHaveValue('tpm.role:auditor');
  await expect(page.getByLabel('Role').locator('option')).toHaveText(['All', 'auditor']);
  await expect(page.getByLabel('Issue').locator('option')).toHaveText(['All']);
  await expect(page.getByLabel('Other label')).toHaveAttribute('list', 'label-choices');
  // 数は 1 と 2 以上で言い分ける。値（ラベル）は訳さない。
  await expect(page.locator('datalist#label-choices option')).toHaveText([
    'project:castlist (5 seats)',
    'tpm.key:t-23zs (3 seats)',
    'tpm.key:t-99 (1 seat)',
  ]);
  expect(problems).toEqual([]);
  await context.close();
});

/** model と役割ごとの行列（`#models`）の、`part`（thead・tbody・tfoot）の行ごとの欄の字。 */
const grid = (page: Page, part: string) =>
  page.locator(`#models ${part} tr`).evaluateAll((rows) =>
    rows.map((row) =>
      [...row.querySelectorAll('th, td')].map((cell) => {
        const copy = cell.cloneNode(true) as HTMLElement;
        copy.querySelectorAll('br').forEach((br) => br.replaceWith(' '));
        return (copy.textContent ?? '').replace(/\s+/g, ' ').trim();
      }),
    ),
  );

test('model と役割ごとの行列は、役割と手伝いの種類を行に、model を額の大きい順の列に出す（script は使わない）', async ({ page }) => {
  const problems = collectProblems(page);
  await page.goto('/usage/');
  await expect(page.locator('#models h2')).toHaveText('model と役割ごと');
  expect(await grid(page, 'thead')).toEqual([['役割・手伝い', 'claude-opus-5-5', 'claude-haiku-5-5', '計']]);
  expect(await grid(page, 'tbody')).toEqual([
    ['（無し） high（1 席）・low（1 席）', '$1.50 3 回', '$0.01 1 回', '$1.51'],
    ['手伝い: general-purpose', '$0.15 1 回', '—', '$0.15'],
    ['手伝い: Explore', '$0.10 1 回', '—', '$0.10'],
  ]);
  expect(await grid(page, 'tfoot')).toEqual([
    ['計', '$1.75', '$0.01', '$1.76'],
    ['全体の額に対する比', '99.4%', '0.6%', '100.0%'],
    ['1 回あたり', '$0.350', '$0.010', '$0.293'],
  ]);
  await expect(page.locator('#models tbody td.heat-3')).toHaveCount(1);
  await expect(page.locator('#models tbody td.heat-2')).toHaveCount(2);
  await expect(page.locator('#models tbody td.heat-1')).toHaveCount(1);
  await expect(page.locator('#models tbody td.heat-3')).toHaveAttribute('title', '1 回あたり $0.500');
  // 絞ると、行列も同じ行から作る（other.1 の本体だけ）。
  await page.goto('/usage/?label=tpm.key:t-99');
  expect(await grid(page, 'tbody')).toEqual([['（無し） low', '$0.01 1 回', '$0.01']]);
  expect(await page.locator('body script').count()).toBe(0);
  expect(problems).toEqual([]);
});

test('行列の地の色は、明るい色と暗い色の両方で、site.css の変数の色', async ({ page }) => {
  const colours = () =>
    page.evaluate(() => {
      const css = (name: string) => {
        const probe = document.createElement('div');
        probe.style.color = `var(${name})`;
        document.body.append(probe);
        const rgb = getComputedStyle(probe).color;
        probe.remove();
        return rgb.replace(/\s/g, '');
      };
      const cell = (n: number) => getComputedStyle(document.querySelector(`#models tbody td.heat-${n}`)!).backgroundColor.replace(/\s/g, '');
      return { got: [cell(1), cell(2), cell(3)], want: [css('--heat-1'), css('--heat-2'), css('--heat-3')] };
    });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/usage/');
  const light = await colours();
  expect(light.got).toEqual(light.want);
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect.poll(async () => (await colours()).got).not.toEqual(light.got);
  const dark = await colours();
  expect(dark.got).toEqual(dark.want);
});

test('英語の画面では、行列の見出しと行の名と下の行と effort の席の数を英語で出す', async ({ browser }) => {
  const context = await browser.newContext({ locale: 'en-US', baseURL: test.info().project.use.baseURL });
  const page = await context.newPage();
  const problems = collectProblems(page);
  await page.goto('/usage/');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.locator('#models h2')).toHaveText('By model and role');
  await expect.poll(() => grid(page, 'thead')).toEqual([['Role / helper', 'claude-opus-5-5', 'claude-haiku-5-5', 'Total']]);
  await expect.poll(() => grid(page, 'tbody')).toEqual([
    ['(none) high (1 seat), low (1 seat)', '$1.50 3 turns', '$0.01 1 turn', '$1.51'],
    ['Helper: general-purpose', '$0.15 1 turn', '—', '$0.15'],
    ['Helper: Explore', '$0.10 1 turn', '—', '$0.10'],
  ]);
  await expect.poll(() => grid(page, 'tfoot')).toEqual([
    ['Total', '$1.75', '$0.01', '$1.76'],
    ['Share of total cost', '99.4%', '0.6%', '100.0%'],
    ['Per turn', '$0.350', '$0.010', '$0.293'],
  ]);
  await expect(page.locator('#models tbody td.heat-3')).toHaveAttribute('title', '$0.500 per turn');
  expect(problems).toEqual([]);
  await context.close();
});
