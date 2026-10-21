// A made-up core for screenshots of the real page in a plain browser (no Tauri, no games).
(() => {
  const s = new URLSearchParams(location.search).get('s') || 'home';
  const accounts = s === 'empty' ? [] : [
    { id: 'a1', name: 'Main Barb', email: 'barb@example.com', region: 'Americas', tag: 'US' },
    { id: 'a2', name: 'Mule Sorc', email: '', region: 'Americas', tag: 'US', method: 'token' },
    { id: 'a3', name: 'Pally Alt', email: 'pally@example.com', region: 'Americas', tag: 'US' },
  ];
  const groups = s === 'empty' ? [] : [
    { id: 'g1', name: 'Farm team', accounts: ['a1', 'a2', 'a3'] },
    { id: 'g2', name: 'Solo run', accounts: ['a1'] },
    { id: 'g3', name: 'Cow runs', accounts: ['a2', 'a3'] },
  ];
  const launching = s === 'launching';
  const state = {
    app: "Gumby's Launcher", accounts, groups, allow_multi: false, max_accounts: 8, max_group: 4, regions: ['Americas', 'Europe', 'Asia'],
    running: launching ? 2 : s === 'open' ? 3 : 0, game_folder: String.raw`C:\Program Files (x86)\Diablo II Resurrected`,
    open: launching ? [{ account: 'a1', quarter: 0 }] : s === 'open' ? [{ account: 'a1', quarter: 0 }, { account: 'a2', quarter: 1 }, { account: 'a3', quarter: 2 }] : [],
    launch: launching ? { busy: true, label: 'Farm team', group: 'g1', error: null, steps: [
      { account: 'a1', name: 'Main Barb', state: 'done', note: 'Open' },
      { account: 'a2', name: 'Mule Sorc', state: 'settling', note: 'Window open, signing in…' },
      { account: 'a3', name: 'Pally Alt', state: 'waiting', note: '' }] }
      : { busy: false, label: null, group: null, steps: [], error: null },
  };
  const now = Date.now() / 1000;
  state.version = '0.1.0';
  state.fav_zones = ['chaos', 'cows', 'tunnels']; state.alert_next = true; state.alert_sound = true;
  state.zone_groups = [['blood-moor','Blood Moor & Den of Evil',1],['tower','Forgotten Tower (Countess)',1],['pit','The Pit (Tamoe Highland)',1],['cows','The Secret Cow Level',1],
    ['tunnels','Ancient Tunnels',2],['arcane','Arcane Sanctuary',2],['tal-rasha',"Tal Rasha's Tombs & Canyon of the Magi",2],['travincal','Travincal',3],['durance','Durance of Hate (Mephisto)',3],
    ['chaos','The Chaos Sanctuary',4],['city-damned','City of the Damned & River of Flame',4],['nihlathak',"Nihlathak's Temple & Halls",5],['wsk','Worldstone Keep & Throne of Destruction',5]]
    .map(([key, name, act]) => ({ key, name, act, ids: key === 'chaos' ? [108] : [] }));
  state.zones = s === 'empty' ? { error: 'Terror zones are not reachable right now' }
    : { now: 'The Chaos Sanctuary', now_act: 4, now_ids: [108], next: "Tal Rasha's Tomb, Canyon of the Magi", next_ids: [66, 46], switch_at: Math.floor(now) + 1421 };
  state.update = s === 'home' ? { state: 'ready', version: '0.2.0', notes: 'Groups can be dragged into a new order.' }
    : { state: 'current', checked_at: Math.floor(now) - 120 };
  state.report = s === 'report' ? { id: '20261020-211415-launch-1', reason: 'Launch stopped: game 2: D2R closed right after starting', sending: false,
    text: "Gumby's Launcher 0.1.0 · Windows 11 24H2 (26100)\nReason: Launch stopped: game 2: D2R closed right after starting\n\n" +
      '21:14:03  Launch all: 3 games (0 already open, several games on)\n21:14:03  game 1 started\n21:14:09  game 1: window after 6.2 s\n' +
      '21:14:14  game 1: open\n21:14:14  game 2: closed 1 lock(s) in 1 running game(s)\n21:14:14  game 2 started\n' +
      '21:14:15  game 2: gone right after starting, trying once more\n21:14:15  game 2 started (second try)\n21:14:15  game 2 FAILED: D2R closed right after starting\n' } : null;
  window.previewInvoke = async (op, p) => {
    if (op === 'state') return state;
    if (op === 'signin_status') return { state: s === 'signedin' ? 'done' : 'waiting' };
    if (op === 'launch_group' && !p.multi) throw 'needs_multi:3';
    return true;
  };
})();
