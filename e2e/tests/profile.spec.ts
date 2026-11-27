// プロフィール（/actors/{id}）を、本物の castlist を相手に見る。見本の記録は seed.setup.ts が castlist に置く。
// 「新旧の比べ」: 同じ castlist を相手に、castlist の画面（古い画面）と callboard の画面の中身を比べる。
import { expect, test, type APIRequestContext } from '@playwright/test';
import { CASTLIST } from './helpers';

type Node = { id: string; name: string };

async function nodes(request: APIRequestContext): Promise<Node[]> {
  const response = await request.get(`${CASTLIST}/api/graph`);
  expect(response.ok()).toBeTruthy();
  return (await response.json()).nodes;
}

async function byName(request: APIRequestContext, name: string): Promise<Node> {
  const node = (await nodes(request)).find((n) => n.name === name);
  expect(node, `${name} が無い`).toBeTruthy();
  return node!;
}

/**
 * 画面の `<main>` の中身。時刻の字の秒の下は、3 桁にそろえる（castlist の画面は、送り手の時刻を送られた字のまま出し、
 * callboard は API の時刻をミリ秒までの字にするので、同じ時刻でも字が違う）。
 */
function main(html: string): string {
  const found = /<main>([\s\S]*)<\/main>/.exec(html);
  expect(found, 'main が無い').toBeTruthy();
  return found![1].replace(/(T\d\d:\d\d:\d\d)(?:\.(\d+))?Z/g, (_, time: string, fraction = '') => `${time}.${`${fraction}000`.slice(0, 3)}Z`);
}

test('新旧の比べ: どのアクターのプロフィールも、castlist の画面と同じ中身', async ({ request }) => {
  const all = await nodes(request);
  expect(all).toHaveLength(5);
  for (const node of all) {
    const old = await request.get(`${CASTLIST}/actors/${node.id}`);
    const now = await request.get(`/actors/${node.id}`);
    expect(old.status(), node.name).toBe(200);
    expect(now.status(), node.name).toBe(200);
    expect(main(await now.text()), node.name).toBe(main(await old.text()));
  }
});

test('新旧の比べ: 無いアクターと、id の形でない物は、どちらも 404 の画面', async ({ request }) => {
  for (const id of ['019a0000-0000-7000-8000-0000000000ff', 'no-such']) {
    const old = await request.get(`${CASTLIST}/actors/${id}`);
    const now = await request.get(`/actors/${id}`);
    expect(old.status(), id).toBe(404);
    expect(now.status(), id).toBe(404);
    expect(await now.text()).toContain('そのアクターは無い');
  }
});

test('プロフィールは、起動した人・引き継ぎ・食い違いを出し、link で次のプロフィールへ行ける', async ({ page, request }) => {
  const se1 = await byName(request, 'se.1');
  const se2 = await byName(request, 'se.2');
  await page.goto(`/actors/${se1.id}`);
  await expect(page).toHaveTitle('se.1 ・ callboard');
  await expect(page.locator('h1')).toHaveText('se.1');
  // 起動した人: 人の起点（pm.1）から。
  await expect(page.locator('#launched-by li')).toHaveText([/^pm\.1 人・起動した$/, /^tpm\.1 起動した$/, 'se.1']);
  // /clear で退いた。
  await expect(page.locator('#state')).toContainText('停止');
  await expect(page.locator('#state .event')).toHaveText('交代');
  // 指定は opus、実際は sonnet。
  await expect(page.locator('#settings .mismatch')).toHaveCount(1);
  // 引き継ぎ先へ。
  await page.locator('#continued').getByRole('link', { name: 'se.2' }).click();
  await expect(page).toHaveURL(`/actors/${se2.id}`);
  await expect(page.locator('h1')).toHaveText('se.2');
  await expect(page.locator('#state')).toContainText('稼働中');
  await expect(page.locator('#continued')).toContainText('se.1');
});

test('プロフィールは、推しの幕引きで終わった者に停止（推し）と書き、人に種類の印を付ける', async ({ page, request }) => {
  const other = await byName(request, 'other.1');
  await page.goto(`/actors/${other.id}`);
  await expect(page.locator('#state .state')).toHaveText('停止（推し）');
  await expect(page.locator('#events')).toContainText('workspace_gone');
  const pm = await byName(request, 'pm.1');
  await page.goto(`/actors/${pm.id}`);
  await expect(page.locator('h1')).toHaveText('pm.1 人');
  await expect(page.locator('#launched-by')).toContainText('ありません（起点）。');
  await expect(page.locator('#children')).toContainText('tpm.1');
});

test('プロフィールは script を使わず、console の誤りも CSP の違反も無い', async ({ page, request }) => {
  const problems: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error') problems.push(`console: ${m.text()}`);
  });
  page.on('pageerror', (e) => problems.push(`pageerror: ${e.message}`));
  await page.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (e) => console.error(`csp: ${e.violatedDirective} ${e.blockedURI}`));
  });
  const se1 = await byName(request, 'se.1');
  await page.goto(`/actors/${se1.id}`);
  await expect(page.locator('script')).toHaveCount(0);
  await expect(page.locator('link[rel="stylesheet"]')).toHaveCount(1);
  expect(problems).toEqual([]);
});
