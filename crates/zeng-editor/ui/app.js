// ZengEngine editor. Talks to the Rust shell through window.ipc (see src/main.rs).
(function () {
  const $ = (s) => document.querySelector(s);
  const el = (tag, attrs = {}, ...kids) => {
    const e = document.createElement(tag);
    for (const k in attrs) {
      if (k === 'class') e.className = attrs[k];
      else if (k.startsWith('on')) e.addEventListener(k.slice(2), attrs[k]);
      else if (k === 'value') e.value = attrs[k];
      else if (attrs[k] !== false && attrs[k] != null) e.setAttribute(k, attrs[k] === true ? '' : attrs[k]);
    }
    for (const kid of kids.flat()) if (kid != null) e.append(kid.nodeType ? kid : document.createTextNode(kid));
    return e;
  };

  // ---------- ipc ----------
  let nextId = 1;
  const pending = {};
  function call(cmd, args = {}) {
    return new Promise((res, rej) => {
      const id = nextId++;
      pending[id] = { res, rej };
      window.ipc.postMessage(JSON.stringify({ id, cmd, ...args }));
    });
  }
  window.zengReply = (r) => {
    const p = pending[r.id];
    if (!p) return;
    delete pending[r.id];
    r.ok ? p.res(r.data) : p.rej(new Error(r.error));
  };

  let toastTimer;
  function toast(msg, ms = 2200) {
    const t = $('#toast');
    t.textContent = msg;
    t.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (t.hidden = true), ms);
  }
  const fail = (e) => toast(e.message || String(e), 4500);

  // ---------- state ----------
  let project = null; // project.json contents
  let scenePath = 'scenes/main.scene.json';
  let scene = { environment: {}, entities: [] };
  let sel = -1;
  const scripts = {}; // rel path -> json string
  const projectVars = new Set();
  let assets = { models: [], textures: [], sounds: [] };
  let ws = null;
  let loadingWs = false;
  let tab = 'blocks';
  let playing = false;
  let problems = [];

  window.zengProject = {
    objects: () => scene.entities.map((e) => e.name),
    sounds: () => assets.sounds,
  };

  // ---------- history (scene undo/redo) ----------
  let hist = [];
  let histIdx = -1;
  let histTimer;
  function commitNow() {
    const s = JSON.stringify(scene);
    if (hist[histIdx] === s) return;
    hist = hist.slice(0, histIdx + 1);
    hist.push(s);
    if (hist.length > 100) hist.shift();
    histIdx = hist.length - 1;
  }
  function commitSoon() {
    clearTimeout(histTimer);
    histTimer = setTimeout(commitNow, 400);
  }
  function restore(i) {
    histIdx = i;
    scene = JSON.parse(hist[i]);
    if (sel >= scene.entities.length) sel = scene.entities.length - 1;
    afterSceneChange(true);
  }

  // ---------- saving ----------
  let sceneTimer;
  function saveSceneSoon() {
    clearTimeout(sceneTimer);
    sceneTimer = setTimeout(saveSceneNow, 250);
  }
  async function saveSceneNow() {
    clearTimeout(sceneTimer);
    if (!project) return;
    try {
      await call('write', { rel: scenePath, text: JSON.stringify(scene, null, 2) });
    } catch (e) { fail(e); }
  }
  const scriptTimers = {};
  function saveScriptSoon(path) {
    clearTimeout(scriptTimers[path]);
    scriptTimers[path] = setTimeout(() => saveScriptNow(path), 250);
  }
  async function saveScriptNow(path) {
    clearTimeout(scriptTimers[path]);
    try {
      await call('write', { rel: path, text: scripts[path] });
    } catch (e) { fail(e); }
  }
  async function flush() {
    await saveSceneNow();
    for (const p in scriptTimers) if (scriptTimers[p]) await saveScriptNow(p);
  }

  function afterSceneChange(full) {
    renderTree();
    renderInspector();
    drawLayout();
    if (full) loadScriptForSelection();
    saveSceneSoon();
    commitSoon();
  }

  // ---------- entities ----------
  const uniqueName = (base) => {
    let n = base, i = 2;
    const names = new Set(scene.entities.map((e) => e.name));
    while (names.has(n)) n = `${base} ${i++}`;
    return n;
  };
  const MAT = (c, extra = {}) => ({ color: c, metallic: 0, roughness: 0.5, emissive: [0, 0, 0], ...extra });
  const KINDS = [
    ['Cube', () => ({ name: uniqueName('Cube'), shape: { type: 'cube', size: 1 }, material: MAT([0.9, 0.35, 0.25]) })],
    ['Sphere', () => ({ name: uniqueName('Ball'), shape: { type: 'sphere', radius: 0.5 }, material: MAT([0.25, 0.5, 0.9], { metallic: 0.3, roughness: 0.3 }) })],
    ['Cylinder', () => ({ name: uniqueName('Cylinder'), shape: { type: 'cylinder', radius: 0.5, height: 1 }, material: MAT([0.9, 0.8, 0.2]) })],
    ['Capsule', () => ({ name: uniqueName('Capsule'), shape: { type: 'capsule', radius: 0.4, length: 1 }, material: MAT([0.4, 0.8, 0.5]) })],
    ['Floor', () => ({ name: uniqueName('Floor'), shape: { type: 'cube', size: 1 }, material: MAT([0.4, 0.55, 0.4], { roughness: 0.9 }), transform: { position: [0, -0.5, 0], rotation: [0, 0, 0], scale: [20, 1, 20] }, physics: { body: 'static', bounce: 0.3, friction: 0.7 } })],
    ['Sun (light)', () => ({ name: uniqueName('Sun'), light: { type: 'directional', illuminance: 10000, shadows: true }, transform: { position: [4, 8, 4], rotation: [0, 0, 0], scale: [1, 1, 1], look_at: [0, 0, 0] } })],
    ['Lamp (light)', () => ({ name: uniqueName('Lamp'), light: { type: 'point', intensity: 800000, shadows: false }, transform: { position: [0, 3, 0], rotation: [0, 0, 0], scale: [1, 1, 1] } })],
    ['Spotlight', () => ({ name: uniqueName('Spotlight'), light: { type: 'spot', intensity: 2000000, angle: 0.7, shadows: true }, transform: { position: [0, 5, 0], rotation: [0, 0, 0], scale: [1, 1, 1], look_at: [0, 0, 0] } })],
    ['Camera', () => ({ name: uniqueName('Camera'), camera: { fov: 60 }, transform: { position: [0, 4, 9], rotation: [0, 0, 0], scale: [1, 1, 1], look_at: [0, 0.5, 0] } })],
  ];
  function addEntity(e) {
    e.transform = { position: [0, 0.5, 0], rotation: [0, 0, 0], scale: [1, 1, 1], ...(e.transform || {}) };
    scene.entities.push(e);
    sel = scene.entities.length - 1;
    afterSceneChange(true);
  }
  function scriptPathFor(e) {
    if (e.script) return e.script;
    const base = e.name.replace(/[^\w\- ]/g, '').trim() || 'object';
    let p = `scripts/${base}.blocks.json`, i = 2;
    const used = new Set(scene.entities.map((x) => x.script));
    while (used.has(p)) p = `scripts/${base} ${i++}.blocks.json`;
    return p;
  }

  async function renameEntity(i, name) {
    const e = scene.entities[i];
    const old = e.name;
    name = name.trim();
    if (!name || name === old) return;
    if (scene.entities.some((x, j) => j !== i && x.name === name)) { toast('Another object already has that name.'); return; }
    e.name = name;
    // keep blocks that point at the old name working
    const walk = (n) => {
      if (Array.isArray(n)) n.forEach(walk);
      else if (n && typeof n === 'object') {
        if (n.fields && n.fields.OBJ === old) n.fields.OBJ = name;
        for (const k in n) walk(n[k]);
      }
    };
    for (const p in scripts) {
      try {
        const j = JSON.parse(scripts[p]);
        walk(j);
        scripts[p] = JSON.stringify(j);
        saveScriptSoon(p);
      } catch (_) {}
    }
    afterSceneChange(true);
  }

  // ---------- tree ----------
  const iconColor = (e) => {
    if (e.light) return '#ffd54a';
    if (e.camera) return '#b0b6c3';
    const c = (e.material && e.material.color) || [0.7, 0.7, 0.7];
    return `rgb(${c.map((v) => Math.round(v * 255)).join(',')})`;
  };
  function renderTree() {
    const ul = $('#tree');
    ul.replaceChildren();
    if (!scene.entities.length) ul.append(el('li', { class: 'muted' }, 'Nothing here yet. Press + Add.'));
    scene.entities.forEach((e, i) => {
      const hasScript = e.script && scripts[e.script] && JSON.parse(scripts[e.script] || '{}').blocks?.blocks?.length;
      ul.append(el('li', { class: i === sel ? 'sel' : '', onclick: () => select(i) },
        el('span', { class: 'ico', style: `background:${iconColor(e)}` }),
        el('span', { class: 'nm' }, e.name),
        hasScript ? el('span', { class: 'has', title: 'Has blocks' }) : null,
        el('span', { class: 'x', title: 'Make a copy', onclick: (ev) => { ev.stopPropagation(); duplicate(i); } }, '⧉'),
        el('span', { class: 'x', title: 'Delete', onclick: (ev) => { ev.stopPropagation(); remove(i); } }, '✕')));
    });
  }
  function select(i) {
    if (i === sel) return;
    sel = i;
    renderTree();
    renderInspector();
    drawLayout();
    loadScriptForSelection();
  }
  function remove(i) {
    scene.entities.splice(i, 1);
    if (sel >= scene.entities.length) sel = scene.entities.length - 1;
    else if (sel > i) sel--;
    afterSceneChange(true);
    toast('Deleted. Use Undo to bring it back.');
  }
  function duplicate(i) {
    const c = JSON.parse(JSON.stringify(scene.entities[i]));
    c.name = uniqueName(c.name);
    if (c.script && scripts[c.script]) {
      const np = scriptPathFor({ ...c, script: null });
      scripts[np] = scripts[c.script];
      c.script = np;
      saveScriptSoon(np);
    }
    c.transform.position[0] += 1.5;
    scene.entities.splice(i + 1, 0, c);
    sel = i + 1;
    afterSceneChange(true);
  }

  // ---------- inspector ----------
  const rgb2hex = (c) => '#' + c.map((v) => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, '0')).join('');
  const hex2rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);

  function changed() {
    drawLayout();
    renderTreeIcons();
    saveSceneSoon();
    commitSoon();
  }
  function renderTreeIcons() {
    const items = $('#tree').children;
    scene.entities.forEach((e, i) => {
      const ico = items[i] && items[i].querySelector('.ico');
      if (ico) ico.style.background = iconColor(e);
    });
  }
  const field = (label, ...ctl) => el('div', { class: 'field' }, el('label', {}, label), el('div', {}, ...ctl));
  function numInput(get, set, step = 0.1) {
    const i = el('input', { type: 'number', step, value: String(+get().toFixed(3)) });
    i.addEventListener('input', () => { const v = parseFloat(i.value); if (!isNaN(v)) { set(v); changed(); } });
    return i;
  }
  const xyz = (arr) => el('div', { class: 'xyz' }, [0, 1, 2].map((k) => numInput(() => arr[k], (v) => (arr[k] = v))));
  function sliderRow(get, set, min, max, step) {
    const out = el('output', {}, String(get()));
    const r = el('input', { type: 'range', min, max, step, value: String(get()) });
    r.addEventListener('input', () => { set(parseFloat(r.value)); out.textContent = r.value; changed(); });
    return el('div', { class: 'range-row' }, r, out);
  }
  function colorInput(arr, rerender) {
    const i = el('input', { type: 'color', value: rgb2hex(arr) });
    i.addEventListener('input', () => { const c = hex2rgb(i.value); arr[0] = c[0]; arr[1] = c[1]; arr[2] = c[2]; changed(); });
    return i;
  }
  function select_(options, get, set) {
    const s = el('select', {}, options.map(([l, v]) => el('option', { value: v }, l)));
    s.value = get();
    s.addEventListener('change', () => { set(s.value); changed(); renderInspector(); });
    return s;
  }
  function checkbox(get, set, label) {
    const c = el('input', { type: 'checkbox' });
    c.checked = !!get();
    c.addEventListener('change', () => { set(c.checked); changed(); });
    return el('label', { class: 'check' }, c, label);
  }

  function renderInspector() {
    const root = $('#inspector');
    root.replaceChildren();
    if (sel < 0 || !scene.entities[sel]) return renderEnvironment(root);
    const e = scene.entities[sel];
    const nameIn = el('input', { type: 'text', value: e.name, maxlength: 40 });
    nameIn.addEventListener('change', () => renameEntity(sel, nameIn.value));
    root.append(el('div', { class: 'grp' }, el('h2', {}, 'Object'), field('Name', nameIn)));

    const t = e.transform;
    if (!t.rotation) t.rotation = [0, 0, 0];
    if (!t.scale) t.scale = [1, 1, 1];
    const tg = el('div', { class: 'grp' }, el('h2', {}, 'Where it is'),
      field('Position', xyz(t.position)), field('Turn (y x z)', xyz(t.rotation)), field('Size', xyz(t.scale)));
    if (t.look_at) tg.append(field('Looks at', xyz(t.look_at)), el('div', { class: 'muted' }, 'Lights and cameras can aim at a spot.'));
    root.append(tg);

    if (e.shape) {
      const g = el('div', { class: 'grp' }, el('h2', {}, 'Shape'));
      const sh = e.shape;
      g.append(field('Kind', select_([['Cube', 'cube'], ['Sphere', 'sphere'], ['Cylinder', 'cylinder'], ['Capsule', 'capsule'], ['Flat plane', 'plane']],
        () => sh.type, (v) => { const d = { cube: { size: 1 }, sphere: { radius: 0.5 }, cylinder: { radius: 0.5, height: 1 }, capsule: { radius: 0.4, length: 1 }, plane: { size: 5 } }[v]; e.shape = { type: v, ...d }; })));
      for (const k of Object.keys(sh).filter((k) => k !== 'type')) g.append(field(k, numInput(() => e.shape[k], (v) => (e.shape[k] = Math.max(0.01, v)))));
      root.append(g);
    }
    if (e.material) {
      const m = e.material;
      if (!m.emissive) m.emissive = [0, 0, 0];
      const g = el('div', { class: 'grp' }, el('h2', {}, 'Look'));
      g.append(field('Colour', colorInput(m.color)), field('Shiny metal', sliderRow(() => m.metallic, (v) => (m.metallic = v), 0, 1, 0.05)),
        field('Roughness', sliderRow(() => m.roughness, (v) => (m.roughness = v), 0, 1, 0.05)), field('Glow colour', colorInput(m.emissive)),
        field('Picture', select_([['(none)', ''], ...assets.textures.map((x) => [x.replace(/^textures\//, ''), x])], () => m.texture || '', (v) => { if (v) m.texture = v; else delete m.texture; })));
      root.append(g);
    }
    if (e.model !== undefined) {
      root.append(el('div', { class: 'grp' }, el('h2', {}, '3D model'),
        field('File', select_(assets.models.map((x) => [x.replace(/^models\//, ''), x]), () => e.model, (v) => (e.model = v)))));
    }
    if (e.light) {
      const l = e.light;
      const g = el('div', { class: 'grp' }, el('h2', {}, 'Light'));
      g.append(field('Kind', select_([['Sun', 'directional'], ['Lamp', 'point'], ['Spotlight', 'spot']], () => l.type, (v) => {
        const d = { directional: { illuminance: 10000 }, point: { intensity: 800000 }, spot: { intensity: 2000000, angle: 0.7 } }[v];
        e.light = { type: v, shadows: l.shadows, ...d };
      })));
      const key = l.type === 'directional' ? 'illuminance' : 'intensity';
      g.append(field('Brightness', numInput(() => l[key], (v) => (l[key] = Math.max(0, v)), l.type === 'directional' ? 500 : 50000)));
      if (l.type === 'spot') g.append(field('Width', sliderRow(() => l.angle, (v) => (l.angle = v), 0.1, 1.5, 0.05)));
      g.append(field('Shadows', checkbox(() => l.shadows, (v) => (l.shadows = v), 'cast shadows')));
      root.append(g);
    }
    if (e.camera) {
      root.append(el('div', { class: 'grp' }, el('h2', {}, 'Camera'),
        field('Zoom (fov)', sliderRow(() => e.camera.fov, (v) => (e.camera.fov = v), 20, 110, 1)),
        el('div', { class: 'muted' }, 'With no camera, the game uses one you can orbit with the mouse. Use the "camera follows" block to chase a player.')));
    }
    if (e.shape || e.model) {
      const p = e.physics;
      const g = el('div', { class: 'grp' }, el('h2', {}, 'Physics'));
      g.append(field('Acts like', select_([['Nothing (not physical)', ''], ['Falls and bumps', 'dynamic'], ['Solid and still', 'static'], ['Solid, moved by blocks', 'kinematic']],
        () => (p ? p.body : ''), (v) => { if (!v) delete e.physics; else e.physics = { bounce: 0.4, friction: 0.5, ...(e.physics || {}), body: v }; })));
      if (p) {
        g.append(field('Bounciness', sliderRow(() => p.bounce, (v) => (p.bounce = v), 0, 1, 0.05)), field('Grip', sliderRow(() => p.friction, (v) => (p.friction = v), 0, 1, 0.05)));
        if (e.model) g.append(field('Box (w h d)', xyz((p.box_size = p.box_size || [1, 1, 1]))));
      }
      root.append(g);
    }
  }

  function renderEnvironment(root) {
    const env = (scene.environment = scene.environment || {});
    env.sky = env.sky || [0.16, 0.17, 0.19];
    if (env.ambient == null) env.ambient = 300;
    if (env.gravity == null) env.gravity = -9.81;
    root.append(el('div', { class: 'grp' }, el('h2', {}, 'World'),
      field('Sky colour', colorInput(env.sky)),
      field('Gravity', numInput(() => env.gravity, (v) => (env.gravity = v), 0.5)),
      field('Soft light', sliderRow(() => env.ambient, (v) => (env.ambient = v), 0, 2000, 50)),
      field('Glow (bloom)', checkbox(() => env.bloom, (v) => (env.bloom = v), 'bright things glow')),
      field('Fog', checkbox(() => !!env.fog, (v) => { env.fog = v ? { color: env.sky.slice(), start: 15, end: 60 } : undefined; if (!v) delete env.fog; }, 'distant things fade')),
      el('div', { class: 'muted' }, 'Pick an object on the left to change it.')));
    root.append(el('div', { class: 'grp' }, el('h2', {}, 'Quality'),
      field('Graphics', select_([['Fast (older computers)', 'low'], ['Nice', 'medium'], ['Best (ambient shadows)', 'high']], () => project.quality || 'medium', (v) => { project.quality = v; saveProject(); })),
      el('div', { class: 'muted' }, 'Players can run your game at this level.')));
  }
  async function saveProject() {
    try { await call('write', { rel: 'project.json', text: JSON.stringify(project, null, 2) }); } catch (e) { fail(e); }
  }

  // ---------- assets ----------
  async function loadAssets() {
    for (const kind of ['models', 'textures', 'sounds']) {
      try {
        const l = await call('list', { rel: `assets/${kind}` });
        assets[kind] = l.filter((f) => !f.dir).map((f) => `${kind}/${f.name}`).sort();
      } catch (_) { assets[kind] = []; }
    }
    renderAssets();
  }
  function renderAssets() {
    const root = $('#assets');
    root.replaceChildren();
    const groups = [['3D models', 'models'], ['Pictures', 'textures'], ['Sounds', 'sounds']];
    let any = false;
    for (const [title, kind] of groups) {
      if (!assets[kind].length) continue;
      any = true;
      root.append(el('div', { class: 'grp-title grp' }, title));
      for (const f of assets[kind]) {
        const row = el('div', { class: 'file' }, el('span', { title: f }, f.replace(/^\w+\//, '')));
        if (kind === 'models') row.append(el('button', { class: 'small', onclick: () => addEntity({ name: uniqueName(f.split('/').pop().replace(/\.\w+$/, '')), model: f }) }, 'Add'));
        if (kind === 'textures') row.append(el('button', { class: 'small', onclick: () => { const e = scene.entities[sel]; if (e && e.material) { e.material.texture = f; afterSceneChange(false); } else toast('Pick a shape first.'); } }, 'Use'));
        root.append(row);
      }
    }
    if (!any) root.append(el('div', { class: 'none' }, 'No files yet. Import a 3D model (.glb), a picture or a sound.'));
    root.querySelectorAll('.grp-title').forEach((n) => { n.className = 'grp'; n.style.cssText = 'color:var(--dim);font-size:11px;text-transform:uppercase;letter-spacing:.06em;border:0;margin:8px 0 2px;padding:0'; });
  }

  // ---------- blockly ----------
  function initBlockly() {
    const theme = Blockly.Theme.defineTheme('zeng', {
      name: 'zeng',
      base: Blockly.Themes.Classic,
      componentStyles: {
        workspaceBackgroundColour: '#14161a', toolboxBackgroundColour: '#1b1e24', toolboxForegroundColour: '#e8eaee',
        flyoutBackgroundColour: '#1f232a', flyoutForegroundColour: '#e8eaee', flyoutOpacity: 0.97, scrollbarColour: '#4a5160',
        insertionMarkerColour: '#ffffff', insertionMarkerOpacity: 0.3, cursorColour: '#d0d0d0',
      },
      fontStyle: { family: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif', weight: '600', size: 11 },
    });
    ws = Blockly.inject('blockly', {
      toolbox: window.ZENG_TOOLBOX, renderer: 'zelos', theme, media: 'vendor/media/',
      grid: { spacing: 24, length: 2, colour: '#262a31', snap: true },
      zoom: { controls: true, wheel: true, startScale: 0.9, maxScale: 2, minScale: 0.4 },
      trashcan: true, move: { scrollbars: true, drag: true, wheel: true }, sounds: false,
    });
    ws.addChangeListener((ev) => {
      if (loadingWs || ev.isUiEvent) return;
      if (ev.type === Blockly.Events.VAR_CREATE) projectVars.add(ev.varName);
      if (sel < 0) return;
      const e = scene.entities[sel];
      const path = scriptPathFor(e);
      const first = !e.script;
      scripts[path] = JSON.stringify(Blockly.serialization.workspaces.save(ws));
      if (first) { e.script = path; saveSceneSoon(); }
      saveScriptSoon(path);
      renderTreeMarks();
    });
    window.addEventListener('resize', () => { Blockly.svgResize(ws); drawLayout(); });
  }
  function renderTreeMarks() {
    const items = $('#tree').children;
    scene.entities.forEach((e, i) => {
      const li = items[i];
      if (!li) return;
      let has = false;
      try { has = !!(e.script && scripts[e.script] && JSON.parse(scripts[e.script]).blocks?.blocks?.length); } catch (_) {}
      const dot = li.querySelector('.has');
      if (has && !dot) li.insertBefore(el('span', { class: 'has', title: 'Has blocks' }), li.querySelector('.x'));
      if (!has && dot) dot.remove();
    });
  }
  function loadScriptForSelection() {
    const none = sel < 0 || !scene.entities[sel];
    $('#noscript').hidden = !none;
    $('#blockly').style.visibility = none ? 'hidden' : 'visible';
    $('#tabnote').textContent = none ? '' : `Blocks for ${scene.entities[sel].name}`;
    if (!ws) return;
    loadingWs = true;
    try {
      ws.clear();
      if (!none) {
        const e = scene.entities[sel];
        const src = e.script && scripts[e.script];
        if (src) Blockly.serialization.workspaces.load(JSON.parse(src), ws);
        const have = new Set(ws.getVariableMap().getAllVariables().map((v) => v.getName()));
        projectVars.forEach((n) => { if (!have.has(n)) ws.getVariableMap().createVariable(n); });
      }
    } catch (err) {
      toast('Could not read this script: ' + err.message, 5000);
    }
    loadingWs = false;
    setTimeout(() => Blockly.svgResize(ws), 0);
  }

  // ---------- layout canvas ----------
  const view = { mode: 'top', cx: 0, cy: 0, scale: 40 };
  const cv = $('#layout');
  const ctx = cv.getContext('2d');
  function sizeCanvas() {
    const r = cv.getBoundingClientRect();
    const d = window.devicePixelRatio || 1;
    if (cv.width !== Math.round(r.width * d) || cv.height !== Math.round(r.height * d)) {
      cv.width = Math.round(r.width * d);
      cv.height = Math.round(r.height * d);
    }
    ctx.setTransform(d, 0, 0, d, 0, 0);
    return r;
  }
  function half(e) {
    const s = e.transform.scale || [1, 1, 1];
    let h = [0.5, 0.5, 0.5];
    const sh = e.shape;
    if (sh) {
      if (sh.type === 'cube') h = [sh.size / 2, sh.size / 2, sh.size / 2];
      else if (sh.type === 'sphere') h = [sh.radius, sh.radius, sh.radius];
      else if (sh.type === 'cylinder') h = [sh.radius, sh.height / 2, sh.radius];
      else if (sh.type === 'capsule') h = [sh.radius, sh.length / 2 + sh.radius, sh.radius];
      else if (sh.type === 'plane') h = [sh.size / 2, 0.01, sh.size / 2];
    } else if (e.physics && e.physics.box_size) h = e.physics.box_size.map((v) => v / 2);
    return h.map((v, i) => Math.abs(v * s[i]));
  }
  // world -> screen. top: (x,z). side: (x,-y)
  const W2S = (r, x, v) => [r.width / 2 + (x - view.cx) * view.scale, r.height / 2 + (v - view.cy) * view.scale];
  const axisV = (e) => (view.mode === 'top' ? e.transform.position[2] : -e.transform.position[1]);
  function drawLayout() {
    if (tab !== 'layout') return;
    const r = sizeCanvas();
    ctx.clearRect(0, 0, r.width, r.height);
    // grid
    ctx.lineWidth = 1;
    const step = view.scale < 18 ? 5 : 1;
    const x0 = Math.floor((view.cx - r.width / 2 / view.scale) / step) * step, x1 = view.cx + r.width / 2 / view.scale;
    const y0 = Math.floor((view.cy - r.height / 2 / view.scale) / step) * step, y1 = view.cy + r.height / 2 / view.scale;
    for (let x = x0; x <= x1; x += step) { const [sx] = W2S(r, x, 0); ctx.strokeStyle = x === 0 ? '#3a4150' : x % 5 === 0 ? '#232831' : '#1a1e24'; ctx.beginPath(); ctx.moveTo(sx, 0); ctx.lineTo(sx, r.height); ctx.stroke(); }
    for (let y = y0; y <= y1; y += step) { const [, sy] = W2S(r, 0, y); ctx.strokeStyle = y === 0 ? '#3a4150' : y % 5 === 0 ? '#232831' : '#1a1e24'; ctx.beginPath(); ctx.moveTo(0, sy); ctx.lineTo(r.width, sy); ctx.stroke(); }
    ctx.fillStyle = '#5c6472'; ctx.font = '11px sans-serif';
    ctx.fillText(view.mode === 'top' ? 'looking down from above (x right, z toward you)' : 'looking from the side (x right, y up)', 12, r.height - 12);

    const order = scene.entities.map((e, i) => [e, i]).sort((a, b) => (view.mode === 'top' ? a[0].transform.position[1] - b[0].transform.position[1] : b[0].transform.position[2] - a[0].transform.position[2]));
    for (const [e, i] of order) {
      const p = e.transform.position;
      const [sx, sy] = W2S(r, p[0], axisV(e));
      ctx.save();
      if (e.light || e.camera) {
        ctx.fillStyle = e.light ? '#ffd54a' : '#b0b6c3';
        ctx.beginPath(); ctx.arc(sx, sy, 9, 0, 7); ctx.fill();
        ctx.fillStyle = '#14161a'; ctx.font = 'bold 10px sans-serif'; ctx.textAlign = 'center';
        ctx.fillText(e.light ? '☀' : '▣', sx, sy + 4);
      } else {
        const h = half(e);
        const hv = view.mode === 'top' ? h[2] : h[1];
        const w = Math.max(6, h[0] * 2 * view.scale), hh = Math.max(6, hv * 2 * view.scale);
        ctx.fillStyle = iconColor(e);
        ctx.globalAlpha = e.physics && e.physics.body === 'static' ? 0.55 : 0.9;
        const round = e.shape && (e.shape.type === 'sphere' || e.shape.type === 'capsule');
        ctx.beginPath();
        if (round) ctx.ellipse(sx, sy, w / 2, hh / 2, 0, 0, 7); else ctx.roundRect(sx - w / 2, sy - hh / 2, w, hh, 4);
        ctx.fill();
        ctx.globalAlpha = 1;
      }
      if (i === sel) {
        ctx.strokeStyle = '#4c97ff'; ctx.lineWidth = 2;
        const h = e.light || e.camera ? [0.4, 0.4, 0.4] : half(e);
        const hv = view.mode === 'top' ? h[2] : h[1];
        const w = Math.max(14, h[0] * 2 * view.scale) + 6, hh = Math.max(14, hv * 2 * view.scale) + 6;
        ctx.strokeRect(sx - w / 2, sy - hh / 2, w, hh);
      }
      ctx.restore();
      ctx.fillStyle = '#c8ced9'; ctx.font = '11px sans-serif'; ctx.textAlign = 'center';
      ctx.fillText(e.name, sx, sy - 12 - (e.light || e.camera ? 0 : Math.max(0, (view.mode === 'top' ? half(e)[2] : half(e)[1]) * view.scale - 2)));
    }
  }
  function hit(mx, my) {
    const r = cv.getBoundingClientRect();
    const rr = { width: r.width, height: r.height };
    for (let i = scene.entities.length - 1; i >= 0; i--) {
      const e = scene.entities[i];
      const [sx, sy] = W2S(rr, e.transform.position[0], axisV(e));
      const h = e.light || e.camera ? [0.4, 0.4, 0.4] : half(e);
      const hv = view.mode === 'top' ? h[2] : h[1];
      const w = Math.max(18, h[0] * 2 * view.scale) / 2, hh = Math.max(18, hv * 2 * view.scale) / 2;
      if (Math.abs(mx - sx) <= w && Math.abs(my - sy) <= hh) return i;
    }
    return -1;
  }
  let drag = null;
  cv.addEventListener('pointerdown', (ev) => {
    const r = cv.getBoundingClientRect();
    const mx = ev.clientX - r.left, my = ev.clientY - r.top;
    const i = hit(mx, my);
    cv.setPointerCapture(ev.pointerId);
    if (i >= 0) {
      if (i !== sel) select(i);
      const e = scene.entities[i];
      drag = { kind: 'obj', mx, my, p: e.transform.position.slice() };
    } else drag = { kind: 'pan', mx, my, cx: view.cx, cy: view.cy };
  });
  cv.addEventListener('pointermove', (ev) => {
    if (!drag) return;
    const r = cv.getBoundingClientRect();
    const dx = (ev.clientX - r.left - drag.mx) / view.scale, dy = (ev.clientY - r.top - drag.my) / view.scale;
    if (drag.kind === 'pan') { view.cx = drag.cx - dx; view.cy = drag.cy - dy; drawLayout(); return; }
    const e = scene.entities[sel];
    const snap = (v) => (ev.shiftKey ? Math.round(v * 2) / 2 : Math.round(v * 100) / 100);
    e.transform.position[0] = snap(drag.p[0] + dx);
    if (view.mode === 'top') e.transform.position[2] = snap(drag.p[2] + dy);
    else e.transform.position[1] = snap(drag.p[1] - dy);
    drawLayout();
    renderInspectorPos();
    saveSceneSoon();
    commitSoon();
  });
  cv.addEventListener('pointerup', () => (drag = null));
  cv.addEventListener('wheel', (ev) => {
    ev.preventDefault();
    const f = Math.exp(-ev.deltaY * 0.0015);
    view.scale = Math.max(6, Math.min(200, view.scale * f));
    drawLayout();
  }, { passive: false });
  function renderInspectorPos() {
    // refresh only position inputs while dragging so typing elsewhere is not disturbed
    const first = $('#inspector .xyz');
    if (!first || sel < 0) return;
    const p = scene.entities[sel].transform.position;
    first.querySelectorAll('input').forEach((inp, k) => (inp.value = String(+p[k].toFixed(3))));
  }
  function setView(m) {
    view.mode = m;
    view.cy = m === 'top' ? 0 : -1;
    $('#view-top').classList.toggle('on', m === 'top');
    $('#view-side').classList.toggle('on', m === 'side');
    drawLayout();
  }
  $('#view-top').onclick = () => setView('top');
  $('#view-side').onclick = () => setView('side');

  // ---------- tabs ----------
  function setTab(t) {
    tab = t;
    document.querySelectorAll('.tab').forEach((b) => b.classList.toggle('on', b.dataset.tab === t));
    $('#pane-blocks').classList.toggle('on', t === 'blocks');
    $('#pane-layout').classList.toggle('on', t === 'layout');
    if (t === 'blocks') setTimeout(() => ws && Blockly.svgResize(ws), 0);
    else drawLayout();
  }
  document.querySelectorAll('.tab').forEach((b) => (b.onclick = () => setTab(b.dataset.tab)));

  // ---------- add menu ----------
  const addMenu = $('#add-menu');
  function buildAddMenu() {
    addMenu.replaceChildren();
    for (const [label, mk] of KINDS) addMenu.append(el('button', { onclick: () => { addMenu.hidden = true; addEntity(mk()); } }, label));
    addMenu.append(el('button', { onclick: () => { addMenu.hidden = true; if (!assets.models.length) { toast('Import a .glb model first (Files, + Import).'); return; } addEntity({ name: uniqueName('Model'), model: assets.models[0], physics: undefined }); } }, '3D model…'));
  }
  $('#btn-add').onclick = (ev) => { ev.stopPropagation(); addMenu.hidden = !addMenu.hidden; };
  document.addEventListener('click', () => (addMenu.hidden = true));

  // ---------- problems ----------
  function ownerOf(blockId) {
    for (const i in scene.entities) {
      const e = scene.entities[i];
      if (e.script && scripts[e.script] && scripts[e.script].includes(`"id":"${blockId}"`)) return +i;
    }
    return -1;
  }
  function renderProblems() {
    $('#problems').hidden = !problems.length;
    const ul = $('#problem-list');
    ul.replaceChildren();
    for (const p of problems) {
      const owner = p.id ? ownerOf(p.id) : -1;
      const who = owner >= 0 ? `${scene.entities[owner].name}: ` : '';
      ul.append(el('li', { onclick: () => goToProblem(p) }, `${who}${p.msg}`));
    }
    setTimeout(() => ws && Blockly.svgResize(ws), 0);
  }
  function goToProblem(p) {
    const owner = p.id ? ownerOf(p.id) : -1;
    if (owner < 0) return;
    setTab('blocks');
    sel = -2;
    select(owner);
    setTimeout(() => {
      const blk = ws.getBlockById(p.id);
      if (blk) { ws.centerOnBlock(p.id); blk.select(); }
    }, 60);
  }
  $('#btn-clear-problems').onclick = () => { problems = []; renderProblems(); };
  window.zengEvent = (ev) => {
    if (ev.type === 'error') {
      if (!problems.some((p) => p.id === ev.id && p.msg === ev.msg)) problems.push({ id: ev.id, msg: ev.msg });
      renderProblems();
    } else if (ev.type === 'reloaded') {
      problems = [];
      renderProblems();
    } else if (ev.type === 'stopped') {
      setPlaying(false);
    }
  };

  // ---------- stage preview / play ----------
  const stage = $('#stage');
  const sctx = stage.getContext('2d');
  let frameVer = 0;
  let stageOn = false;
  let polling = false;
  function setPlaying(p, mode) {
    playing = p;
    $('#btn-play').hidden = p;
    $('#btn-stop').hidden = !p;
    stageOn = p && mode !== 'window';
    $('#stage-idle').hidden = stageOn;
    if (stageOn) { frameVer = 0; pollFrames(); } else sctx.clearRect(0, 0, stage.width, stage.height);
  }
  async function pollFrames() {
    if (polling) return;
    polling = true;
    while (stageOn) {
      try {
        const r = await fetch(`${location.origin}/frame.jpg?since=${frameVer}`);
        if (r.status === 200) {
          frameVer = +r.headers.get('X-Frame') || frameVer + 1;
          const bmp = await createImageBitmap(await r.blob());
          sctx.drawImage(bmp, 0, 0, stage.width, stage.height);
          bmp.close();
          continue;
        }
      } catch (_) {}
      await new Promise((res) => setTimeout(res, 12));
    }
    polling = false;
  }
  async function play(mode) {
    if (!project) return;
    try {
      problems = [];
      renderProblems();
      await flush();
      await call('play', { stage: mode !== 'window' });
      setPlaying(true, mode);
      if (mode === 'window') toast('Game is running in its own window. Changes you make here show up in it.');
      else stage.focus({ preventScroll: true });
    } catch (e) { fail(e); }
  }
  $('#btn-play').onclick = () => play();
  $('#stage-go').onclick = () => play();
  $('#stage-window').onclick = () => play('window');
  async function stopGame() { await call('stop').catch(fail); setPlaying(false); }
  $('#btn-stop').onclick = stopGame;
  $('#stage-stop').onclick = stopGame;

  // keyboard and mouse on the stage are sent to the game
  const post = (cmd, args) => window.ipc.postMessage(JSON.stringify({ cmd, ...args }));
  let outbox = [];
  const send = (l) => { if (stageOn) { outbox.push(l); if (outbox.length === 1) requestAnimationFrame(() => { post('stage_input', { lines: outbox }); outbox = []; }); } };
  const keyName = (e) => {
    const c = e.code;
    if (/^Key[A-Z]$/.test(c)) return c.slice(3);
    if (/^Digit\d$/.test(c)) return c.slice(5);
    return { Space: 'SPACE', Enter: 'ENTER', ArrowUp: 'UP', ArrowDown: 'DOWN', ArrowLeft: 'LEFT', ArrowRight: 'RIGHT', ShiftLeft: 'SHIFT', ShiftRight: 'SHIFT', ControlLeft: 'CTRL', ControlRight: 'CTRL', Escape: 'ESC' }[c] || null;
  };
  stage.addEventListener('keydown', (e) => { const k = keyName(e); if (k) { e.preventDefault(); if (!e.repeat) send(`K ${k} 1`); } });
  stage.addEventListener('keyup', (e) => { const k = keyName(e); if (k) { e.preventDefault(); send(`K ${k} 0`); } });
  stage.addEventListener('blur', () => { ['W','A','S','D','UP','DOWN','LEFT','RIGHT','SPACE'].forEach((k) => send(`K ${k} 0`)); });
  const norm = (e) => { const r = stage.getBoundingClientRect(); return [(e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height]; };
  let lastM = null;
  stage.addEventListener('pointerdown', (e) => { stage.focus(); stage.setPointerCapture(e.pointerId); const [x, y] = norm(e); send(`M ${x} ${y}`); send('B 1'); lastM = [e.clientX, e.clientY]; });
  stage.addEventListener('pointermove', (e) => {
    const [x, y] = norm(e); send(`M ${x} ${y}`);
    if (lastM) { const r = stage.getBoundingClientRect(); send(`D ${(e.clientX - lastM[0]) * stage.width / r.width} ${(e.clientY - lastM[1]) * stage.height / r.height}`); lastM = [e.clientX, e.clientY]; }
  });
  stage.addEventListener('pointerup', () => { send('B 0'); lastM = null; });
  stage.addEventListener('wheel', (e) => { e.preventDefault(); send(`S ${-e.deltaY}`); }, { passive: false });

  async function play_() {}
  $('#btn-folder').onclick = () => call('reveal').catch(fail);
  $('#btn-export').onclick = async () => {
    try {
      await flush();
      toast('Exporting…', 8000);
      const r = await call('export');
      if (r) toast(`Saved your game: ${r.path}`, 6000);
      else $('#toast').hidden = true;
    } catch (e) { fail(e); }
  };
  $('#btn-import').onclick = async () => {
    try {
      const added = await call('import_assets');
      if (added.length) { await loadAssets(); renderInspector(); toast(`Added ${added.length} file${added.length > 1 ? 's' : ''}.`); }
    } catch (e) { fail(e); }
  };
  $('#btn-undo').onclick = () => (tab === 'blocks' ? ws.undo(false) : histIdx > 0 && restore(histIdx - 1));
  $('#btn-redo').onclick = () => (tab === 'blocks' ? ws.undo(true) : histIdx < hist.length - 1 && restore(histIdx + 1));
  $('#btn-home').onclick = async () => { await flush(); showHome(); };
  window.addEventListener('keydown', (ev) => {
    const mod = ev.metaKey || ev.ctrlKey;
    if (mod && ev.key === 's') { ev.preventDefault(); flush().then(() => toast('Saved')); }
    else if (mod && ev.key === 'Enter') { ev.preventDefault(); play(); }
    else if (mod && ev.key.toLowerCase() === 'z' && tab === 'layout' && !(ev.target instanceof HTMLInputElement)) {
      ev.preventDefault();
      if (ev.shiftKey) $('#btn-redo').click(); else $('#btn-undo').click();
    }
  });

  // ---------- project open ----------
  async function openProject() {
    const read = async (rel) => JSON.parse(await call('read', { rel }));
    project = await read('project.json');
    scenePath = project.main_scene || 'scenes/main.scene.json';
    try { scene = await read(scenePath); } catch (_) { scene = { environment: {}, entities: [] }; }
    scene.entities = scene.entities || [];
    scene.entities.forEach((e) => { e.transform = { position: [0, 0, 0], rotation: [0, 0, 0], scale: [1, 1, 1], ...(e.transform || {}) }; });
    for (const k in scripts) delete scripts[k];
    projectVars.clear();
    for (const e of scene.entities) {
      if (!e.script) continue;
      try {
        scripts[e.script] = await call('read', { rel: e.script });
        for (const v of JSON.parse(scripts[e.script]).variables || []) projectVars.add(v.name);
      } catch (_) { delete e.script; }
    }
    await loadAssets();
    hist = []; histIdx = -1; commitNow();
    sel = scene.entities.length ? 0 : -1;
    problems = [];
    renderProblems();
    $('#projname').textContent = project.name || '';
    document.title = `${project.name} - ZengEngine`;
    setView('top');
    view.cx = 0; view.cy = 0; view.scale = 40;
    renderTree(); renderInspector(); loadScriptForSelection(); drawLayout();
    $('#home').hidden = true;
  }

  // ---------- home ----------
  let chosenTemplate = null;
  let autoDone = false;
  async function showHome() {
    $('#home').hidden = false;
    $('#home-err').textContent = '';
    let h;
    try { h = await call('home'); } catch (e) { $('#home-err').textContent = e.message; return; }
    $('#home-warn').hidden = h.player;
    if (h.auto && !autoDone) { autoDone = true; await open(call('open_path', { path: h.auto })); if (h.autoplay) play(); if (h.autoexport) $('#btn-export').click(); return; }
    const t = $('#templates');
    t.replaceChildren();
    chosenTemplate = chosenTemplate && h.templates.some((x) => x.id === chosenTemplate) ? chosenTemplate : (h.templates[0] || {}).id;
    for (const tpl of h.templates) {
      t.append(el('button', { class: 'tpl' + (tpl.id === chosenTemplate ? ' on' : ''), onclick: () => { chosenTemplate = tpl.id; showHomeSel(); } },
        el('b', {}, tpl.title || tpl.id), el('span', {}, tpl.blurb || '')));
    }
    const r = $('#recent');
    r.replaceChildren();
    for (const p of h.recent) r.append(el('li', { title: p, onclick: () => open(call('open_path', { path: p })) }, p));
  }
  function showHomeSel() {
    document.querySelectorAll('.tpl').forEach((b, i) => b.classList.toggle('on', false));
    showHome();
  }
  async function open(promise) {
    try {
      const r = await promise;
      if (r) await openProject();
    } catch (e) { $('#home-err').textContent = e.message; }
  }
  $('#btn-new').onclick = () => open(call('new_project', { template: chosenTemplate, name: $('#newname').value }));
  $('#newname').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('#btn-new').click(); });
  $('#btn-open').onclick = () => open(call('open_project'));

  // ---------- boot ----------
  buildAddMenu();
  initBlockly();
  loadScriptForSelection();
  showHome();
  if (window.__zengTest) window.__zengTest({ get scene() { return scene; }, get ws() { return ws; }, scripts, select, addEntity, KINDS, openProject, flush });
})();
