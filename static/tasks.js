// 課題の筋（/messages/tasks/<Key>）の、新着の知らせ。callboard が /static/tasks.js で配る（CSP の script-src 'self'）。
// - script が無くても画面は動く。ここでするのは、この課題の記録が置かれた・書き足された時に、押すと読み直す印（#fresh）を出すことだけ
//   （棒や行は差し替えない。読んでいる途中で、時間の軸が動かないように）。
// - この課題の記録かは、知らせの中身（記録の JSON）の key の、点の前の字が、.tray の data-key と同じかで見る（ほかの課題の記録では出さない）。
//   中身を読めない知らせでは、出さない。
// - 知らせは、.tray の data-events（pagetray の新着の知らせを callboard がそのまま流す道。connect-src 'self'）から聞く。
(() => {
  'use strict';

  const root = document.querySelector('.tray[data-events]');
  const fresh = document.getElementById('fresh');
  if (!root || !fresh) return;

  // 流れが切れただけなら、EventSource が自分でつなぎ直す。つなぎ直した先が流れでない答え（pagetray が止まっている間の 503）だと、
  // EventSource はそこでやめる（readyState が CLOSED）ので、少し待って、新しくつなぐ。
  const RETRY_MS = 3000;
  const connect = () => {
    const events = new EventSource(root.dataset.events);
    events.onerror = () => {
      if (events.readyState === EventSource.CLOSED) setTimeout(connect, RETRY_MS);
    };
    events.addEventListener('message-record', (e) => {
      let key;
      try {
        key = JSON.parse(e.data).key;
      } catch {
        return;
      }
      if (typeof key === 'string' && key.split('.')[0] === root.dataset.key) fresh.hidden = false;
    });
  };
  connect();
})();
