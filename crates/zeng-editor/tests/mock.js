// Browser-only stand-in for the Rust shell, used to test the editor UI in Chromium.
(() => {
  const mem = {};
  const base = '/templates/coin-collector/';
  const files = {
    'assets/sounds': [{ name: 'pop.wav', dir: false }, { name: 'win.wav', dir: false }],
    'assets/models': [], 'assets/textures': [],
  };
  const reply = (id, ok, data) => setTimeout(() => window.zengReply(ok ? { id, ok, data } : { id, ok, error: data }), 0);
  window.__mem = mem;
  window.ipc = {
    postMessage: async (s) => {
      const m = JSON.parse(s);
      try {
        switch (m.cmd) {
          case 'home': return reply(m.id, true, { recent: [], player: true, templates: [{ id: 'coin-collector', title: 'Coin Collector', blurb: 'Grab coins.', order: 1 }] });
          case 'new_project': case 'open_project': case 'open_path': return reply(m.id, true, { dir: '/x' });
          case 'read': {
            if (m.rel in mem) return reply(m.id, true, mem[m.rel]);
            const r = await fetch(base + m.rel.split('/').map(encodeURIComponent).join('/'));
            if (!r.ok) return reply(m.id, false, 'missing ' + m.rel);
            return reply(m.id, true, await r.text());
          }
          case 'write': mem[m.rel] = m.text; return reply(m.id, true, null);
          case 'list': return reply(m.id, true, files[m.rel] || []);
          default: return reply(m.id, true, null);
        }
      } catch (e) { reply(m.id, false, String(e)); }
    },
  };
})();
