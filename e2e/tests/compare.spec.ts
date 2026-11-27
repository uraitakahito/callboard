// 新旧の比べ: 同じ castlist を相手に、castlist の画面（古い画面）と callboard の画面の中身（`<main>` の中）を比べる。
// castlist の画面を外す段の前の関門。見本の記録は seed.setup.ts が castlist に置く。プロフィールの比べは profile.spec.ts。
import { expect, test, type APIRequestContext } from '@playwright/test';
import { CASTLIST } from './helpers';

/**
 * 画面の `<main>` の中身を、比べられる形にそろえる。
 * - 時刻の字の秒の下は 3 桁に（castlist の画面は、送り手の時刻を送られた字のまま出す）。
 * - 期間をすぐ選ぶ link の期間は、問うた時の今で決まるので、値を落とす。
 */
function main(html: string): string {
  const found = /<main>([\s\S]*)<\/main>/.exec(html);
  expect(found, 'main が無い').toBeTruthy();
  return found![1]
    .replace(/(T\d\d:\d\d:\d\d)(?:\.(\d+))?Z/g, (_, time: string, fraction = '') => `${time}.${`${fraction}000`.slice(0, 3)}Z`)
    .replace(/(active_from|active_to|from|to)=20\d\d-\d\d-\d\dT[0-9:%A-Z.+]+/g, '$1=T');
}

/** castlist の画面の道（一覧は `/`）を、callboard の道（`/actors/`）に読み替える。 */
function moved(html: string): string {
  return html.replaceAll('action="/"', 'action="/actors/"').replaceAll('href="/"', 'href="/actors/"').replaceAll('href="/?', 'href="/actors/?');
}

async function both(request: APIRequestContext, old: string, now: string): Promise<{ old: string; now: string }> {
  const [a, b] = [await request.get(`${CASTLIST}${old}`), await request.get(now)];
  expect(a.status(), old).toBe(200);
  expect(b.status(), now).toBe(200);
  return { old: main(moved(await a.text())), now: main(await b.text()) };
}

test('新旧の比べ: 一覧は、どの絞りでも castlist の画面と同じ中身', async ({ request }) => {
  for (const query of [
    '',
    '?label=tpm.key:t-23zs',
    '?label=tpm.key:t-23zs&label=project:castlist',
    '?active=true',
    '?active=false&label=project:castlist',
    '?kind=human',
    '?model=claude-sonnet-5',
    '?effort=low',
    '?skewed=true',
    '?label=project:castlist&order=asc&sort=exited',
    '?active_from=2000-01-01T00:00&active_to=2000-01-02T00:00',
  ]) {
    const page = await both(request, `/${query}`, `/actors/${query}`);
    expect(page.now, query).toBe(page.old);
  }
  // 見本の 5 人が出ていること（空の画面どうしを比べていない）。
  const all = await both(request, '/?label=project:castlist', '/actors/?label=project:castlist');
  expect(all.now.match(/<a class="name"/g)).toHaveLength(5);
  // 時刻ずれの印は、出どころごとに 1 人ずつ（seed.setup.ts）。
  expect(all.now.match(/class="skew"/g)).toHaveLength(3);
});

test('新旧の比べ: 一覧の次のページを辿っても、castlist の画面と同じ中身', async ({ request }) => {
  let query: string | undefined = '?label=project:castlist&limit=2';
  let pages = 0;
  while (query) {
    const page = await both(request, `/${query}`, `/actors/${query}`);
    expect(page.now, query).toBe(page.old);
    pages += 1;
    const next = /rel="next" href="\/actors\/(\?[^"]+)"/.exec(page.now);
    query = next?.[1].replaceAll('&#38;', '&');
  }
  expect(pages).toBe(3);
});

test('新旧の比べ: タイムラインは、どの絞りでも castlist の画面と同じ図', async ({ request }) => {
  // 窓の終わりを、今より前（この試験の始まり）にする。稼働中の帯は窓の右端で切れ、今の線は窓の外になるので、
  // 2 つの問いの間に時刻が進んでも、同じ図になる。
  // （秒に切り捨てない。見本は、この試験の少し前に置いた物なので、切り捨てると窓の外になる。）
  const to = new Date();
  const from = new Date(to.getTime() - 10 * 60 * 1000);
  const window = `from=${encodeURIComponent(from.toISOString())}&to=${encodeURIComponent(to.toISOString())}`;
  for (const query of [`?${window}`, `?label=tpm.key:t-23zs&${window}`, `?label=tpm.key:t-99&${window}`, '?from=2000-01-01T00:00&to=2000-01-02T00:00']) {
    const page = await both(request, `/graph/timeline${query}`, `/graph/timeline${query}`);
    expect(page.now, query).toBe(page.old);
  }
  const all = await both(request, `/graph/timeline?${window}`, `/graph/timeline?${window}`);
  expect(all.now.match(/<g class="lane /g)).toHaveLength(5);
  // 推しの幕引きで終わった other.1 の、推しの帯。
  expect(all.now).toContain('（推し: 居場所が消えた）');
});

test('新旧の比べ: 引数の誤りは、castlist の画面と同じ状態と訳', async ({ request }) => {
  const message = (html: string) => /<p class="error">([\s\S]*?)<\/p>/.exec(html)?.[1];
  for (const [old, now] of [
    ['/?nope=1', '/actors/?nope=1'],
    ['/?label=nocolon', '/actors/?label=nocolon'],
    ['/?limit=0', '/actors/?limit=0'],
    ['/?after=zz', '/actors/?after=zz'],
    ['/?active_from=yesterday', '/actors/?active_from=yesterday'],
    ['/?at=2026-11-22T00:00:00Z', '/actors/?at=2026-11-22T00:00:00Z'],
    ['/graph/timeline?kind=ai', '/graph/timeline?kind=ai'],
    ['/graph/timeline?from=bad', '/graph/timeline?from=bad'],
    ['/graph/network?label=x', '/graph/network?label=x'],
  ]) {
    const [a, b] = [await request.get(`${CASTLIST}${old}`), await request.get(now)];
    expect(b.status(), now).toBe(400);
    expect(a.status(), old).toBe(400);
    expect(message(await b.text()), now).toBe(message(await a.text()));
  }
});
