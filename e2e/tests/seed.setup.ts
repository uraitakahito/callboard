// 見本の記録を、castlist の API で置く（ほかの試験より先に 1 度だけ。browser の外から、castlist に直に送る。callboard は書く道を持たない）。
//   pm.1（人）─起動した→ tpm.1（opus-5-5 · high）─起動した→ se.1（指定は opus、実際は sonnet-5 · high。/clear で退く）
//   se.1 ─引き継いだ→ se.2（opus-5-5 · high）
//   tpm.1 ─起動した→ other.1（別の Key の t-99。haiku-5 · low。退いた）
// 点は 5 つ、線は 4 本（起動した 3・引き継いだ 1）。
// 時刻ずれ（送り手の時刻が 10 分前。castlist は 5 分より離れた行に印を立てる）は、出どころごとに別のアクターに 1 つずつ:
//   pm.1 は登録・tpm.1 は出来事（登場）・other.1 は model と effort（実際の値）。se.1 と se.2 には無い。
// トークンの使用量の行（castlist 2.1.0 から。使用量の画面 /usage/ が読む）は 4 つ: se.1 の本体（Opus。3 回・$1.50）と
// 手伝い 2 つ（どちらも Opus。Explore 1 回・$0.10 と general-purpose 1 回・$0.15。計は 2 回・$0.25）、other.1 の本体
// （Haiku。1 回・$0.01）。どれも、今の時（UTC）の内の行。model と手伝いの種類は model と役割ごとの行列（t-dues）のため。
// 行の数の計（全体・役割・課題・一覧・プロフィールの数）は、行列を足す前と同じにしてある。
// 幅を選ぶ試験（t-ztu7）の行は 2 つ: se.1 の本体の、決まった過去の時刻（2026-10-01）の行。上の数には入らない（期間の外）。
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
function usage(by: string, model: string, agent: string | undefined, maxContext: number, buckets: [number, number, number][]): Record<string, unknown> {
  const hour = new Date(Math.floor(Date.now() / 3_600_000) * 3_600_000).toISOString();
  return {
    issuer: 'tpm',
    by,
    model,
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

/**
 * 決まった過去の時刻の、数の 1 行（本体。Opus。1 回）。幅を選ぶ試験（usage.spec.ts）が、期間を名指して開く。
 * 今の時刻に依らないように、区間も送り手の時刻も決めた字にする（ほかの試験の「直近 7 日」「直近 31 日」に入らない、古い日）。
 */
function usageAt(by: string, fromAt: string, toAt: string, costMicroUsd: number): Record<string, unknown> {
  return {
    ...usage(by, 'claude-opus-5-5', undefined, 30_000, [[1, 1, costMicroUsd]]),
    fromAt,
    toAt,
    sentAt: '2026-10-01T02:00:00.000Z',
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
  await send(request, 'put', '/api/usage/u-se1-main', usage(SE1, 'claude-opus-5-5', undefined, 230_000, [[1, 1, 500_000], [9, 2, 1_000_000]]));
  await send(request, 'put', '/api/usage/u-se1-explore', usage(SE1, 'claude-opus-5-5', 'Explore', 30_000, [[1, 1, 100_000]]));
  await send(request, 'put', '/api/usage/u-se1-general', usage(SE1, 'claude-opus-5-5', 'general-purpose', 30_000, [[1, 1, 150_000]]));
  await send(request, 'put', '/api/usage/u-other-main', usage(OTHER, 'claude-haiku-5-5', undefined, 10_000, [[0, 1, 10_000]]));

  // 幅を選ぶ試験の行（2026-10-01 の日本時間 10:30〜11:00。今の時刻に依らない）。
  // 10 分で切った行（10:30:00.000〜10:39:59.999。$1.20）と、10 分で切る前の形の行（10:45:00〜10:54:59。$0.60。10:40 と 10:50 の枠に半分ずつ）。
  await send(request, 'put', '/api/usage/u-se1-cut', usageAt(SE1, '2026-10-01T01:30:00.000Z', '2026-10-01T01:39:59.999Z', 1_200_000));
  await send(request, 'put', '/api/usage/u-se1-crossing', usageAt(SE1, '2026-10-01T01:45:00.000Z', '2026-10-01T01:54:59.000Z', 600_000));

  const graph = await (await request.get(`${CASTLIST}/api/graph`)).json();
  expect(graph.nodes.map((n: { name: string }) => n.name).sort()).toEqual(['other.1', 'pm.1', 'se.1', 'se.2', 'tpm.1']);
  expect(graph.edges).toHaveLength(4);
});
