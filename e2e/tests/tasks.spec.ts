// 課題の一覧（/messages/tasks）を、本物の pagetray を相手に、本物の browser で見る。HTML の字は crates/server/tests/tasks.rs（Rust）が
// 見るので、ここは人が押してたどる道（メッセージの一覧 → 課題ごと → 課題の筋）・返事待ちの絞り・英語・新着の印を見る。
// 一覧は置き場の全部（新しい 1,000 通）を読むので、ほかの試験が置いた記録の課題も並ぶ。行は、この試験の課題の Key で名指す。
import { expect, test } from '@playwright/test';
import { collectProblems, putMessage, type Message } from './helpers';

const PROJECT = 'tasks-1';
const message = (heading: string, from: string, to: string, at: string, text: string, change: Partial<Message> = {}): Message => ({
  // 記録の Key は、担当の Key（TPM 担当が送った物は、受け手の Key）。
  key: from === 'tpm' ? to : from,
  heading,
  from,
  to,
  text: `【${heading}】${text}`,
  // ほかの試験の記録より新しくして、一覧の頭に出す。
  sentAt: `2026-12-01T${at}Z`,
  state: 'delivered',
  ...change,
});

/** 返事待ちの課題 TL-2（PM に確認）と、済んだ課題 TL-1 を置く。 */
async function place(request: Parameters<typeof putMessage>[0]): Promise<void> {
  await putMessage(request, PROJECT, 'b1', message('brief', 'tpm', 'TL-1', '03:00:00', '依頼書'));
  await putMessage(request, PROJECT, 'd1', message('done', 'TL-1', 'tpm', '03:30:00', '要点: 入れた', { replyTo: 'b1' }));
  await putMessage(request, PROJECT, 'x1', message('dismiss', 'tpm', 'TL-1', '03:40:00', '続きは無い'));
  await putMessage(request, PROJECT, 'e1', message('end', 'TL-1', 'tpm', '04:00:00', '残り: なし', { replyTo: 'x1' }));
  await putMessage(request, PROJECT, 'q2', message('ask-pm', 'TL-2', 'tpm', '05:00:00', '項目: (1) 案'));
}

const row = (page: import('@playwright/test').Page, key: string) => page.locator(`#list li[data-key="${key}"]`);

test('メッセージの一覧の「課題ごと」から課題の一覧へ。行から課題の筋へ。「返事待ちだけ」で絞る。console の誤りも CSP の違反も無い', async ({ page, request }) => {
  const problems = collectProblems(page);
  await place(request);
  await page.goto('/messages');
  await page.locator('#toolbar').getByRole('link', { name: '課題ごと', exact: true }).click();
  await expect(page).toHaveURL(/\/messages\/tasks$/);
  await expect(page).toHaveTitle('課題ごと ・ メッセージの記録 ・ callboard');
  await expect(row(page, 'TL-2').locator('.status')).toHaveText('PM の返事待ち');
  await expect(row(page, 'TL-1').locator('.status')).toHaveText('済み');
  // 新しい課題が上。
  const keys = await page.locator('#list li[data-key]').evaluateAll((els) => els.map((el) => (el as HTMLElement).dataset.key));
  expect(keys.indexOf('TL-2')).toBeLessThan(keys.indexOf('TL-1'));
  // 返事待ちだけに絞ると、済んだ課題は出ない。
  await page.locator('#waiting').click();
  await expect(page).toHaveURL(/\/messages\/tasks\?waiting=1$/);
  await expect(row(page, 'TL-2')).toBeVisible();
  await expect(row(page, 'TL-1')).toHaveCount(0);
  // 行の Key を押すと、課題の筋。そこから「課題ごと」で戻れる。
  await row(page, 'TL-2').locator('a.key').click();
  await expect(page).toHaveURL(/\/messages\/tasks\/TL-2$/);
  await expect(page.locator('.thread-key .status')).toHaveText('PM の返事待ち');
  await page.locator('#toolbar').getByRole('link', { name: '課題ごと', exact: true }).click();
  await expect(page).toHaveURL(/\/messages\/tasks$/);
  expect(problems).toEqual([]);
});

test('英語にすると、課題の様子と数が英語になる。新しい記録が置かれると、読み直す印が出る', async ({ page, request }) => {
  const problems = collectProblems(page);
  await place(request);
  const live = page.waitForResponse((r) => r.url().endsWith('/api/pagetray/events') && r.status() === 200);
  await page.goto('/messages/tasks');
  await live;
  await page.locator('#lang-switch button[data-lang="en"]').click();
  await expect(page).toHaveTitle('By task ・ Message log ・ callboard');
  await expect(row(page, 'TL-2').locator('.status')).toHaveText('Waiting for PM');
  await expect(row(page, 'TL-1').locator('.status')).toHaveText('Done');
  await expect(page.locator('#fresh')).toBeHidden();
  // どの課題の記録でも、一覧は古くなるので印を出す。
  await putMessage(request, PROJECT, 'n3', message('notify', 'tpm', 'TL-3', '06:00:00', '新しい課題'));
  await expect(page.locator('#fresh')).toBeVisible();
  await page.locator('#fresh').click();
  await expect(row(page, 'TL-3')).toBeVisible();
  expect(problems).toEqual([]);
  // 言語の選びは cookie に残るので、日本語に戻しておく（ほかの試験は、日本語の画面を見る）。
  await page.locator('#lang-switch button[data-lang="ja"]').click();
});
