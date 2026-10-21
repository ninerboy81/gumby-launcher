// The launcher window. Everything comes from the core through one command: call(op, p).
const invoke = (op, p = {}) => (window.__TAURI__ ? window.__TAURI__.core.invoke('call', { op, p }) : window.previewInvoke(op, p));
const openSignin = (region) => (window.__TAURI__ ? window.__TAURI__.core.invoke('signin', { region }) : window.previewInvoke('signin', { region }));

const ICON = {
  play: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M3 1.5v11l9.5-5.5z" fill="currentColor"/></svg>',
  gear: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>',
  pen: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>',
  tick: '<svg class="tick" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><path d="M5 12.5l4.5 4.5L19 7"/></svg>',
  plus: '<svg width="13" height="13" viewBox="0 0 14 14"><path d="M7 1v12M1 7h12" stroke="currentColor" stroke-width="2"/></svg>',
};
const QUARTER = ['top left', 'top right', 'bottom left', 'bottom right'];
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

let S = null;          // the last state from the core
let dialog = null;     // what the open dialog is, so polling does not redraw over typing

// ------------------------------------------------------------------------------------------------ toast
let toastTimer = 0;
function toast(text, good = false) {
  const t = $('toast');
  t.textContent = text;
  t.className = 'toast on' + (good ? ' good' : '');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.className = 'toast'), good ? 2500 : 6000);
}

// ------------------------------------------------------------------------------------------------ drawing
const acct = (id) => S.accounts.find((a) => a.id === id);
const openOf = (id) => S.open.find((o) => o.account === id);

function screenMap(cells, cls = '') {
  if (cells.length === 1) return `<div class="screen one ${cls}" aria-hidden="true"><i class="${cells[0].cls || 'fill'}">${cells[0].n}</i></div>`;
  const out = [];
  for (let i = 0; i < 4; i++) {
    const c = cells[i];
    out.push(c ? `<i class="${c.cls || 'fill'}">${c.n}</i>` : '<i></i>');
  }
  return `<div class="screen ${cls}" aria-hidden="true">${out.join('')}</div>`;
}

function stepRow(s, i) {
  const mark = s.state === 'done' ? ICON.tick : s.state === 'starting' || s.state === 'settling' ? '<span class="spin"></span>'
    : s.state === 'failed' ? '<span class="cross">✕</span>' : '<span class="wait"></span>';
  const cls = s.state === 'done' ? 'done' : s.state === 'starting' || s.state === 'settling' ? 'now' : s.state;
  let note = s.note || (s.state === 'waiting' ? 'Next' : '');
  const o = openOf(s.account);
  if (s.state === 'done' && o && o.quarter != null) note = 'Open, ' + QUARTER[o.quarter];
  return `<div class="step ${cls}">${mark}<span>${i + 1} · ${esc(s.name)}</span><span class="st">${esc(note)}</span></div>`;
}

