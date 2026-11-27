// 費え（/usage/）を、本物の castlist（2.1.0 から）を相手に、本物の browser で見る。数の行は seed.setup.ts が castlist の API
// （PutUsage）で置く: se.1 の本体（3 回・$1.50。段 1 に 1 回・段 9 に 2 回）と手伝い（2 回・$0.25）、other.1 の本体（1 回・$0.01）。
// 見本のアクターに役割のラベル（tpm.role）は無いので、役割は「（無し）」の 1 行。課題は tpm.key（t-23zs・t-99）。
import { expect, test, type Page } from '@playwright/test';
import { CASTLIST, collectProblems } from './helpers';

/** 表（役割ごと `roles`・課題ごと `subjects`）の、名の行の、数の欄。 */
const cells = (page: Page, section: string, name: string) =>
  page
    .locator(`#${section} tbody tr`)
    .filter({ has: page.locator('td:first-child', { hasText: name }) })
    .locator('td.num');

test('費えは、見本の数の行を、全体・役割ごと・課題ごとの表で出す（script は使わない）', async ({ page }) => {
  const problems = collectProblems(page);
  // 頭の帯の link から開く。期間が無いので、直近 7 日。
  await page.goto('/');
  await page.locator('header nav a', { hasText: '費え' }).click();
  await expect(page).toHaveURL(/\/usage\/$/);
  await expect(page).toHaveTitle('費え ・ callboard');
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
  expect(await page.locator('script').count()).toBe(0);

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

test('費えの図と、一覧の額の列と、プロフィールの費えの節を出す（script は使わない）', async ({ page, request }) => {
  const problems = collectProblems(page);
  // 費えの画面の 2 つの図。期間が無いので直近 7 日（日ごと）。
  await page.goto('/usage/');
  const byContext = page.locator('#by-context svg.chart');
  await expect(byContext).toHaveAttribute('role', 'img');
  await expect(byContext).toHaveAttribute('aria-label', /^文脈の長さの段ごとの費え（本体の回）。/);
  // 本体の段は 0（other.1）・1・9（se.1）の 3 つ。手伝いの段は混ぜない。
  await expect(byContext.locator('rect.bar.main')).toHaveCount(3);
  await expect(byContext.locator('line.limit')).toHaveCount(1);
  const byDay = page.locator('#by-day svg.chart');
  await expect(page.locator('#by-day h2')).toHaveText('日ごとの費え（日本時間）');
  // 行はみな今の時の物: 今日の棒に、本体（$1.51）と手伝い（$0.25）を積む。
  await expect(byDay.locator('rect.bar.main')).toHaveCount(1);
  await expect(byDay.locator('rect.bar.helper')).toHaveCount(1);
  await expect(byDay.locator('rect.bar.main title')).toContainText('本体 $1.51（本体と手伝いの計 $1.76）');
  expect(await page.locator('script').count()).toBe(0);

  // 一覧の行の、額といちばん長い文脈。数の無い席は「—」。
  await page.goto('/actors/?label=tpm.key:t-23zs');
  const row = (name: string) => page.locator('table.actors tbody tr').filter({ has: page.locator('a.name', { hasText: new RegExp(`^${name.replace('.', '\\.')}$`) }) });
  await expect(row('se.1').locator('td.num')).toHaveText(['$1.75', '23 万']);
  await expect(row('se.2').locator('td.num')).toHaveText(['—', '—']);

  // プロフィールの費えの節。要約の出来事（前と後の文脈の長さ付き）を、castlist の API で置いてから開く。
  const event = await request.put(`${CASTLIST}/api/events/e-se1-compact`, {
    data: { issuer: 'tpm', by: 'claude.session:s-se1', type: 'compacted', detail: { preTokens: 180_000, postTokens: 30_000 }, sentAt: new Date().toISOString() },
  });
  expect(event.ok(), await event.text()).toBeTruthy();
  await row('se.1').locator('a.name').click();
  await expect(page).toHaveTitle('se.1 ・ callboard');
  const usage = page.locator('#usage');
  await expect(usage.locator('h2')).toHaveText('費え（直近 31 日）');
  await expect(usage.locator('dd').first()).toHaveText('$1.75（本体 $1.50・手伝い $0.25）');
  const line = usage.locator('svg.chart');
  await expect(line).toHaveAttribute('aria-label', /^文脈の長さの段々の線/);
  await expect(line.locator('path.context')).toHaveCount(1);
  await expect(line.locator('line.compact')).toHaveCount(1);
  await expect(line.locator('line.compact title')).toContainText('（18 万 → 3 万）');
  expect(await page.locator('script').count()).toBe(0);
  expect(problems).toEqual([]);
});
