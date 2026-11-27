// 一覧（/actors/）とタイムライン（/graph/timeline）を、本物の castlist を相手に見る。見本の記録は seed.setup.ts が castlist に置く。
// castlist の画面と中身を比べていた試験（新旧の比べ）は、castlist が画面を外した 2.0.0 で、比べる相手が無くなったので外した。
// ここは、その試験が一緒に見ていた「見本がそのとおりに出ること」を引き継ぐ。
import { expect, test, type APIRequestContext } from '@playwright/test';
import { CASTLIST, PAGETRAY } from './helpers';

async function main(request: APIRequestContext, path: string): Promise<string> {
  const page = await request.get(path);
  expect(page.status(), path).toBe(200);
  const found = /<main>([\s\S]*)<\/main>/.exec(await page.text());
  expect(found, 'main が無い').toBeTruthy();
  return found![1];
}

const names = (html: string): string[] => [...html.matchAll(/<a class="name" href="[^"]+">([^<]+)<\/a>/g)].map((m) => m[1]);

test('一覧は、見本のアクターを絞りのとおりに出し、時刻ずれの印を出どころごとに 1 人ずつ付ける', async ({ request }) => {
  const all = await main(request, '/actors/?label=project:castlist');
  expect(names(all).sort()).toEqual(['other.1', 'pm.1', 'se.1', 'se.2', 'tpm.1']);
  // 時刻ずれ: pm.1 は登録・tpm.1 は出来事・other.1 は model と effort（seed.setup.ts）。
  expect(all.match(/class="skew"/g)).toHaveLength(3);
  expect(names(await main(request, '/actors/?label=tpm.key:t-23zs')).sort()).toEqual(['se.1', 'se.2', 'tpm.1']);
  expect(names(await main(request, '/actors/?kind=human'))).toEqual(['pm.1']);
  expect(names(await main(request, '/actors/?skewed=true')).sort()).toEqual(['other.1', 'pm.1', 'tpm.1']);
  expect(names(await main(request, '/actors/?model=claude-sonnet-5'))).toEqual(['se.1']);
  expect(await main(request, '/actors/?active_from=2000-01-01T00:00&active_to=2000-01-02T00:00')).toContain('条件に合うアクターはいません。');
  // 条件が無ければ、この 7 日に稼働した者（見本は今の記録）。
  const recent = await main(request, '/actors/');
  expect(recent).toContain('条件が無いので、この 7 日に稼働したアクター');
  expect(names(recent)).toHaveLength(5);
});

test('一覧の次のページを辿ると、穴も重なりも無く終わる', async ({ request }) => {
  const seen: string[] = [];
  let query: string | undefined = '?label=project:castlist&limit=2';
  let pages = 0;
  while (query) {
    const html = await main(request, `/actors/${query}`);
    seen.push(...names(html));
    pages += 1;
    const next = /rel="next" href="\/actors\/(\?[^"]+)"/.exec(html);
    query = next?.[1].replaceAll('&#38;', '&');
  }
  expect(pages).toBe(3);
  expect([...seen].sort()).toEqual(['other.1', 'pm.1', 'se.1', 'se.2', 'tpm.1']);
});

test('タイムラインは、見本のアクターを 1 行ずつ描き、推しの幕引きの帯を出す', async ({ request }) => {
  const all = await main(request, '/graph/timeline');
  expect(all.match(/<g class="lane /g)).toHaveLength(5);
  expect(all.match(/<g class="lane human">/g)).toHaveLength(1);
  // 線: 起動した 3 本・引き継いだ 1 本。
  expect(all.match(/<path class="link launched"/g)).toHaveLength(3);
  expect(all.match(/<path class="link continued"/g)).toHaveLength(1);
  // 推しの幕引きで終わった other.1 の、推しの帯。
  expect(all).toContain('（推し: 居場所が消えた）');
  const one = await main(request, '/graph/timeline?label=tpm.key:t-99');
  // other.1 と、線の向こうの端の tpm.1。
  expect(one.match(/<g class="lane /g)).toHaveLength(2);
  expect(await main(request, '/graph/timeline?from=2000-01-01T00:00&to=2000-01-02T00:00')).toContain('条件に合うアクターはいません。');
});

test('引数の誤りは、引数の名を書いた 400 の画面', async ({ request }) => {
  for (const [path, word] of [
    ['/actors/?nope=1', '知らない引数: nope'],
    ['/actors/?label=nocolon', 'label は「鍵:値」の形（nocolon）'],
    ['/actors/?active_from=yesterday', '「稼働した期間（から）」の日時を読めません。'],
    ['/graph/timeline?kind=ai', '知らない引数: kind（使えるのは label・from・to）'],
    ['/graph/network?label=x', 'label は「鍵:値」の形（x）'],
  ]) {
    const page = await request.get(path);
    expect(page.status(), path).toBe(400);
    expect(await page.text(), path).toContain(word);
  }
});

test('相手の castlist は画面を持たない（2.0.0 から）。前の画面の道は 404', async ({ request }) => {
  for (const path of ['/', '/graph/timeline', '/graph/network', '/static/site.css']) {
    expect((await request.get(`${CASTLIST}${path}`)).status(), path).toBe(404);
  }
  expect((await request.get(`${CASTLIST}/api/timeline`)).status()).toBe(200);
});

test('相手の pagetray は画面を持たない（2.0.0 から）。前の画面の道は 404 で、置いたページと API は前のまま', async ({ request }) => {
  for (const path of ['/', '/?project=x', '/x/', '/x/K-1/', '/messages']) {
    const answer = await request.get(`${PAGETRAY}${path}`);
    expect(answer.status(), path).toBe(404);
    expect(answer.headers()['content-type'] ?? '', path).not.toContain('text/html');
  }
  for (const path of ['/api/pages', '/api/pages/facets', '/api/messages', '/api/files/rules']) {
    expect((await request.get(`${PAGETRAY}${path}`)).status(), path).toBe(200);
  }
});
