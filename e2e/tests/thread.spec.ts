// 課題の筋（/messages/tasks/<Key>）を、本物の pagetray を相手に、本物の browser で見る。
// HTML の字と棒の位置は crates/server/tests/thread.rs（Rust）が見るので、ここは browser でしか見えない物（棒が CSS の色で描かれ、
// 幅を持つこと・CSP の違反が無いこと・行が開くこと・英語への差し替え・新着の印）と、人が押してたどる道を見る。
import { expect, test } from '@playwright/test';
import { collectProblems, messageRow, putMessage, type Message } from './helpers';

const PROJECT = 'thr-1';
const message = (heading: string, from: string, to: string, at: string, text: string, change: Partial<Message> = {}): Message => ({
  // 記録の Key は、担当の Key（TPM 担当が送った物は、受け手の Key）。
  key: from === 'tpm' ? to : from,
  heading,
  from,
  to,
  text: `【${heading}】${text}`,
  sentAt: `2026-11-01T${at}Z`,
  state: 'delivered',
  deliveredAt: `2026-11-01T${at}Z`,
  ...change,
});

/** 済んだ課題 TH-1（依頼書 → レビュー → merged → done → 片付け）を置く。 */
async function place(request: Parameters<typeof putMessage>[0]): Promise<void> {
  await putMessage(request, PROJECT, 'b1', message('brief', 'tpm', 'TH-1', '03:00:00', 'http://127.0.0.1:8801/thr-1/TH-1/brief-1.html'));
  await putMessage(request, PROJECT, 'r1', message('ask-review', 'TH-1', 'TH-1.staff', '03:10:00', '頼み: #1 https://github.com/o/roster/pull/7'));
  await putMessage(request, PROJECT, 'v1', message('review', 'TH-1.staff', 'TH-1', '03:15:00', '判定: よい（approve）', { key: 'TH-1.staff', replyTo: 'r1' }));
  await putMessage(request, PROJECT, 'm1', message('merged', 'TH-1', 'tpm', '03:20:00', '<img src=x onerror="document.title=1"> 入れた'));
  await putMessage(request, PROJECT, 'd1', message('done', 'TH-1', 'tpm', '03:30:00', '要点: 入れた', { replyTo: 'b1' }));
  await putMessage(request, PROJECT, 'x1', message('dismiss', 'tpm', 'TH-1', '03:40:00', 'この課題に続きは無い。'));
  await putMessage(request, PROJECT, 'e1', message('end', 'TH-1', 'tpm', '04:00:00', '残っている物: なし', { replyTo: 'x1' }));
}

const rows = (page: import('@playwright/test').Page) => page.locator('#list details.xrow');
const row = (page: import('@playwright/test').Page, id: string) => page.locator(`#list details.xrow[data-id="${id}"]`);

test('メッセージの一覧の Key から課題の筋へ。往復は色の付いた棒、片道は印。行を押すと頼みと返事の全文。console の誤りも CSP の違反も無い', async ({ page, request }) => {
  const problems = collectProblems(page);
  await place(request);
  await page.goto(`/messages?project=${PROJECT}`);
  await messageRow(page, PROJECT, 'b1').locator('a.task').click();
  await expect(page).toHaveURL(new RegExp(`/messages/tasks/TH-1\\?project=${PROJECT}$`));
  await expect(page).toHaveTitle('TH-1 の筋 ・ メッセージの記録 ・ callboard');
  await expect(page.locator('.thread-key .status')).toHaveText('済み');
  // 返事（review・done・end）は、頼みの行に入る。
  await expect(rows(page)).toHaveCount(4);
  await expect(rows(page).locator('.kind')).toHaveText(['依頼書', 'レビュー', 'merged', '片付け']);
  // 棒は、幅を持ち、球の持ち手の色（CSS の class）で塗られる。依頼書の往復（30 分）は、レビューの往復（5 分）より長い。
  const bar = (id: string) => row(page, id).locator('svg.trk rect');
  const [brief, review] = [await bar('b1').boundingBox(), await bar('r1').boundingBox()];
  expect(brief!.width).toBeGreaterThan(review!.width * 4);
  expect(review!.x).toBeGreaterThan(brief!.x);
  const fill = (id: string) => bar(id).evaluate((el) => getComputedStyle(el).fill);
  expect(await fill('b1')).not.toBe(await fill('r1'));
  await expect(bar('m1')).toHaveClass('mark ok');
  // 行を押すと、頼みと返事の全文が開く。
  await expect(row(page, 'b1').locator('.parts')).toBeHidden();
  await row(page, 'b1').locator('summary .kind').click();
  await expect(row(page, 'b1').locator('.parts pre')).toHaveText(['【brief】http://127.0.0.1:8801/thr-1/TH-1/brief-1.html', '【done】要点: 入れた']);
  // 本文は HTML として動かない。
  await row(page, 'm1').locator('summary .kind').click();
  await expect(row(page, 'm1').locator('.parts pre')).toContainText('<img src=x onerror=');
  await expect(page).toHaveTitle('TH-1 の筋 ・ メッセージの記録 ・ callboard');
  // 本文の中の PR は、短い名の link。
  await expect(row(page, 'r1').locator('a.chip')).toHaveText(['roster#7']);
  await expect(row(page, 'r1').locator('a.chip')).toHaveAttribute('href', 'https://github.com/o/roster/pull/7');
  expect(problems).toEqual([]);
});

