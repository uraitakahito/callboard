// メッセージの一覧（/messages）の、新着の知らせ。callboard が /static/messages.js で配る（CSP の script-src 'self'）。
// pagetray の templates/messages.html の script を移した物。
// - script が無くても一覧は動く。ここでするのは、新着の知らせでの、一覧の列と絞り込みの列の差し替え。
// - 知らせは、.messages の data-events（pagetray の新着の知らせを callboard がそのまま流す道。connect-src 'self'）から聞く。
// - 差し替える中身は、同じ URL を callboard から読み直した HTML（サーバーが組んで、字を逃がした物）の節。字を HTML として
//   組み立てることはしない（innerHTML に書かない）。
(() => {
  'use strict';

  const root = document.querySelector('.messages');
  const fresh = document.getElementById('fresh');
  const list = document.getElementById('list');
  const filters = document.getElementById('filters');
  const counter = document.getElementById('undelivered');
  if (!root || !fresh || !list || !filters || !counter) return;

  // メッセージの記録が置かれる・書き足されるたびに pagetray が知らせる。一覧の列と絞り込みの列だけを、サーバーが組んだ HTML に
  // 差し替える。古い方の続き（URL に at がある）を見ている時は差し替えず、押すと読み直す印を出す。
  const box = list.parentElement;
  const paged = new URLSearchParams(location.search).has('at');
  let above = 0;
  let pulling = false;
  let again = false;
  // 一覧の列を送る物: 幅が広い時は一覧の列の枠、狭い時は画面全体。
  const scroller = () => (box.scrollHeight > box.clientHeight ? box : document.scrollingElement);
  const viewTop = (el) => (el === box ? box.getBoundingClientRect().top : 0);
  const atTop = () => list.getBoundingClientRect().top >= viewTop(scroller()) - 1;
  const rowsOf = (node) => [...node.querySelectorAll('li[data-id]')];
  // 行の並びと、行ごとの最後に書き足された時刻。どれかが変われば、一覧の列を差し替える。
  const shape = (node) => rowsOf(node).map((li) => `${li.dataset.id} ${li.dataset.at}`).join('\n') + '\n' + (node.querySelector('.more')?.getAttribute('href') ?? '');
  const show = (text) => {
    fresh.hidden = false;
    fresh.textContent = text;
  };
  // 差し替えても、開いていた本文は開いたままにし、下へ送っていた時は、見えていた頭の行を同じ所に置く。
  // 操作の帯の釦を替える band も、この中で呼ぶ（狭い幅では、釦が増えて帯が 2 段になると、下の行がみな動くため）。
  const swap = (next, nextFilters, band) => {
    const el = scroller();
    const top = viewTop(el);
    const anchor = atTop() ? null : rowsOf(list).find((li) => li.getBoundingClientRect().bottom > top);
    const from = anchor?.getBoundingClientRect().top;
    band();
    if (filters.innerHTML !== nextFilters.innerHTML) filters.replaceChildren(...nextFilters.childNodes);
    if (next) {
      const open = new Set([...list.querySelectorAll('details[open]')].map((d) => d.closest('li').dataset.id));
      list.replaceChildren(...next.childNodes);
      for (const li of rowsOf(list)) {
        const details = li.querySelector('details');
        if (details && open.has(li.dataset.id)) details.open = true;
      }
    }
    const same = anchor && rowsOf(list).find((li) => li.dataset.id === anchor.dataset.id);
    if (same) el.scrollTop += same.getBoundingClientRect().top - from;
  };
  const pull = async () => {
    if (pulling) {
      again = true;
      return;
    }
    pulling = true;
    do {
      again = false;
      try {
        const response = await fetch(fresh.href, { cache: 'no-store' });
        if (!response.ok) continue;
        const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
        const next = doc.getElementById('list');
        const nextFilters = doc.getElementById('filters');
        if (!next || !nextFilters) continue;
        // まだ届いていない数（操作の帯の釦と tab の題）は、サーバーが組んだ物に合わせる。新着の印（text）があれば、一緒に出す。
        const band = (text) => () => {
          const nextCounter = doc.getElementById('undelivered');
          if (nextCounter) {
            counter.hidden = nextCounter.hidden;
            counter.textContent = nextCounter.textContent;
          }
          if (text) show(text);
        };
        document.title = doc.title || document.title;
        if (paged) {
          swap(null, nextFilters, band('● 新しい記録（読み直す）'));
          continue;
        }
        const had = new Set(rowsOf(list).map((li) => li.dataset.id));
        const added = rowsOf(next).filter((li) => !had.has(li.dataset.id)).length;
        const down = added && !atTop();
        if (down) above += added;
        swap(shape(list) !== shape(next) ? next : null, nextFilters, band(down ? `● 新着 ${above} 件（上へ）` : ''));
      } catch {
        // 読み直せなかった時は、今の画面のまま（次の知らせか、つなぎ直しで読み直す）。
      }
    } while (again);
    pulling = false;
  };
  const seenTop = () => {
    if (!paged && above && atTop()) {
      above = 0;
      fresh.hidden = true;
    }
  };
  const toTop = () => {
    const el = scroller();
    el.scrollTop += list.getBoundingClientRect().top - viewTop(el);
  };
  fresh.addEventListener('click', (e) => {
    if (paged) return;
    e.preventDefault();
    toTop();
    seenTop();
    // 狭い幅では、印が消えて操作の帯が 1 段に戻ると、一覧の列が上へ動く。もう 1 度、頭へ送る。
    toTop();
  });
  box.addEventListener('scroll', seenTop, { passive: true });
  window.addEventListener('scroll', seenTop, { passive: true });

  // 知らせにつなぐ。pagetray か callboard を起こし直すと、つなぎ直す。切れていた間に書かれた物も入れる。
  // 流れが切れただけなら、EventSource が自分でつなぎ直す。つなぎ直した先が流れでない答え（pagetray が止まっている間の 503）だと、
  // EventSource はそこでやめる（readyState が CLOSED）ので、少し待って、新しくつなぐ。
  // つながるたびに読み直す。初めての時も（画面を組んでから流れが開くまでの間に置かれた記録を入れる。中身が同じなら、一覧の列は替えない）。
  const RETRY_MS = 3000;
  const connect = () => {
    const events = new EventSource(root.dataset.events);
    events.onopen = () => pull();
    events.onerror = () => {
      if (events.readyState === EventSource.CLOSED) setTimeout(connect, RETRY_MS);
    };
    events.addEventListener('message-record', () => pull());
  };
  connect();
})();
