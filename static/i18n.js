// 画面の字を言語ごとに差し替える（AI-50）。callboard が /static/i18n.js で配る（CSP の script-src 'self'）。
// どの画面も <head> で、i18next（static/vendor/）・訳（/static/locales/ja.js と en.js）・この file の順に、defer を付けずに読む。
// - サーバーは日本語の字を HTML に書き、字の要素に鍵（data-i18n）を付ける。日本語の時は、ここは DOM を触らない。
// - 言語は cookie（callboard-lang）→ ブラウザーの言語の並びの初めの 1 つ → 日本語、の順に決める。ja と en のほかは日本語。
// - 英語の時は、<head> の内に <html> へ i18n-wait を付けて本文を隠し（site.css）、DOMContentLoaded で字を差し替えてから外す。
//   ここが途中で止まっても、site.css の守りが 1.5 秒で本文を見せる（日本語のまま）。
// - 訳の中身は同期の script で読んであるので、i18next は同期で起きる。本文の script（messages.js など）は、起きた時から
//   window.callboardT で字を引ける。新着で server の HTML を読み直す script は、比べと差し替えの前に
//   window.callboardLocalize(読み直した木) で訳す。JavaScript は、起きた時に DOM の字を読まない（日本語を取ってしまう）。
// - 字は textContent と属性でだけ置く（innerHTML は使わない。i18next の escapeValue は切る）。
(() => {
  'use strict';

  const COOKIE = 'callboard-lang';
  const LANGS = ['ja', 'en'];
  const root = document.documentElement;

  /** 言語の名（en-US など）を、ja か en に寄せる。どちらでもなければ null。 */
  function normalize(name) {
    const base = String(name || '').toLowerCase().split('-')[0];
    return LANGS.includes(base) ? base : null;
  }

  /** cookie の言語。無ければ null。 */
  function cookieLang() {
    for (const part of document.cookie.split(';')) {
      const [name, value] = part.trim().split('=');
      if (name === COOKIE) return normalize(decodeURIComponent(value || ''));
    }
    return null;
  }

  /** 使う言語: cookie → ブラウザーの言語の並びの初めの 1 つ → 日本語。 */
  function pick() {
    const chosen = cookieLang();
    if (chosen) return chosen;
    const first = (navigator.languages && navigator.languages[0]) || navigator.language;
    return normalize(first) === 'en' ? 'en' : 'ja';
  }

  const locales = window.callboardLocales || {};
  // 選んだ言語の訳が読めていなければ（訳の script が届かないなど）、日本語のまま（字を差し替えない）。
  const picked = pick();
  const lang = locales[picked] ? picked : 'ja';

  // 訳の中身を渡すので、i18next は同期で起きる（initAsync: false は、その明示）。
  i18next.init({
    lng: lang,
    fallbackLng: 'ja',
    supportedLngs: LANGS,
    load: 'languageOnly',
    initAsync: false,
    resources: {
      ja: { translation: locales.ja || {} },
      en: { translation: locales.en || {} },
    },
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  // 本文を隠すのは、i18next が起きてから（起きられなければ、隠さずに日本語のまま見せる）。
  if (lang !== 'ja') root.classList.add('i18n-wait');

  /** 鍵の字（今の言語）。options は i18next の物（値・count）。 */
  window.callboardT = (key, options) => i18next.t(key, options);

  /** data-i18n-options の値（JSON）。読めなければ空。 */
  function options(element) {
    const raw = element.getAttribute('data-i18n-options');
    if (!raw) return undefined;
    try {
      return JSON.parse(raw);
    } catch {
      return undefined;
    }
  }

  /**
   * 与えた木（document か、DOMParser で読んだ物か、要素）の字を、今の言語に差し替える。日本語の時は何もしない。
   * - data-i18n="<鍵>": 字（textContent）を替える。
   * - data-i18n-attr="<属性>:<鍵>;…": 属性（title・aria-label・placeholder など）を替える。
   * - data-i18n-options='{"n":3}': 差し込む値（両方に効く）。
   */
  function localize(tree) {
    if (lang === 'ja' || !tree) return;
    const own = tree.matches && tree.matches('[data-i18n], [data-i18n-attr]') ? [tree] : [];
    for (const element of [...own, ...tree.querySelectorAll('[data-i18n], [data-i18n-attr]')]) {
      const values = options(element);
      const key = element.getAttribute('data-i18n');
      if (key) element.textContent = i18next.t(key, values);
      const attrs = element.getAttribute('data-i18n-attr');
      if (attrs) {
        for (const pair of attrs.split(';')) {
          const at = pair.indexOf(':');
          if (at <= 0) continue;
          element.setAttribute(pair.slice(0, at).trim(), i18next.t(pair.slice(at + 1).trim(), values));
        }
      }
    }
  }
  window.callboardLocalize = localize;

  /** 帯の言語の切り替え。JavaScript が無い時は hidden のまま（切り替えられないので出さない）。 */
  function showSwitch() {
    const box = document.getElementById('lang-switch');
    if (!box) return;
    for (const button of box.querySelectorAll('button[data-lang]')) {
      const target = button.getAttribute('data-lang');
      button.setAttribute('aria-pressed', String(target === lang));
      button.addEventListener('click', () => {
        if (target === lang) return;
        document.cookie = `${COOKIE}=${target}; Path=/; Max-Age=31536000; SameSite=Lax`;
        window.location.reload();
      });
    }
    box.hidden = false;
  }

  document.addEventListener('DOMContentLoaded', () => {
    try {
      localize(document);
      root.lang = lang;
      showSwitch();
    } finally {
      root.classList.remove('i18n-wait');
    }
  });
})();
