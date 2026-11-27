// ページの一覧（/pages/）を、本物の pagetray を相手に、本物の browser で見る。pagetray の e2e の list・reply・review・files の
// 試験から、browser でしか見えない物（script の動き・本文の枠・file を上げる枠）と、人が押してたどる道を移した物。
// HTML の字は crates/server/tests/pages.rs（Rust）が見る。
// 待ちの数（返事待ち・レビュー待ち・上げた）は置き場の全部を数えるので、待ちにしたページは、試験の終わりまでに答えたにする。
import { expect, test } from '@playwright/test';
import { PAGETRAY, PNG, choice, collectProblems, fileRows, fresh, openLive, pageRow, pageRows, placeFile, placePage, reply, reviewMarks, uploadBox, viewer, waitingCount } from './helpers';

const html = (title: string, body = '<p>本文</p>') => `<!doctype html><title>${title}</title><h1>${title}</h1>${body}`;

test('頭の帯の「ページ」は callboard の一覧を開き、メッセージの一覧と行き来できる', async ({ page, request }) => {
  await placePage(request, 'pg-link', 'K-1', 'plan', html('一枚目'));
  await page.goto('/');
  await page.locator('header.bar').getByRole('link', { name: 'ページ', exact: true }).click();
  await expect(page).toHaveURL(/\/pages\/$/);
  await expect(page).toHaveTitle(/ページの置き場 ・ callboard$/);
  await expect(pageRow(page, '/pg-link/K-1/plan-1.html')).toBeVisible();
  await page.locator('#toolbar').getByRole('link', { name: 'メッセージ', exact: true }).click();
  await expect(page).toHaveURL(/\/messages$/);
  await page.locator('#toolbar').getByRole('link', { name: 'ページ', exact: true }).click();
  await expect(page).toHaveURL(/\/pages\/$/);
});

test('行を押すと本文の枠に 1 枚だけ出る。絞り込み・検索・並びが動く。console の誤りも CSP の違反も無い', async ({ page, request }) => {
  const problems = collectProblems(page);
  const plan = await placePage(request, 'pg-rows', 'A-1', 'plan', html('計画の題', '<p id="mark">みかんの本文</p>'), 'software-engineer', 'workspace:1');
  const result = await placePage(request, 'pg-rows', 'A-1', 'result', html('結果の題', '<p>りんご</p>'), 'software-engineer', 'workspace:1');
  const brief = await placePage(request, 'pg-rows', 'B-2', 'brief', html('依頼書の題'), 'tpm');

  await page.goto('/pages/?project=pg-rows');
  await expect(pageRows(page)).toHaveCount(3);
  await expect(pageRows(page).locator('.name')).toHaveText(['brief-1', 'result-1', 'plan-1']);
  const row = pageRow(page, plan);
  await expect(row.locator('.title')).toHaveText('計画の題');
  await expect(row.locator('.who')).toHaveText('software-engineer ・ workspace:1');
  await expect(viewer(page)).toContainText('ページを選ぶと、ここに 1 枚だけ出る');

  // 行を押すと、本文の枠に pagetray のページが出る（URL に open が付く。枠の中身は pagetray から直に読む）。
  await row.locator(':scope > a').click();
  await expect(page).toHaveURL(/open=%2Fpg-rows%2FA-1%2Fplan-1\.html/);
  await expect(viewer(page).locator('iframe')).toHaveAttribute('src', `${PAGETRAY}${plan}`);
  await expect(page.frameLocator('#viewer iframe').locator('#mark')).toHaveText('みかんの本文');
  await expect(viewer(page).getByRole('link', { name: '別の tab で開く' })).toHaveAttribute('href', `${PAGETRAY}${plan}`);
  await expect(pageRow(page, plan).locator(':scope > a')).toHaveAttribute('aria-current', 'true');

  // 絞り込み: 候補を押すと合う行だけになり、もう一度押すと外れる。本文の枠は、そのまま。
  await choice(page, 'Key', 'B-2').click();
  await expect(pageRows(page)).toHaveCount(1);
  await expect(pageRow(page, brief)).toBeVisible();
  await expect(viewer(page).locator('iframe')).toHaveAttribute('src', `${PAGETRAY}${plan}`);
  await choice(page, 'Key', 'B-2').click();
  await expect(pageRows(page)).toHaveCount(3);
  // 検索: 題と本文。
  await page.getByRole('searchbox').fill('りんご');
  await page.getByRole('button', { name: '探す' }).click();
  await expect(page).toHaveURL(/[?&]q=/);
  await expect(pageRows(page)).toHaveCount(1);
  await expect(pageRow(page, result)).toBeVisible();
  await page.locator('#toolbar').getByRole('link', { name: '絞り込みを外す' }).click();
  // 並び。
  await page.goto('/pages/?project=pg-rows');
  await page.locator('#toolbar').getByRole('link', { name: '並び: 新しい順' }).click();
  await expect(page).toHaveURL(/sort=created_at/);
  await expect(pageRows(page).locator('.name')).toHaveText(['plan-1', 'result-1', 'brief-1']);
  expect(problems).toEqual([]);
});