function drawGroups() {
  const L = S.launch;
  const box = $('groups');
  if (!S.groups.length) {
    box.innerHTML = `<div class="empty"><div><b>No groups yet</b>${S.accounts.length ? 'Make a group from your accounts, then open all of them with one click.' : 'Add your accounts first, then make a group to open several of them with one click.'}</div></div>`;
    return;
  }
  box.innerHTML = S.groups.map((g) => {
    const mine = L.group === g.id && L.steps.length;
    const live = mine && L.busy;
    const names = g.accounts.map((id) => acct(id)?.name ?? '?');
    const cells = g.accounts.map((id, i) => {
      if (!mine) return { n: i + 1 };
      const st = L.steps[i]?.state;
      return { n: i + 1, cls: st === 'done' ? 'fill' : st === 'starting' || st === 'settling' ? 'live' : '' };
    });
    const body = mine ? `<div class="steps">${L.steps.map(stepRow).join('')}</div>`
      : `<div class="chips">${names.map((n, i) => `<span class="chip"><b>${i + 1}</b>${esc(n)}</span>`).join('')}</div>`;
    const opened = g.accounts.filter((id) => openOf(id)).length;
    const launchBtn = `<button class="btn primary big" data-act="launch" data-id="${g.id}" ${L.busy || opened ? 'disabled' : ''}>${ICON.play}${g.accounts.length > 1 ? 'Launch all' : 'Launch'}</button>`;
    const acts = live ? '<button class="btn quiet" data-act="stop">Stop</button>'
      : `${opened ? `<div class="stack">${launchBtn}<button class="btn closeall" data-act="closeall" data-id="${g.id}">${opened > 1 ? `Close all ${opened}` : 'Close game'}</button></div>` : launchBtn}
         <button class="iconbtn" data-act="editgroup" data-id="${g.id}" title="Edit group" aria-label="Edit ${esc(g.name)}">${ICON.pen}</button>`;
    const n = g.accounts.length;
    return `<div class="group"><div><div class="name">${esc(g.name)}</div>
      <div class="sub">${n} game${n === 1 ? '' : 's'}${n > 1 ? ' · opens in quarters of your screen' : ''}</div>${body}</div>
      ${screenMap(cells)}<div class="acts">${acts}</div></div>`;
  }).join('');
}

function drawAccounts() {
  const L = S.launch;
  const box = $('accounts');
  if (!S.accounts.length) {
    box.innerHTML = `<div class="empty"><div><b>Add your first account</b>Save a Battle.net login once. After that one click opens the game, already signed in.</div>
      <button class="btn primary" data-act="addacct">${ICON.plus}Add account</button></div>`;
    return;
  }
  box.innerHTML = S.accounts.map((a) => {
    const o = openOf(a.id);
    const step = !L.group && L.steps.find((s) => s.account === a.id && ['starting', 'settling', 'waiting'].includes(s.state));
    const right = step ? `<span class="runtag" style="color:var(--blue-glow)"><span class="spin"></span>${esc(step.note || 'Starting…')}</span>`
      : o ? `<span class="runtag"><span class="dot on"></span>Running${o.quarter != null && S.running > 1 ? `<span class="q">${QUARTER[o.quarter]}</span>` : ''}</span>` : '<span></span>';
    return `<div class="acct"><button class="play ${L.busy ? 'off' : ''}" data-act="play" data-id="${a.id}" title="Open ${esc(a.name)}" aria-label="Open ${esc(a.name)}">${ICON.play}</button>
      <div><div class="who">${esc(a.name)}</div><div class="mail">${a.method === 'token' ? 'Battle.net Authenticator' : esc(a.email)}</div></div>
      ${right}<span class="badge">${esc(a.tag)}</span>
      <button class="iconbtn" data-act="editacct" data-id="${a.id}" title="Edit account" aria-label="Edit ${esc(a.name)}">${ICON.pen}</button></div>`;
  }).join('');
}

function drawStatus() {
  const L = S.launch;
  const now = L.steps.findIndex((s) => ['starting', 'settling'].includes(s.state));
  let text, on = S.running > 0;
  if (L.busy && L.label) text = `Opening ${esc(L.label)} · ${Math.max(now, 0) + 1} of ${L.steps.length}`;
  else if (L.busy) text = `Opening ${esc(L.steps[0]?.name)}`;
  else text = S.running ? `${S.running} game${S.running === 1 ? '' : 's'} running` : 'No games running';
  $('status').innerHTML = `<span class="dot ${on || L.busy ? 'on' : ''}"></span>${text}`;
  $('newgroup').disabled = !S.accounts.length;
  $('addacct').disabled = S.accounts.length >= S.max_accounts;
  $('addacct').style.visibility = S.accounts.length ? '' : 'hidden';   // the empty list has its own big button
}