test('依頼書の往復の「時間の軸を、この往復に絞る」で絞り、「課題の全体に戻す」で戻る', async ({ page, request }) => {
  await place(request);
  await page.goto(`/messages/tasks/TH-1?project=${PROJECT}`);
  await row(page, 'b1').locator('summary .kind').click();
  await row(page, 'b1').getByRole('link', { name: '時間の軸を、この往復に絞る' }).click();
  await expect(page).toHaveURL(/[?&]at=b1$/);
  // 依頼書の往復（03:00〜03:30）に重なる行だけ。
  await expect(rows(page).locator('.kind')).toHaveText(['依頼書', 'レビュー', 'merged']);
  await page.locator('#toolbar').getByRole('link', { name: '課題の全体に戻す' }).click();
  await expect(rows(page)).toHaveCount(4);
});

test('英語にすると、種類・持ち手・長さ・課題の様子が英語になる。新しい記録が置かれると、読み直す印が出る', async ({ page, request }) => {
  const problems = collectProblems(page);
  await place(request);
  const live = page.waitForResponse((r) => r.url().endsWith('/api/pagetray/events') && r.status() === 200);
  await page.goto(`/messages/tasks/TH-1?project=${PROJECT}`);
  await live;
  await page.locator('#lang-switch button[data-lang="en"]').click();
  await expect(page).toHaveTitle('Thread of TH-1 ・ Message log ・ callboard');
  await expect(page.locator('.thread-key .status')).toHaveText('Done');
  await expect(rows(page).locator('.kind')).toHaveText(['Brief', 'Review', 'merged', 'Wrap-up']);
  await expect(row(page, 'b1').locator('.took')).toHaveText('Assignee 30 min');
  await expect(page.locator('#fresh')).toBeHidden();
  // ほかの課題の記録（点の前の字が違う Key。TH-1 で始まる別の課題も）では、印を出さない。
  await putMessage(request, PROJECT, 'o1', message('notify', 'tpm', 'TH-10', '04:05:00', 'ほかの課題の知らせ'));
  await putMessage(request, PROJECT, 'o2', message('notify', 'tpm', 'OTHER-1', '04:06:00', 'ほかの課題の知らせ'));
  // 知らせが browser に届くのを待つ（同じ流れで、後から置いたこの課題の席の記録で印が出るのを待てば、前の 2 つは届いている）。
  await putMessage(request, PROJECT, 'n0', message('review', 'TH-1.staff', 'TH-1', '04:07:00', '席の記録', { key: 'TH-1.staff' }));
  await expect(page.locator('#fresh')).toBeVisible();
  await page.reload();
  await page.waitForResponse((r) => r.url().endsWith('/api/pagetray/events') && r.status() === 200);
  await expect(page.locator('#fresh')).toBeHidden();
  await putMessage(request, PROJECT, 'o3', message('notify', 'tpm', 'TH-10', '04:08:00', 'ほかの課題の知らせ'));
  await putMessage(request, PROJECT, 'n1', message('notify', 'tpm', 'TH-1', '04:10:00', '後からの知らせ'));
  await expect(page.locator('#fresh')).toBeVisible();
  await expect(page.locator('#fresh')).toHaveText('● New records (reload)');
  await page.locator('#fresh').click();
  await expect(rows(page)).toHaveCount(6);
  expect(problems).toEqual([]);
  // 言語の選びは cookie に残るので、日本語に戻しておく（ほかの試験は、日本語の画面を見る）。
  await page.locator('#lang-switch button[data-lang="ja"]').click();
});
