// ページの一覧（/pages/）の script。callboard が /static/pages.js で配る（CSP の script-src 'self'）。
// pagetray の templates/index.html の script を移した物。
// - script が無くても一覧は動く。ここでするのは、新着と返事の状態での一覧の列の差し替えと印・未読の印と、Key の画面で
//   file を上げること。
// - 知らせは、.pages の data-events（pagetray の新着の知らせを callboard がそのまま流す道。connect-src 'self'）から聞く。
// - 差し替える中身は、同じ URL を callboard から読み直した HTML（サーバーが組んで、字を逃がした物）の節。字を HTML として
//   組み立てることはしない（innerHTML に書かない）。
(() => {
  'use strict';

  const root = document.querySelector('.pages');
  const fresh = document.getElementById('fresh');
  const list = document.getElementById('list');
  const filters = document.getElementById('filters');
  if (!root || !fresh || !list || !filters) return;

  // 未読: 初めて開いた時より後に置かれ、まだ本文の枠に出していないページ。browser が覚える（callboard の origin ごと。
  // pagetray の画面で覚えた印は、origin が違うので引き継がない）。
  let markUnread = () => {};
  try {
    const store = localStorage;
    const since = store.getItem('callboard.pages.since') || new Date().toISOString();
    store.setItem('callboard.pages.since', since);
    const seen = new Set(JSON.parse(store.getItem('callboard.pages.seen') || '[]'));
    const viewer = document.body.dataset.viewer;
    if (viewer && !seen.has(viewer)) {
      seen.add(viewer);
      store.setItem('callboard.pages.seen', JSON.stringify([...seen].slice(-5000)));
    }
    markUnread = (node) => {
      for (const li of node.querySelectorAll('li[data-path]')) {
        if (li.dataset.at > since && !seen.has(li.dataset.path)) li.classList.add('unread');
      }
    };
  } catch {
    // browser が覚える所を使えない時は、未読の印を出さない。
  }
  markUnread(document);

  // 新着: 置かれるたびに pagetray が知らせる。一覧の列と絞り込みの列だけを、サーバーが組んだ HTML に差し替える
  // （本文の枠は読み込み直さない。読んでいる所を動かさない）。数えるのは、今の絞り込みで一覧に入ったページだけ。
  // 古い方の続き（URL に at がある）を見ている時は差し替えず、押すと読み直す印を出す。
  const box = list.parentElement;
  const counters = ['waiting', 'review-waiting', 'escalated'].map((id) => document.getElementById(id)).filter(Boolean);
  let title = document.title;
  const paged = new URLSearchParams(location.search).has('at');
  const heard = new Set();
  let count = 0;
  let above = 0;
  let pulling = false;
  let again = false;
  // 一覧の列を送る物: 幅が広い時は真ん中の列、狭い時は画面全体。
  const scroller = () => (box.scrollHeight > box.clientHeight ? box : document.scrollingElement);
  const viewTop = (el) => (el === box ? box.getBoundingClientRect().top : 0);
  const atTop = () => list.getBoundingClientRect().top >= viewTop(scroller()) - 1;
  const rowsOf = (node) => [...node.querySelectorAll('li[data-path]')];
  // 行の並びと、行ごとの返事の状態・判定・見た物とレビューの link。どれかが変われば、一覧の列を差し替える。
  const marks = (li) => [li.dataset.reply, li.dataset.on, li.dataset.verdict, ...[...li.querySelectorAll('.links')].map((l) => l.textContent)].join(' ');
  const shape = (node) => rowsOf(node).map((li) => `${li.dataset.path} ${marks(li)}`).join('\n') + '\n' + (node.querySelector('.more')?.getAttribute('href') ?? '');
  const show = (text) => {
    fresh.hidden = false;
    fresh.textContent = text;
  };
  // 一覧の列を下へ送っていた時は、差し替える前に見えていた頭の行を、差し替えた後も同じ所に置く
  // （絞り込みの列の高さが変わる、狭い幅の時も）。頭にいた時は、そのまま（新しい行が頭に見える）。
  // 操作の帯の釦を替える band も、この中で呼ぶ（狭い幅では、釦が増えて帯が 2 段になると、下の行がみな動くため）。
  const swap = (next, nextFilters, band) => {
    const el = scroller();
    const top = viewTop(el);
    const anchor = atTop() ? null : rowsOf(list).find((li) => li.getBoundingClientRect().bottom > top);
    const from = anchor?.getBoundingClientRect().top;
    const focused = document.activeElement?.closest?.('#list li[data-path]')?.dataset.path;
    band();
    if (filters.innerHTML !== nextFilters.innerHTML) filters.replaceChildren(...nextFilters.childNodes);
    if (next) {
      list.replaceChildren(...next.childNodes);
      markUnread(list);
    }
    const same = anchor && rowsOf(list).find((li) => li.dataset.path === anchor.dataset.path);
    if (same) el.scrollTop += same.getBoundingClientRect().top - from;
    if (focused) rowsOf(list).find((li) => li.dataset.path === focused)?.querySelector('a')?.focus({ preventScroll: true });
  };
  const pull = async () => {
    if (pulling) {
      again = true;
      return;
    }
    pulling = true;
    do {
      again = false;
      // 読む前に来ていた知らせだけを、この回で確かめる（読んでいる間に来た物は、次の回）。
      const asked = [...heard];
      heard.clear();
      try {
        const response = await fetch(fresh.href, { cache: 'no-store' });
        if (!response.ok) continue;
        const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
        // 読み直した木は、比べと差し替えの前に今の言語の字にする（英語の画面が日本語に戻らない。題も訳される）。
        window.callboardLocalize(doc);
        const next = doc.getElementById('list');
        const nextFilters = doc.getElementById('filters');
        if (!next || !nextFilters) continue;
        // 待ちの数（操作の帯の釦と tab の題）は、サーバーが組んだ物に合わせる。新着の印（text）があれば、一緒に出す。
        const band = (text) => () => {
          for (const counter of counters) {
            const nextCounter = doc.getElementById(counter.id);
            if (nextCounter) {
              counter.hidden = nextCounter.hidden;
              // 数の字は中の <span>（鍵と値を持つ）にあるので、textContent でなく子ごと写す（訳した木から）。
              counter.replaceChildren(...nextCounter.childNodes);
            }
          }
          if (text) show(text);
        };
        title = doc.title || title;
        const arrived = new Set(rowsOf(next).map((li) => li.dataset.path));
        const added = asked.filter((path) => arrived.has(path)).length;
        count += added;
        document.title = count ? `(${count}) ${title}` : title;
        if (paged) {
          swap(null, nextFilters, band(count ? window.callboardT('pages.freshPaged', { count }) : ''));
          continue;
        }
        const down = added && !atTop();
        if (down) above += added;
        swap(shape(list) !== shape(next) ? next : null, nextFilters, band(down ? window.callboardT('common.tray.freshAbove', { count: above }) : ''));
      } catch {
        // 読み直せなかった時は、今の画面のまま（次の知らせか、つなぎ直しで読み直す）。
      }
    } while (again);
    pulling = false;
  };
  const toTop = () => {
    const el = scroller();
    el.scrollTop += list.getBoundingClientRect().top - viewTop(el);
  };
  const seenTop = () => {
    if (!paged && above && atTop()) {
      above = 0;
      fresh.hidden = true;
    }
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

  // 上げる枠（Key の画面だけ）。file の枠を読み直す関数は、知らせの受け口より先に決めておく。
  const upload = document.getElementById('upload');
  const shelf = document.getElementById('files');
  const refresh = async () => {
    // file の枠は、サーバーが組んだ HTML を読み直して差し替える（行の作りを 2 か所に持たない）。
    const html = await (await fetch(location.href, { cache: 'no-store' })).text();
    const doc = new DOMParser().parseFromString(html, 'text/html');
    window.callboardLocalize(doc);
    const next = doc.getElementById('files');
    if (next && shelf) shelf.replaceChildren(...next.childNodes);
  };

  // 知らせにつなぐ。pagetray か callboard を起こし直すと、つなぎ直す。つながるたびに読み直す（切れていた間に置かれた物も
  // 入れる。数には入らず、未読の印で見える）。つなぎ直した先が流れでない答え（pagetray が止まっている間の 503）だと、
  // EventSource はそこでやめる（readyState が CLOSED）ので、少し待って、新しくつなぐ。
  const RETRY_MS = 3000;
  const connect = () => {
    const events = new EventSource(root.dataset.events);
    events.onopen = () => pull();
    events.onerror = () => {
      if (events.readyState === EventSource.CLOSED) setTimeout(connect, RETRY_MS);
    };
    // 名の無い知らせ: ページが置かれた。
    events.onmessage = (e) => {
      try {
        const path = JSON.parse(e.data).path;
        if (path) heard.add(path);
      } catch {
        // 読めない知らせでも、読み直しはする。
      }
      pull();
    };
    // 返事の状態（返事待ち・答えた）やレビューの印（判定・見た物）が書かれたら、印と数を新しくする。新着には数えない。
    events.addEventListener('reply', () => pull());
    events.addEventListener('review', () => pull());
    // ほかの所（CLI など）から、この Key に file が置かれたら、file の枠を読み直す。
    events.addEventListener('file', (e) => {
      if (!upload || !shelf) return;
      try {
        const file = JSON.parse(e.data);
        if (file.project === upload.dataset.project && file.key === upload.dataset.key) refresh();
      } catch {
        // 読めない知らせは、読まない。
      }
    });
  };
  connect();

  // 上げる枠。送る前に、拡張子・大きさを見て、見本を出し、秘密が写っていないか見た印を求める。
  if (!upload || !shelf) return;
  // 送る先の道は、callboard が data-upload で渡す（callboard が受けて、pagetray に置く）。名は name の引数で足す。
  const target = new URL(upload.dataset.upload, location.href);
  const types = new Map(JSON.parse(upload.dataset.types));
  const max = Number(upload.dataset.max);
  const form = upload.querySelector('form');
  const drop = form.querySelector('.drop');
  const input = form.querySelector('input[type=file]');
  const preview = form.querySelector('.preview');
  const reviewed = form.querySelector('input[name=reviewed]');
  const send = form.querySelector('button');
  const say = form.querySelector('.say');
  form.hidden = false;
  upload.querySelector('.nojs').remove();
  let chosen = null;
  let shown = null;

  const size = (n) => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${(n / 1024 / 1024).toFixed(1)} MiB`);
  const tell = (text, bad) => {
    say.textContent = text;
    say.classList.toggle('bad', !!bad);
  };
  // 名の語は pagetray の CLI と同じ作り方: 英小文字にし、英小文字・数字・- _ のほかが続く所を - 1 つにし、
  // 頭の - _ と尻の - を除いて 64 字に切る。字が残らなければ file。
  const stemOf = (base) => {
    let out = '';
    for (const c of base.replace(/[A-Z]/g, (x) => x.toLowerCase())) {
      if (/[a-z0-9_-]/.test(c)) out += c;
      else if (!out.endsWith('-')) out += '-';
    }
    out = [...out.replace(/^[-_]+/, '')].slice(0, 64).join('').replace(/-+$/, '');
    return out || 'file';
  };
  const reset = () => {
    chosen = null;
    reviewed.checked = false;
    send.disabled = true;
    preview.hidden = true;
    preview.replaceChildren();
    if (shown) URL.revokeObjectURL(shown);
    shown = null;
  };
  const choose = async (file) => {
    reset();
    tell('');
    if (!file) return;
    const dot = file.name.lastIndexOf('.');
    const ext = dot > 0 ? file.name.slice(dot + 1).replace(/[A-Z]/g, (x) => x.toLowerCase()) : '';
    if (!types.has(ext)) {
      tell(window.callboardT('pages.upload.js.badExt', { ext: ext ? `.${ext}` : window.callboardT('pages.upload.js.noExt'), types: [...types.keys()].join(window.callboardT('common.listSep')) }), true);
      return;
    }
    if (file.size > max) {
      tell(window.callboardT('pages.upload.js.tooBig', { size: size(file.size), max: size(max) }), true);
      return;
    }
    if (file.size === 0) {
      tell(window.callboardT('pages.upload.js.empty'), true);
      return;
    }
    const type = types.get(ext);
    const name = `${stemOf(file.name.slice(0, dot))}.${ext}`;
    const head = document.createElement('p');
    head.className = 'say';
    head.textContent = `${file.name}（${size(file.size)}）→ ${name}`;
    preview.append(head);
    if (type.startsWith('image/')) {
      const img = document.createElement('img');
      shown = URL.createObjectURL(file);
      img.src = shown;
      img.alt = window.callboardT('pages.upload.js.previewAlt');
      preview.append(img);
    } else if (type !== 'application/pdf') {
      const pre = document.createElement('pre');
      const text = await file.slice(0, 64 * 1024).text();
      pre.textContent = text.split('\n').slice(0, 20).join('\n');
      preview.append(pre);
    }
    preview.hidden = false;
    chosen = { file, name, type };
  };

  input.addEventListener('change', () => choose(input.files[0]));
  reviewed.addEventListener('change', () => {
    send.disabled = !(chosen && reviewed.checked);
  });
  drop.addEventListener('dragover', (e) => {
    e.preventDefault();
    drop.classList.add('over');
  });
  drop.addEventListener('dragleave', () => drop.classList.remove('over'));
  drop.addEventListener('drop', (e) => {
    e.preventDefault();
    drop.classList.remove('over');
    input.files = e.dataTransfer.files;
    choose(input.files[0]);
  });
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (!chosen || !reviewed.checked) return;
    const { file, name, type } = chosen;
    send.disabled = true;
    tell(window.callboardT('pages.upload.js.sending'));
    try {
      target.searchParams.set('name', name);
      const response = await fetch(target, {
        method: 'POST',
        headers: { 'Content-Type': type },
        body: file,
      });
      const json = await response.json().catch(() => ({}));
      if (!response.ok) {
        // 断った訳は message に入る。code（字の鍵）と values があれば、選んだ言語の字で組む（日本語の字は message と同じ）。
        const why = json.code
          ? window.callboardT('pages.upload.failed', { status: response.status, why: window.callboardT(json.code, json.values) })
          : window.callboardT('pages.upload.failed', { status: response.status, why: json.message || json.error || response.statusText });
        if (response.status === 503) {
          // 聞けなかった時（時間切れなど）は、置かれたかもしれない。file の枠を読み直して、送り直す前に確かめられるようにする。
          await refresh().catch(() => {});
          tell(window.callboardT('pages.upload.js.check', { why }), true);
        } else {
          tell(why, true);
        }
        send.disabled = false;
        return;
      }
      reset();
      input.value = '';
      say.replaceChildren(window.callboardT('pages.upload.js.doneBefore'), Object.assign(document.createElement('code'), { textContent: `files/${json.name}-${json.n}.${json.ext}` }), window.callboardT('pages.upload.js.doneAfter'));
      say.classList.remove('bad');
      await refresh();
    } catch (error) {
      tell(window.callboardT('pages.upload.js.error', { error }), true);
      send.disabled = false;
    }
  });
})();
