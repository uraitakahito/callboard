// 費え（/usage/）を、本物の castlist（2.1.0 から）を相手に、本物の browser で見る。数の行は seed.setup.ts が castlist の API
// （PutUsage）で置く: se.1 の本体（3 回・$1.50。段 1 に 1 回・段 9 に 2 回）と手伝い（2 回・$0.25）、other.1 の本体（1 回・$0.01）。
// 見本のアクターに役割のラベル（tpm.role）は無いので、役割は「（無し）」の 1 行。課題は tpm.key（t-23zs・t-99）。
import { expect, test, type Page } from '@playwright/test';
import { collectProblems } from './helpers';

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
  // 役割ごと: 席・回（本体）・額（本体）・比・中央・上から 1 割・いちばん長い文脈・20 万を越えた回・その額・手伝いの額。
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
