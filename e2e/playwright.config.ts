// 画面の browser の試験の設定。出した版の castlist-server と、同じ commit の callboard-server を、空いている port で起こして向ける。
//   CALLBOARD_E2E_CASTLIST  起こす castlist-server の binary（必ず書く。ci/castlist-binary.sh が、出した版の image から取り出す。
//                           手元では、castlist の同じ版の tag を build した物）
//   CALLBOARD_E2E_SERVER    起こす callboard-server の binary（既定は ../target/debug/callboard-server を cargo build してから）
//   CALLBOARD_E2E_CHANNEL   この機械に入っている browser を使う時の名（chrome など）。無ければ Playwright の Chromium
// castlist は一時の DB で起こし、見本の記録は tests/seed.setup.ts が castlist の API で置く（ほかの試験より先に 1 度だけ）。
// pagetray は起こさない（この段の画面は castlist だけを呼ぶ。聞く先は、だれも待っていない所にする）。
import { defineConfig } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// 設定は、試験を流す worker の中でも読み直される。port と DB は初めの 1 度だけ決め、環境変数で worker に渡す。
const freePort = () =>
  execFileSync(process.execPath, [
    '-e',
    'const s = require("node:net").createServer().listen(0, "127.0.0.1", () => { console.log(s.address().port); s.close(); });',
  ])
    .toString()
    .trim();
process.env.CALLBOARD_E2E_PORT ??= freePort();
process.env.CALLBOARD_E2E_CASTLIST_PORT ??= freePort();
process.env.CALLBOARD_E2E_DB ??= join(mkdtempSync(join(tmpdir(), 'callboard-e2e-')), 'castlist.db');
const port = Number(process.env.CALLBOARD_E2E_PORT);
const castlistPort = Number(process.env.CALLBOARD_E2E_CASTLIST_PORT);
const db = process.env.CALLBOARD_E2E_DB;
const castlist = process.env.CALLBOARD_E2E_CASTLIST;
if (!castlist) throw new Error('CALLBOARD_E2E_CASTLIST に、起こす castlist-server の binary を書く（docs/src/development.md）');
const server = process.env.CALLBOARD_E2E_SERVER;
const build = server ? `exec "${server}"` : 'cargo build --locked --bin callboard-server && exec ../target/debug/callboard-server';

export default defineConfig({
  testDir: 'tests',
  // 見本の記録を数える試験があるので、1 本ずつ順に流す。
  fullyParallel: false,
  workers: 1,
  // やり直さない（時々落ちる試験を隠さない）。
  retries: 0,
  forbidOnly: !!process.env.CI,
  outputDir: 'test-results',
  reporter: process.env.CI ? [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]] : 'list',
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    channel: process.env.CALLBOARD_E2E_CHANNEL,
    timezoneId: 'Asia/Tokyo',
    locale: 'ja-JP',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'seed', testMatch: /seed\.setup\.ts/ },
    { name: 'chromium', testMatch: /\.spec\.ts/, dependencies: ['seed'] },
  ],
  webServer: [
    {
      // 前の回の DB を消してから起こす（試験は、いつも空の台帳から始まる）。
      command: `rm -f "${db}" "${db}-wal" "${db}-shm" && exec "${castlist}"`,
      env: { CASTLIST_LISTEN: `127.0.0.1:${castlistPort}`, CASTLIST_DB: db },
      url: `http://127.0.0.1:${castlistPort}/healthz`,
      timeout: 60_000,
      reuseExistingServer: false,
    },
    {
      command: build,
      env: {
        CALLBOARD_LISTEN: `127.0.0.1:${port}`,
        CALLBOARD_CASTLIST_URL: `http://127.0.0.1:${castlistPort}`,
        // 画面の link の頭も、試験の castlist にする（まだ移していない画面への link を辿れる）。
        CALLBOARD_CASTLIST_LINK_URL: `http://127.0.0.1:${castlistPort}`,
        CALLBOARD_PAGETRAY_URL: 'http://127.0.0.1:9',
      },
      url: `http://127.0.0.1:${port}/healthz`,
      // cargo build の分も待つ。
      timeout: 600_000,
      reuseExistingServer: false,
    },
  ],
});
