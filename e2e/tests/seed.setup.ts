// 見本の記録を、castlist の API で置く（ほかの試験より先に 1 度だけ。browser の外から、castlist に直に送る。callboard は書く道を持たない）。
//   pm.1（人）─起動した→ tpm.1（opus-5-5 · high）─起動した→ se.1（指定は opus、実際は sonnet-5 · high。/clear で退く）
//   se.1 ─引き継いだ→ se.2（opus-5-5 · high）
//   tpm.1 ─起動した→ other.1（別の Key の t-99。haiku-5 · low。退いた）
// 点は 5 つ、線は 4 本（起動した 3・引き継いだ 1）。
// 時刻ずれ（送り手の時刻が 10 分前。castlist は 5 分より離れた行に印を立てる）は、出どころごとに別のアクターに 1 つずつ:
//   pm.1 は登録・tpm.1 は出来事（登場）・other.1 は model と effort（実際の値）。se.1 と se.2 には無い。
// トークンの使用量の行（castlist 2.1.0 から。使用量の画面 /usage/ が読む）は 3 つ: se.1 の本体（3 回・$1.50）と
// 手伝い（Explore。2 回・$0.25）、other.1 の本体（1 回・$0.01）。どれも、今の時（UTC）の内の行。
import { expect, test as setup, type APIRequestContext } from '@playwright/test';
import { CASTLIST } from './helpers';

const now = () => new Date().toISOString();
/** 時計が 10 分遅れた送り手の時刻。 */
const late = () => new Date(Date.now() - 10 * 60 * 1000).toISOString();

async function send(request: APIRequestContext, method: 'post' | 'put', url: string, data: Record<string, unknown>): Promise<void> {
  const response = await request[method](`${CASTLIST}${url}`, { data });
  expect(response.ok(), `${url}: ${response.status()} ${await response.text()}`).toBeTruthy();
}

function seat(prefix: string, session: string, key: string, model: string, effort: string): Record<string, unknown> {
  return {
    issuer: 'tpm',
    kind: 'ai',
    namePrefix: prefix,
    labels: { project: 'castlist', 'tpm.key': key },
    bindings: [{ type: 'claude.session', value: session }],
    settings: { model, effort },
    sentAt: now(),
  };
}

const event = (by: string, type: string) => ({ issuer: 'tpm', by, type, sentAt: now() });
const line = (type: string, from: string, to: string) => ({ issuer: 'tpm', type, from, to, sentAt: now() });

/** 数の 1 行（PutUsage の本文）。区間は、今の時（UTC）の初め。`buckets` は `[段, 回の数, 額]`（和は、行の回の数と額に合わせる）。 */
function usage(by: string, agent: string | undefined, maxContext: number, buckets: [number, number, number][]): Record<string, unknown> {
  const hour = new Date(Math.floor(Date.now() / 3_600_000) * 3_600_000).toISOString();
  return {
    issuer: 'tpm',
    by,
    model: 'claude-opus-5-5',
    ...(agent ? { agent } : {}),
    speed: 'standard',
    serviceTier: 'standard',
    fromAt: hour,
    toAt: hour,
    turns: buckets.reduce((sum, b) => sum + b[1], 0),
    inputTokens: 10,
    outputTokens: 20,
    cacheReadTokens: 30,
    cacheWrite5mTokens: 0,
    cacheWrite1hTokens: 40,
    webSearchRequests: 0,
    webFetchRequests: 0,
    costMicroUsd: buckets.reduce((sum, b) => sum + b[2], 0),
    maxContext,
    buckets: buckets.map(([bucket, turns, costMicroUsd]) => ({ bucket, turns, costMicroUsd })),
    sentAt: now(),
  };
}

setup('見本の記録を置く', async ({ request }) => {
  const PM = 'tpm.person:pm';
  const TPM = 'claude.session:s-tpm';
  const SE1 = 'claude.session:s-se1';
  const SE2 = 'claude.session:s-se2';
  const OTHER = 'claude.session:s-other';
  await send(request, 'post', '/api/actors', {
    issuer: 'tpm',
    kind: 'human',
    namePrefix: 'pm',
    labels: { project: 'castlist' },
    bindings: [{ type: 'tpm.person', value: 'pm' }],
    sentAt: late(),
  });
  await send(request, 'put', '/api/events/e-pm-in', event(PM, 'entered'));
  await send(request, 'post', '/api/actors', seat('tpm', 's-tpm', 't-23zs', 'claude-opus-5-5', 'high'));
  await send(request, 'put', '/api/links/l-pm-tpm', line('launched', PM, TPM));
  await send(request, 'put', '/api/events/e-tpm-in', { ...event(TPM, 'entered'), sentAt: late() });
  await send(request, 'post', '/api/actors', seat('se', 's-se1', 't-23zs', 'claude-opus-5-5', 'high'));
  await send(request, 'put', '/api/links/l-tpm-se1', line('launched', TPM, SE1));
  await send(request, 'put', '/api/events/e-se1-in', event(SE1, 'entered'));
  await send(request, 'put', '/api/settings/t-se1-sonnet', {
    issuer: 'tpm',
    by: SE1,
    model: 'claude-sonnet-5',
    effort: 'high',
    source: 'observed',
    sentAt: now(),
  });
  await send(request, 'put', '/api/events/e-se1-clear', event(SE1, 'cleared'));
  await send(request, 'post', '/api/actors', seat('se', 's-se2', 't-23zs', 'claude-opus-5-5', 'high'));
  await send(request, 'put', '/api/links/l-se1-se2', line('continued', SE1, SE2));
  await send(request, 'put', '/api/events/e-se2-in', event(SE2, 'entered'));
  await send(request, 'post', '/api/actors', seat('other', 's-other', 't-99', 'claude-haiku-5', 'low'));
  await send(request, 'put', '/api/links/l-tpm-other', line('launched', TPM, OTHER));
  await send(request, 'put', '/api/events/e-other-in', event(OTHER, 'entered'));
  // castlist は、最新と同じ値の行を足さない。値を 1 度替えて（時刻ずれの行）、すぐ戻す（最新は haiku-5 · low のまま）。
  const observed = (effort: string, sentAt: string) => ({ issuer: 'tpm', by: OTHER, model: 'claude-haiku-5', effort, source: 'observed', sentAt });
  await send(request, 'put', '/api/settings/t-other-medium', observed('medium', late()));
  await send(request, 'put', '/api/settings/t-other-low', observed('low', now()));
  // other.1 は、roster が落ちたと推して閉じた（推しの幕引き）。
  await send(request, 'put', '/api/events/e-other-out', {
    ...event(OTHER, 'closed'),
    detail: { inferred: true, reason: 'workspace_gone' },
  });

  // トークンの使用量の行（使用量の画面）。
  await send(request, 'put', '/api/usage/u-se1-main', usage(SE1, undefined, 230_000, [[1, 1, 500_000], [9, 2, 1_000_000]]));
  await send(request, 'put', '/api/usage/u-se1-explore', usage(SE1, 'Explore', 30_000, [[1, 2, 250_000]]));
  await send(request, 'put', '/api/usage/u-other-main', usage(OTHER, undefined, 10_000, [[0, 1, 10_000]]));

  const graph = await (await request.get(`${CASTLIST}/api/graph`)).json();
  expect(graph.nodes.map((n: { name: string }) => n.name).sort()).toEqual(['other.1', 'pm.1', 'se.1', 'se.2', 'tpm.1']);
  expect(graph.edges).toHaveLength(4);
});