test('新着の知らせ: 置かれると押さずに頭に入り、tab の題に数が付き、未読の印が付く。開くと未読が消える', async ({ page, request }) => {
  const problems = collectProblems(page);
  const first = await placePage(request, 'pg-live', 'K-1', 'plan', html('一枚目'));
  await openLive(page, '/pages/?project=pg-live');
  await expect(pageRows(page)).toHaveCount(1);
  // 開いた時より前に置いたページに、未読の印は無い。
  await expect(pageRow(page, first)).not.toHaveClass(/unread/);
  await page.evaluate(() => ((window as any).__kept = true));

  // ほかの project のページは、一覧の列に入らず、新着にも数えない。
  await placePage(request, 'other-pg-live', 'K-1', 'plan', html('ほか'));
  const second = await placePage(request, 'pg-live', 'K-1', 'plan', html('二枚目'));
  await expect(pageRows(page)).toHaveCount(2);
  await expect(pageRows(page).first().locator('.title')).toHaveText('二枚目');
  await expect(page).toHaveTitle('(1) ページの置き場 ・ callboard');
  await expect(pageRow(page, second)).toHaveClass(/unread/);
  // 絞り込みの列の数も新しくなる。
  await expect(choice(page, 'project', 'pg-live').locator('.n')).toHaveText('2');
  expect(await page.evaluate(() => (window as any).__kept)).toBe(true);

  // 開くと（本文の枠に出すと）、読んだ物になる。
  await pageRow(page, second).locator(':scope > a').click();
  await expect(viewer(page).locator('iframe')).toHaveAttribute('src', `${PAGETRAY}${second}`);
  await expect(pageRow(page, second)).not.toHaveClass(/unread/);
  expect(problems).toEqual([]);
});

test('返事の状態とレビューの印: 書かれると、押さずに行の印と待ちの数が替わる', async ({ page, request }) => {
  const confirm = await placePage(request, 'pg-marks', 'K-1', 'confirm', html('確認'));
  const plan = await placePage(request, 'pg-marks', 'K-1', 'plan', html('計画'));
  const review = await placePage(request, 'pg-marks', 'K-1', 'review', html('レビュー'), 'staff-engineer');
  await openLive(page, '/pages/?project=pg-marks');
  await expect(pageRows(page)).toHaveCount(3);
  await expect(waitingCount(page)).toBeHidden();

  await reply(request, confirm, 'waiting');
  await expect(pageRow(page, confirm).locator('.reply')).toHaveText('返事待ち');
  await expect(waitingCount(page)).toHaveText('返事待ち 1 件');
  await expect(page).toHaveTitle('返事待ち 1 件 ・ ページの置き場 ・ callboard');
  await expect(choice(page, '返事', '返事待ち').locator('.n')).toHaveText('1');

  // 待ちの数の釦を押すと、返事待ちだけに絞る。
  await waitingCount(page).click();
  await expect(page).toHaveURL(/\/pages\/\?reply=waiting$/);
  await expect(pageRow(page, confirm)).toBeVisible();
  await page.goBack();
  await openLive(page, '/pages/?project=pg-marks');

  await reply(request, confirm, 'answered');
  await expect(pageRow(page, confirm).locator('.reply')).toHaveText('答えた');
  await expect(waitingCount(page)).toBeHidden();
  await expect(page).toHaveTitle('ページの置き場 ・ callboard');

  // レビューの印: 判定と見た物。見られたページの行に、レビューの link と判定が出る。
  await reviewMarks(request, review, { verdict: 'concerns', targets: [plan, 'https://github.com/o/r/pull/7'] });
  await expect(pageRow(page, review).locator('.top .verdict')).toHaveText('気になる所あり');
  await expect(pageRow(page, review).locator('.links.targets')).toHaveText('見た物: plan-1 r#7');
  await expect(pageRow(page, plan).locator('.links.reviews')).toContainText('レビュー: review-1');
  await expect(pageRow(page, plan).locator('.links.reviews .verdict')).toHaveText('気になる所あり');
  // 見た物の link を押すと、本文の枠に、そのページが出る。
  await pageRow(page, review).locator('.links.targets a').first().click();
  await expect(viewer(page).locator('iframe')).toHaveAttribute('src', `${PAGETRAY}${plan}`);
});

