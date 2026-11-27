// プロフィール（/actors/{id}）を、本物の castlist を相手に見る。見本の記録は seed.setup.ts が castlist に置く。
// castlist の画面（古い画面）と中身を比べる試験は、castlist が画面を外した 2.0.0 で、比べる相手が無くなったので外した
// （外す前の比べの結果は、AI-32 の成果のページと、callboard の PR #2・#4 にある）。
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

test('どのアクターのプロフィールも開き、無いアクターと id の形でない物は 404 の画面', async ({ request }) => {
  const all = await nodes(request);
  expect(all).toHaveLength(5);
  for (const node of all) {
    const page = await request.get(`/actors/${node.id}`);
    expect(page.status(), node.name).toBe(200);
    expect(await page.text(), node.name).toContain(`<code>${node.id}</code>`);
  }
  for (const id of ['019a0000-0000-7000-8000-0000000000ff', 'no-such']) {
    const page = await request.get(`/actors/${id}`);
    expect(page.status(), id).toBe(404);
    expect(await page.text()).toContain('そのアクターは無い');
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

test('時刻ずれの印は、登録・出来事・model と effort のどれがずれたアクターにも付き、ずれの無いアクターには付かない', async ({ page, request }) => {
  for (const [name, skewed] of [['pm.1', true], ['tpm.1', true], ['other.1', true], ['se.1', false], ['se.2', false]] as const) {
    const node = await byName(request, name);
    await page.goto(`/actors/${node.id}`);
    await expect(page.locator('#state .skew'), name).toHaveCount(skewed ? 1 : 0);
  }
});

test('プロフィールは自分の script を使わず、console の誤りも CSP の違反も無い', async ({ page, request }) => {
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
  // 画面の script は、どの画面も head で読む字の差し替えの 4 つ（AI-50）だけ。
  await expect(page.locator('body script')).toHaveCount(0);
  await expect(page.locator('head script[src]')).toHaveCount(4);
  await expect(page.locator('link[rel="stylesheet"]')).toHaveCount(1);
  expect(problems).toEqual([]);
});
