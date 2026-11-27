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

/** pagetray の API で、ページを 1 枚置く（ページを置いた知らせを流すため）。 */
export async function placePage(request: APIRequestContext, project: string, key: string, kind: string, html: string): Promise<void> {
  const response = await request.post(`${PAGETRAY}/api/projects/${project}/keys/${key}/kinds/${kind}/pages?role=tpm`, {
    data: html,
    headers: { 'content-type': 'text/html' },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** メッセージの一覧を開き、新着の知らせ（callboard が流す道）につながるまで待つ。つながる前に置いた記録の知らせは届かない。 */
export async function openLive(page: Page, url: string): Promise<void> {
  const live = page.waitForResponse((r) => r.url().endsWith('/api/pagetray/events') && r.status() === 200);
  await page.goto(url);
  await live;
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