function clock(at) {
  const left = Math.max(0, Math.round(at - Date.now() / 1000));
  return `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
}
function drawZones() {
  const z = S.zones, box = $('zones');
  if (!z) { box.innerHTML = ''; return; }
  if (z.error || !z.now) {
    box.innerHTML = `<div class="glass tz quiet">${z.error ? esc(z.error) : 'Terror zones: waiting for the next update'}</div>`;
    return;
  }
  box.innerHTML = `<div class="glass tz"><div><div class="k"><span class="live"></span>Terror zone now</div><div class="z">${esc(z.now)}</div>
      ${z.now_act ? `<div class="act">Act ${z.now_act}</div>` : ''}</div>
    <div><div class="k">Next</div><div class="z next">${z.next ? esc(z.next) : 'Not known yet'}</div></div>
    <div class="clock">${z.switch_at ? `<b id="tzclock">${clock(z.switch_at)}</b><span>until it changes</span>` : ''}</div></div>`;
}
function drawUpdate() {
  const u = S.update, box = $('update');
  box.innerHTML = u && u.state === 'ready'
    ? `<div class="update"><span><b>Version ${esc(u.version)} is ready.</b> ${esc(u.notes || '')}</span><button class="btn" data-act="restart">Restart to update</button></div>` : '';
}

// a report waiting to be sent: ask once per report
let reportAsked = null, sendWatch = null;
function watchReport() {
  const r = S.report;
  if (sendWatch) {
    if (r && r.id === sendWatch.id && r.sending) sendWatch.seen = true;
    else if (sendWatch.seen && (!r || r.id !== sendWatch.id)) { toast('Report sent. Thanks!', true); sendWatch = null; if (dialog === 'report') closeDialog(); }
    else if (sendWatch.seen && r && r.id === sendWatch.id && !r.sending) { toast('Could not send the report. Try again later.'); sendWatch = null; reportDialog(); }
    return;
  }
  if (r && r.id !== reportAsked && !dialog) { reportAsked = r.id; reportDialog(); }
}

let lastError = null;
function draw() {
  drawStatus();
  drawUpdate();
  drawZones();
  drawGroups();
  drawAccounts();
  watchReport();
  const err = S.launch.error;
  if (err && err !== lastError) toast(err);
  lastError = err;
}

async function refresh() {
  try { S = await invoke('state'); draw(); } catch (e) { toast(String(e)); }
}

// ------------------------------------------------------------------------------------------------ dialogs
function openDialog(kind, html, width = 460) {
  dialog = kind;
  $('modal').innerHTML = `<div class="dim" data-act="close"></div><div class="dialog" role="dialog" aria-modal="true" style="width:${width}px">${html}</div>`;
  const first = $('modal').querySelector('input');
  if (first) first.focus();
}
function closeDialog() { dialog = null; $('modal').innerHTML = ''; }

let acctForm = null;   // the open account dialog: {id, method, signin: idle|waiting|done, signedIn}
function accountDialog(id) {
  const a = id ? acct(id) : null;
  acctForm = { id: a?.id ?? '', method: a?.method ?? 'password', signin: 'idle', had: a?.method === 'token' };
  const region = a ? a.region : 'Americas';
  invoke('signin_reset').catch(() => {});
  openDialog('account', `<h3 class="title">${a ? 'Edit account' : 'Add account'}</h3>
    <p class="lead">Your Battle.net login. ${esc(S.app)} signs you in, so you never see the Battle.net app.</p>
    <label class="f" for="f-name">Name (shown on the button)</label><div class="input"><input id="f-name" maxlength="24" value="${esc(a?.name)}" placeholder="Main"></div>
    <label class="f">Region</label><div class="seg" id="f-region">${S.regions.map((r) => `<span data-act="region" data-r="${r}" class="${r === region ? 'on' : ''}">${r}</span>`).join('')}</div>
    <label class="f">How does this account sign in?</label>
    <div class="seg" id="f-method" style="grid-template-columns:1fr 1fr"><span data-act="method" data-m="password">Email + password</span><span data-act="method" data-m="token">Battle.net Authenticator</span></div>
    <div id="f-pwbox">
      <label class="f" for="f-email">Battle.net email</label><div class="input"><input id="f-email" maxlength="120" value="${esc(a?.email)}" placeholder="you@example.com" spellcheck="false"></div>
      <label class="f" for="f-pw">Password</label><div class="input"><input id="f-pw" type="password" maxlength="128" placeholder="${a && a.method !== 'token' ? 'Leave empty to keep the saved one' : ''}"><button class="eye" data-act="peek" type="button">Show</button></div>
      <div class="note"><span>🔒</span><span>The password is locked with Windows encryption for your Windows user. ${esc(S.app)} never shows it again, never logs it, never sends it anywhere.</span></div>
    </div>
    <div id="f-tokbox"></div>
    <div class="err" id="f-err"></div>
    <div class="foot">${a ? `<button class="btn ghost danger left" data-act="delacct" data-id="${a.id}">Delete</button>` : ''}
      <button class="btn ghost" data-act="close">Cancel</button><button class="btn primary" id="f-save" data-act="saveacct" data-id="${a?.id ?? ''}">Save account</button></div>`, 500);
  drawMethod();
}

// the sign-in part of the account dialog, redrawn as the method or the Battle.net sign-in changes
function drawMethod() {
  const f = acctForm;
  document.querySelectorAll('#f-method span').forEach((s) => s.classList.toggle('on', s.dataset.m === f.method));
  $('f-pwbox').style.display = f.method === 'password' ? '' : 'none';
  const signedIn = f.signin === 'done' || (f.had && f.signin === 'idle');
  $('f-tokbox').innerHTML = f.method !== 'token' ? '' : signedIn
    ? `<div class="note" style="background:rgba(63,214,166,.08);border-color:rgba(63,214,166,.35);color:#cdeee3"><span>✓</span><span><b>Signed in.</b> This account opens without a password. If you turn the Authenticator off and on again, sign in once more.</span></div>
       <div style="display:flex;justify-content:center;margin-top:12px"><button class="btn" data-act="signin">Sign in again</button></div>`
    : `<div class="note"><span>🛡</span><span>For accounts with the Authenticator (2-step). Sign in once on Battle.net's own page, code included. ${esc(S.app)} keeps the sign-in it gets back, locked with Windows encryption, and never sees your password.</span></div>
       <div style="display:flex;justify-content:center;margin-top:16px"><button class="btn primary big" data-act="signin" ${f.signin === 'waiting' ? 'disabled' : ''}>${f.signin === 'waiting' ? 'Waiting for Battle.net…' : 'Sign in with Battle.net'}</button></div>`;
  $('f-save').disabled = f.method === 'token' && !signedIn;
}

// while the Battle.net window is open, follow it
setInterval(async () => {
  if (dialog !== 'account' || !acctForm || acctForm.signin !== 'waiting') return;
  try {
    const st = (await invoke('signin_status')).state;
    if (st === 'done' || st === 'idle') { acctForm.signin = st; if (st === 'done') acctForm.had = true; drawMethod(); }
  } catch (e) { /* next round */ }
}, 600);

let picked = [];
function groupDialog(id) {
  const g = id ? S.groups.find((x) => x.id === id) : null;
  picked = g ? [...g.accounts] : [];
  openDialog('group', `<h3 class="title">${g ? 'Edit group' : 'New group'}</h3>
    <p class="lead">Tap accounts in the order they should open. Up to ${S.max_group}: one per quarter of the screen.</p>
    <label class="f" for="f-gname">Group name</label><div class="input"><input id="f-gname" maxlength="24" value="${esc(g?.name)}" placeholder="Farm team"></div>
    <div style="display:grid;grid-template-columns:1fr auto;gap:22px;align-items:start;margin-top:12px">
      <div><label class="f" style="margin-top:0">Accounts</label><div id="f-picks"></div></div>
      <div><label class="f" style="margin-top:0">Where each game opens</label><div id="f-map"></div><p id="f-where" style="color:var(--ash);font-size:12px;margin-top:14px;max-width:220px"></p></div>
    </div>
    <div class="err" id="f-err"></div>
    <div class="foot">${g ? `<button class="btn ghost danger left" data-act="delgroup" data-id="${g.id}">Delete</button>` : ''}
      <button class="btn ghost" data-act="close">Cancel</button><button class="btn primary" data-act="savegroup" data-id="${g?.id ?? ''}">${g ? 'Save group' : 'Create group'}</button></div>`, 580);
  drawPicks();
}
function drawPicks() {
  $('f-picks').innerHTML = S.accounts.map((a) => {
    const n = picked.indexOf(a.id) + 1;
    return `<button class="pick${n ? ' on' : ''}" data-act="pick" data-id="${a.id}"><span class="box">${n || ''}</span><span>${esc(a.name)}</span><span class="badge">${esc(a.tag)}</span></button>`;
  }).join('');
  $('f-map').innerHTML = screenMap(picked.map((_, i) => ({ n: i + 1 })), 'lg');
  $('f-where').textContent = picked.length > 1 ? picked.map((_, i) => `${i + 1} ${QUARTER[i]}`).join(', ') + '.'
    : picked.length === 1 ? 'One game opens as it always does.' : '';
}

function settingsDialog() {
  openDialog('settings', `<h3 class="title">Settings</h3>
    <p class="lead">Intro videos are always skipped. Two or more games always open in quarters of the screen.</p>
    <div class="switchrow"><div><h4 class="title">Open more than one game</h4>
      <p>D2R refuses to start a second copy while one is open. With this on, ${esc(S.app)} closes that one lock inside the running game just before it opens the next one, the same thing people do by hand in Process Explorer. Nothing else in the game is read or changed. Off: one game at a time.</p></div>
      <button class="sw ${S.allow_multi ? 'on' : ''}" data-act="multi" role="switch" aria-checked="${S.allow_multi}" aria-label="Open more than one game"></button></div>
    <div class="switchrow"><div style="grid-column:1/-1"><h4 class="title">Game</h4>
      ${S.game_folder ? `<p>Found by itself: ${esc(S.game_folder)}</p>`
        : `<p>Diablo II: Resurrected was not found on this PC. Install it with Battle.net, or paste the folder that holds D2R.exe.</p>
      <div style="display:flex;gap:8px;margin-top:8px"><div class="input" style="flex:1"><input id="f-folder" placeholder="D:\\Games\\Diablo II Resurrected" value=""></div>
      <button class="btn" data-act="setfolder">Use this folder</button></div>`}</div></div>
    <div class="switchrow about"><div><h4 class="title">${esc(S.app)} ${esc(S.version)}</h4><p>${aboutText()}</p></div>
      <div style="display:flex;gap:8px"><button class="btn" data-act="reportnew">Send a report</button><button class="btn" data-act="checknow">Check now</button></div></div>
    <div class="err" id="f-err"></div>
    <div class="foot"><button class="btn primary" data-act="close">Done</button></div>`, 620);
}

function reportDialog() {
  const r = S.report;
  if (!r) return;
  const kind = /-crash-/.test(r.id) ? 'crash' : /-launch-/.test(r.id) ? 'launch' : 'manual';
  const title = kind === 'crash' ? 'Something went wrong last time' : kind === 'launch' ? 'A launch stopped' : 'Send a report';
  const lead = kind === 'manual' ? 'This sends what the launcher did lately, so Gumby can look at it with you.'
    : `${esc(r.reason.replace(/^Launch stopped: /, ''))}. Send a report so Gumby can look at it?`;
  openDialog('report', `<h3 class="title">${title}</h3><p class="lead">${lead}</p>
    <label class="f" style="margin-top:6px">What gets sent</label><pre class="report">${esc(r.text)}</pre>
    <div class="note"><span>🔒</span><span>This is the whole report. Emails, account and group names and passwords are taken out before it leaves your PC. Nothing is sent unless you press Send.</span></div>
    <div class="foot"><button class="btn ghost" data-act="reportno" data-id="${esc(r.id)}">Not now</button>
      <button class="btn primary" data-act="reportsend" data-id="${esc(r.id)}" ${r.sending ? 'disabled' : ''}>${r.sending ? 'Sending…' : 'Send report'}</button></div>`, 560);
}

function aboutText() {
  const u = S.update || {};
  const ago = (t) => { const m = Math.round((Date.now() / 1000 - t) / 60); return m < 1 ? 'just now' : m < 60 ? `${m} minute${m === 1 ? '' : 's'} ago` : `${Math.round(m / 60)} h ago`; };
  if (u.state === 'checking') return 'Checking for updates…';
  if (u.state === 'ready') return `Version ${esc(u.version)} is ready: restart to update.`;
  if (u.state === 'error') return `${esc(u.error)}.`;
  if (u.state === 'current') return `Up to date · checked ${ago(u.checked_at)}.`;
  return 'Updates download by themselves; you choose when to restart.';
}

function multiDialog(n, run) {
  const L = pendingLaunch;
  const names = L.names;
  const cells = Array.from({ length: Math.min(n, 4) }, (_, i) => ({ n: i + 1 }));
  openDialog('multi', `<h3 class="title">Open ${n} games at once?</h3>
    <p class="lead" style="margin-bottom:6px">To open more than one game, ${esc(S.app)} has to close D2R's "only one copy" lock in each running game. It changes nothing else in the game.</p>
    <div style="display:flex;gap:18px;align-items:center;margin:14px 0 4px">${screenMap(cells)}<p style="color:var(--ash);font-size:12.5px">${esc(names)} ${S.running ? 'joins the game already open' : 'open one after another'}, each in its own quarter of the screen.</p></div>
    <div class="note"><span>ⓘ</span><span>You can turn this off again any time in Settings.</span></div>
    <div class="foot"><button class="btn ghost" data-act="close">Not now</button><button class="btn primary" data-act="multigo">Turn on and launch</button></div>`, 500);
}

// ------------------------------------------------------------------------------------------------ actions
let pendingLaunch = null;
async function launch(op, id, names) {
  try {
    await invoke(op, { id });
  } catch (e) {
    const m = /^needs_multi:(\d+)$/.exec(String(e));
    if (m) { pendingLaunch = { op, id, names }; multiDialog(+m[1]); return; }
    toast(String(e));
  }
  refresh();
}

function formError(e) { const el = $('f-err'); if (el) el.textContent = String(e); }

document.addEventListener('click', async (ev) => {
  const el = ev.target.closest('[data-act]');
  if (!el || el.disabled) return;
  const id = el.dataset.id;
  switch (el.dataset.act) {
    case 'close': closeDialog(); break;
    case 'addacct': accountDialog(null); break;
    case 'editacct': accountDialog(id); break;
    case 'newgroup': groupDialog(null); break;
    case 'editgroup': groupDialog(id); break;
    case 'gear': settingsDialog(); break;
    case 'peek': { const pw = $('f-pw'); pw.type = pw.type === 'password' ? 'text' : 'password'; el.textContent = pw.type === 'password' ? 'Show' : 'Hide'; break; }
    case 'region': document.querySelectorAll('#f-region span').forEach((s) => s.classList.toggle('on', s === el)); break;
    case 'pick': {
      const i = picked.indexOf(id);
      if (i >= 0) picked.splice(i, 1); else if (picked.length < S.max_group) picked.push(id); else { formError(`A group opens up to ${S.max_group} games`); break; }
      formError(''); drawPicks(); break;
    }
    case 'method': acctForm.method = el.dataset.m; formError(''); drawMethod(); break;
    case 'signin': {
      const region = document.querySelector('#f-region span.on')?.dataset.r;
      try { await openSignin(region); acctForm.signin = 'waiting'; acctForm.had = false; drawMethod(); } catch (e) { formError(e); }
      break;
    }
    case 'saveacct': {
      const region = document.querySelector('#f-region span.on')?.dataset.r;
      try {
        await invoke('account_save', { id: id || '', name: $('f-name').value, email: $('f-email').value, password: $('f-pw').value, region, method: acctForm.method });
        $('f-pw').value = '';
        closeDialog(); toast('Account saved', true); refresh();
      } catch (e) { formError(e); }
      break;
    }
    case 'delacct':
      if (el.dataset.sure !== '1') { el.dataset.sure = '1'; el.textContent = 'Delete it?'; break; }
      try { await invoke('account_remove', { id }); closeDialog(); refresh(); } catch (e) { formError(e); }
      break;
    case 'savegroup':
      try { await invoke('group_save', { id: id || '', name: $('f-gname').value, accounts: picked }); closeDialog(); toast('Group saved', true); refresh(); } catch (e) { formError(e); }
      break;
    case 'delgroup':
      if (el.dataset.sure !== '1') { el.dataset.sure = '1'; el.textContent = 'Delete it?'; break; }
      try { await invoke('group_remove', { id }); closeDialog(); refresh(); } catch (e) { formError(e); }
      break;
    case 'play': if (!S.launch.busy) launch('play', id, acct(id)?.name ?? ''); break;
    case 'launch': {
      const g = S.groups.find((x) => x.id === id);
      const names = g.accounts.map((a) => acct(a)?.name).filter(Boolean);
      launch('launch_group', id, names.length > 1 ? names.slice(0, -1).join(', ') + ' and ' + names.at(-1) : names[0]);
      break;
    }
    case 'stop': await invoke('stop'); refresh(); break;
    case 'closeall':
      if (el.dataset.sure !== '1') { el.dataset.sure = '1'; el.textContent = 'Close them?'; setTimeout(() => { if (el.isConnected) { el.dataset.sure = ''; refresh(); } }, 4000); break; }
      try { const n = await invoke('close_group', { id }); toast(`Closing ${n} game${n === 1 ? '' : 's'}`, true); } catch (e) { toast(String(e)); }
      refresh(); break;
    case 'reportsend':
      try { await invoke('report_send', { id }); sendWatch = { id, seen: false }; el.disabled = true; el.textContent = 'Sending…'; } catch (e) { toast(String(e)); }
      break;
    case 'reportno': try { await invoke('report_dismiss', { id }); } catch (e) { /* kept for next time */ } closeDialog(); refresh(); break;
    case 'reportnew':
      try { await invoke('report_new'); closeDialog(); reportAsked = null; await refresh(); } catch (e) { formError(e); }
      break;
    case 'checknow': try { await invoke('update_check'); await refresh(); settingsDialog(); } catch (e) { formError(e); } break;
    case 'restart': try { await invoke('update_apply'); } catch (e) { toast(String(e)); } break;
    case 'multigo': {
      const p = pendingLaunch; closeDialog();
      try { await invoke(p.op, { id: p.id, multi: true }); } catch (e) { toast(String(e)); }
      refresh(); break;
    }
    case 'multi':
      try { await invoke('set_multi', { on: !S.allow_multi }); await refresh(); settingsDialog(); } catch (e) { formError(e); }
      break;
    case 'setfolder':
      try { await invoke('set_folder', { folder: $('f-folder').value }); await refresh(); settingsDialog(); toast('Game folder saved', true); } catch (e) { formError(e); }
      break;
  }
});
document.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && dialog) closeDialog();
  if (ev.key === 'Enter' && dialog && ev.target.tagName === 'INPUT') {
    const go = $('modal').querySelector('[data-act=saveacct],[data-act=savegroup],[data-act=setfolder]');
    if (go) go.click();
  }
});

$('gear').innerHTML = ICON.gear;
$('gear').dataset.act = 'gear';
$('newgroup').innerHTML = ICON.plus + 'New group';
$('newgroup').dataset.act = 'newgroup';
$('addacct').innerHTML = ICON.plus + 'Add account';
$('addacct').dataset.act = 'addacct';
refresh();
setInterval(refresh, 700);
setInterval(() => { const c = $('tzclock'); if (c && S && S.zones && S.zones.switch_at) c.textContent = clock(S.zones.switch_at); }, 1000);
