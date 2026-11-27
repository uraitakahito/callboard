// 試験の道具。
import { expect, type APIRequestContext, type Page } from '@playwright/test';

/** 試験の castlist（playwright.config.ts が起こす）。見本の記録を置く時と、古い画面・API と比べる時に、直に聞く。 */
export const CASTLIST = `http://127.0.0.1:${process.env.CALLBOARD_E2E_CASTLIST_PORT}`;

// --- pagetray（メッセージの一覧） ---

/** 試験の pagetray（playwright.config.ts が起こす）。記録を置く時と、古い画面と比べる時に、直に聞く。 */
export const PAGETRAY = `http://127.0.0.1:${process.env.CALLBOARD_E2E_PAGETRAY_PORT}`;

export type Message = {
  key?: string;
  heading: string;
  from: string;
  to: string;
  text: string;
  sentAt: string;
  state: 'waiting' | 'delivered' | 'undeliverable' | 'expired' | 'carried';
  deliveredAt?: string;
  via?: string;
  deliveries?: number;
  replyTo?: string;
};

/** pagetray の API で、メッセージの記録を 1 件置く・書き足す（browser の外から、pagetray に直に送る。callboard は書く道を持たない）。 */
export async function putMessage(request: APIRequestContext, project: string, id: string, message: Message): Promise<void> {
  const response = await request.put(`${PAGETRAY}/api/projects/${project}/messages/${id}`, { data: message });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** pagetray の API で、ページを 1 枚置き、置き場の中の path（`/<project>/<key>/<kind>-<n>.html`）を返す。 */
export async function placePage(request: APIRequestContext, project: string, key: string, kind: string, html: string, role = 'tpm', workspace = ''): Promise<string> {
  const query = new URLSearchParams({ role, ...(workspace ? { workspace } : {}) });
  const response = await request.post(`${PAGETRAY}/api/projects/${project}/keys/${key}/kinds/${kind}/pages?${query}`, {
    data: html,
    headers: { 'content-type': 'text/html' },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).path;
}

/** ページ 1 枚の API の道（`/api/projects/{p}/keys/{k}/kinds/{kind}/pages/{n}`）。`path` は `placePage` が返した物。 */
const pageApi = (path: string) => {
  const [, project, key, kind, n] = /^\/([^/]+)\/([^/]+)\/(.+)-(\d+)\.html$/.exec(path)!;
  return `${PAGETRAY}/api/projects/${project}/keys/${key}/kinds/${kind}/pages/${n}`;
};

/** pagetray の API で、ページ 1 枚の返事の状態を書く。 */
export async function reply(request: APIRequestContext, path: string, state: 'waiting' | 'answered', on?: 'review' | 'tpm'): Promise<void> {
  const response = await request.put(`${pageApi(path)}/reply`, { data: on ? { state, on } : { state } });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** pagetray の API で、レビューのページに印（判定・見た物）を書く。 */
export async function reviewMarks(request: APIRequestContext, path: string, marks: { verdict?: string; targets?: string[] }): Promise<void> {
  const response = await request.post(`${pageApi(path)}/review`, { data: marks });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** pagetray の API で、file を 1 つ置く。 */
export async function placeFile(request: APIRequestContext, project: string, key: string, name: string, type: string, body: Buffer | string): Promise<void> {
  const response = await request.post(`${PAGETRAY}/api/projects/${project}/keys/${key}/files?name=${name}&role=tpm`, {
    data: body,
    headers: { 'content-type': type },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** 1×1 の PNG。 */
export const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');

/**
 * 一覧（メッセージの一覧か、ページの一覧）を開き、新着の知らせ（callboard が流す道）につながり、つながった時の読み直しが
 * 済むまで待つ。つながる前に置いた記録の知らせは届かない。
 */
export async function openLive(page: Page, url: string): Promise<void> {
  const path = new URL(url, 'http://x').pathname;
  const live = page.waitForResponse((r) => r.url().endsWith('/api/pagetray/events') && r.status() === 200);
  const pulled = page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().resourceType() === 'fetch');
  await page.goto(url);
  await live;
  await (await pulled).finished();
}

/** console の誤り・script の例外・CSP の違反を集める。 */
export function collectProblems(page: Page): string[] {
  const problems: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') problems.push(`console: ${message.text()}`);
  });
  page.on('pageerror', (error) => problems.push(`pageerror: ${error.message}`));
  void page.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (e) => console.error(`csp: ${e.violatedDirective} ${e.blockedURI}`));
  });
  return problems;
}

// メッセージの一覧の区画: 行（`li[data-id]`。値は `<project>/<id>`）・まだ届いていない数・新着の印・絞り込みの列の候補。
export const messageRows = (page: Page) => page.locator('#list li[data-id]');
export const messageRow = (page: Page, project: string, id: string) => page.locator(`#list li[data-id="${project}/${id}"]`);
export const undelivered = (page: Page) => page.locator('#undelivered');
export const fresh = (page: Page) => page.locator('#fresh');
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
/** 絞り込みの列の候補。見出し（状態・受け手・送り手・Key・project）と、候補の名で選ぶ。 */
export const choice = (page: Page, heading: string, label: string) =>
  page
    .locator('#filters')
    .locator(`h2:text-is("${heading}") + ul a`)
    .filter({ has: page.locator('span:first-child', { hasText: new RegExp(`^${escape(label)}`) }) });

// ページの一覧の区画: 行（`li[data-path]`）・待ちの数の釦・本文の枠・Key の画面の file の枠と上げる枠。
export const pageRows = (page: Page) => page.locator('#list li[data-path]');
export const pageRow = (page: Page, path: string) => page.locator(`#list li[data-path="${path}"]`);
export const waitingCount = (page: Page) => page.locator('#waiting');
export const viewer = (page: Page) => page.locator('#viewer');
export const fileRows = (page: Page) => page.locator('#files li[data-file]');
export const uploadBox = (page: Page) => page.locator('#upload');