test('Key の画面: file の枠に file が出て、form から上げると、役割 product-manager で置かれて枠に入る', async ({ page, request }) => {
  const problems = collectProblems(page);
  await placePage(request, 'pg-files', 'K-1', 'plan', html('計画'));
  await placeFile(request, 'pg-files', 'K-1', 'before.png', 'image/png', PNG);
  // project だけでは、file の枠は出ない。
  await page.goto('/pages/?project=pg-files');
  await expect(uploadBox(page)).toHaveCount(0);

  await openLive(page, '/pages/?project=pg-files&key=K-1');
  await expect(fileRows(page)).toHaveCount(1);
  await expect(fileRows(page).first().locator('.name')).toHaveText('before-1.png');
  await expect(fileRows(page).first().locator('.ref')).toHaveText('files/before-1.png');
  // file の link と図の URL は、pagetray の link の頭で組む。
  const shot = `${PAGETRAY}/pg-files/K-1/files/before-1.png`;
  await expect(fileRows(page).first().locator('a.thumb')).toHaveAttribute('href', shot);
  await expect(fileRows(page).first().locator('a.name')).toHaveAttribute('href', shot);
  await expect(fileRows(page).first().locator('img')).toHaveAttribute('src', shot);
  // 図は、pagetray から直に読めている（CSP の img-src）。
  await expect.poll(() => fileRows(page).first().locator('img').evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);

  // 受けない拡張子は、送る前に断る。
  const box = uploadBox(page);
  await box.locator('input[type=file]').setInputFiles({ name: 'evil.exe', mimeType: 'application/octet-stream', buffer: Buffer.from('x') });
  await expect(box.locator('form > .say')).toContainText('.exe は上げられない');
  await expect(box.getByRole('button', { name: '送る' })).toBeDisabled();

  // 図を選ぶと見本が出て、秘密を見た印を付けるまで送れない。
  await box.locator('input[type=file]').setInputFiles({ name: 'My Shot 1.PNG', mimeType: 'image/png', buffer: PNG });
  await expect(box.locator('.preview')).toContainText('→ my-shot-1.png');
  await expect(box.locator('.preview img')).toBeVisible();
  await expect(box.getByRole('button', { name: '送る' })).toBeDisabled();
  await box.getByRole('checkbox').check();
  await box.getByRole('button', { name: '送る' }).click();
  await expect(box.locator('form > .say')).toContainText('上げた。ページからは files/my-shot-1-1.png で参照する');
  await expect(fileRows(page)).toHaveCount(2);
  await expect(fileRows(page).first().locator('.name')).toHaveText('my-shot-1-1.png');
  await expect(fileRows(page).first().locator('.who')).toContainText('product-manager');
  // 置き場の記録: 役割は product-manager、workspace は空、送り手は無い。
  const files = await (await request.get(`${PAGETRAY}/api/files?project=pg-files&key=K-1`)).json();
  const placed = files.files.find((f: { name: string }) => f.name === 'my-shot-1');
  expect(placed).toMatchObject({ role: 'product-manager', workspace: '', ext: 'png', n: '1' });
  expect(placed.sender).toBeUndefined();

  // 中身が拡張子と合わない file は、pagetray が断り、その訳を出す。
  await box.locator('input[type=file]').setInputFiles({ name: 'fake.png', mimeType: 'image/png', buffer: Buffer.from('not a png') });
  await box.getByRole('checkbox').check();
  await box.getByRole('button', { name: '送る' }).click();
  await expect(box.locator('form > .say')).toContainText('上げられなかった（415）');
  await expect(fileRows(page)).toHaveCount(2);

  // ほかの所（API）から、この Key に file が置かれると、押さずに枠に入る。
  await placeFile(request, 'pg-files', 'K-1', 'notes.txt', 'text/plain', 'hello');
  await expect(fileRows(page)).toHaveCount(3);
  // 415 は、browser が console に出す（network の誤り）。もう 1 組は、中身が図でない file（上の fake.png）を図の見本
  // （blob: の URL の <img>）に出した時に、Chrome が出す connect-src の違反（見本が出ないだけ。pagetray の画面の script と同じ
  // 作りで、試験で 1 手ずつ確かめた）。この 2 つのほかの誤りと CSP の違反は無い。
  const mine = problems.filter((p) => !p.includes('415') && !p.includes("Connecting to 'blob:") && p !== 'console: csp: connect-src blob');
  expect(mine).toEqual([]);
});

test('大きな file（15 MiB）も、上げられる（中継の時間内に置ける）', async ({ page, request }) => {
  await placePage(request, 'pg-big', 'K-1', 'plan', html('計画'));
  await openLive(page, '/pages/?project=pg-big&key=K-1');
  const box = uploadBox(page);
  await box.locator('input[type=file]').setInputFiles({ name: 'big.txt', mimeType: 'text/plain', buffer: Buffer.alloc(15 * 1024 * 1024, 'a') });
  await box.getByRole('checkbox').check();
  const started = Date.now();
  await box.getByRole('button', { name: '送る' }).click();
  await expect(box.locator('form > .say')).toContainText('上げた。', { timeout: 15_000 });
  console.log(`15 MiB の上げ: ${Date.now() - started} ms`);
  await expect(fileRows(page).first().locator('.who')).toContainText('15.0 MiB');
});

// --- 続きの頁 ---
// pagetray の画面（古い画面）と中身を比べていた試験は、pagetray が 2.0.0 で画面を外したので外した（比べる相手が無い。
// 外す前の比べの結果は、AI-32 の成果のページと、callboard の PR #8 にある）。比べていた事のうち、続きの頁を辿る事と、
// 1 頁 200 行の時に行の path を 200 並べて聞く ListReviews（14 KB ほどの URL）が通る事は残す。

test('続きの link を辿ると、201 枚が 200 枚と 1 枚の 2 頁に出る（新しい順と古い順。project の名は本物の長さ）', async ({ page, request }) => {
  const project = '-Users-someone-projects-more-many';
  for (let i = 0; i < 201; i += 1) {
    await placePage(request, project, `K-${i % 7}`, 'note', html(`記録 ${i}`), 'tpm', `workspace:${i % 3}`);
  }
  for (const sort of ['', '&sort=created_at']) {
    await page.goto(`/pages/?project=${project}${sort}`);
    await expect(pageRows(page)).toHaveCount(200);
    const seen = new Set(await pageRows(page).evaluateAll((rows) => rows.map((r) => r.getAttribute('data-path'))));
    await page.getByRole('link', { name: '続きを出す' }).click();
    await expect(page).toHaveURL(new RegExp(`/pages/\\?project=${project}(&sort=created_at)?&at=[0-9a-f]+$`));
    await expect(pageRows(page)).toHaveCount(1);
    const last = await pageRows(page).first().getAttribute('data-path');
    expect(seen.has(last), `${last} が 2 度出た`).toBe(false);
    await expect(page.getByRole('link', { name: '続きを出す' })).toHaveCount(0);
  }
});

test('同じ名の引数を 2 度書いた物と、値の形が違う引数は、400 の画面で訳を書く', async ({ request }) => {
  const twice = await request.get('/pages/?kind=a&kind=b');
  expect(twice.status()).toBe(400);
  expect(await twice.text()).toContain('引数を読めません（kind を 2 度書いています）');
  for (const query of ['?project=a%2Fb', '?kind=Plan', '?at=zz', '?actor=not-a-uuid']) {
    const now = await request.get(`/pages/${query}`);
    expect(now.status(), query).toBe(400);
    expect(await now.text(), query).toContain('pagetray が引数を断りました');
  }
});

test('上げる道は、ほかの site と、本文の枠の中のページからは使えない', async ({ request }) => {
  await placePage(request, 'pg-guard', 'K-1', 'plan', html('計画'));
  const path = '/api/pagetray/projects/pg-guard/keys/K-1/files?name=a.png';
  for (const headers of [
    { origin: 'https://evil.example', 'sec-fetch-site': 'cross-site' },
    { origin: PAGETRAY, 'sec-fetch-site': 'same-site' },
    { origin: 'null' },
    {},
  ]) {
    const response = await request.post(path, { data: PNG, headers: { 'content-type': 'image/png', ...headers } });
    expect(response.status(), JSON.stringify(headers)).toBe(403);
  }
  const files = await (await request.get(`${PAGETRAY}/api/files?project=pg-guard`)).json();
  expect(files.files).toEqual([]);
});
