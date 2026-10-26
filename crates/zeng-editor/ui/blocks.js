// Block definitions for ZengEngine. Block type names are read by crates/zeng-player/src/blocks.rs.
(function () {
  const C = {
    events: '#FFBF00', control: '#FFAB19', motion: '#4C97FF', looks: '#9966FF', sound: '#CF63CF',
    physics: '#2EB5B5', sensing: '#5CB1D6', ops: '#59C059', vars: '#FF8C1A', lists: '#FF661A', funcs: '#FF6680',
  };
  window.ZC = C;
  // Filled in by app.js
  window.zengProject = { objects: () => [], sounds: () => [] };

  class ObjField extends Blockly.FieldDropdown {
    constructor(kind) {
      super(function () {
        const names = window.zengProject.objects();
        const out = names.map((n) => [n, n]);
        if (kind === 'any') out.unshift(['anything', '*']);
        if (kind === 'me') out.unshift(['myself', '_ME_']);
        if (kind === 'none') out.push(['nobody', '']);
        const cur = this.getValue && this.getValue();
        if (cur && !out.some((o) => o[1] === cur)) out.push([cur + ' (missing)', cur]);
        return out.length ? out : [['(no objects yet)', '']];
      });
      this.kind = kind;
    }
    doClassValidator_(v) { return v; }
    static fromJson(o) { return new ObjField(o.kind || 'object'); }
  }
  class SoundField extends Blockly.FieldDropdown {
    constructor() {
      super(function () {
        const s = window.zengProject.sounds();
        const out = s.map((n) => [n.replace(/^sounds\//, ''), n]);
        const cur = this.getValue && this.getValue();
        if (cur && !out.some((o) => o[1] === cur)) out.push([cur + ' (missing)', cur]);
        return out.length ? out : [['(import a sound first)', '']];
      });
    }
    doClassValidator_(v) { return v; }
    static fromJson() { return new SoundField(); }
  }
  Blockly.fieldRegistry.register('field_obj', ObjField);
  Blockly.fieldRegistry.register('field_sound', SoundField);

  const KEYS = [];
  for (const k of 'ABCDEFGHIJKLMNOPQRSTUVWXYZ') KEYS.push([k, k]);
  for (const k of '0123456789') KEYS.push([k, k]);
  KEYS.push(['space', 'SPACE'], ['up arrow', 'UP'], ['down arrow', 'DOWN'], ['left arrow', 'LEFT'], ['right arrow', 'RIGHT'],
    ['enter', 'ENTER'], ['shift', 'SHIFT'], ['ctrl', 'CTRL'], ['escape', 'ESC']);
  const KEYS_FIRST = [['space', 'SPACE'], ['up arrow', 'UP'], ['down arrow', 'DOWN'], ['left arrow', 'LEFT'], ['right arrow', 'RIGHT'],
    ...KEYS.filter((k) => !['SPACE', 'UP', 'DOWN', 'LEFT', 'RIGHT'].includes(k[1]))];

  const AXES = [['x (left / right)', 'X'], ['y (up / down)', 'Y'], ['z (forward / back)', 'Z']];
  const V = (name) => ({ type: 'input_value', name });
  const S = (name) => ({ type: 'input_statement', name });
  const D = (name, options) => ({ type: 'field_dropdown', name, options });
  const T = (name, text) => ({ type: 'field_input', name, text });
  const OBJ = (name, kind) => ({ type: 'field_obj', name, kind });

  function def(type, colour, message, args, o = {}) {
    Blockly.Blocks[type] = {
      init() {
        const j = { type, message0: message, args0: args, colour, tooltip: o.tip || '', inputsInline: true };
        if (o.out !== undefined) j.output = o.out;
        else if (o.hat) j.nextStatement = null;
        else if (o.cap) j.previousStatement = null;
        else { j.previousStatement = null; j.nextStatement = null; }
        this.jsonInit(j);
        if (o.hat) this.hat = 'cap';
      },
    };
  }

  // Events
  def('event_start', C.events, 'when the game starts', [], { hat: true, tip: 'Runs once when the game begins.' });
  def('event_key', C.events, 'when %1 key is pressed', [D('KEY', KEYS_FIRST)], { hat: true, tip: 'Runs each time you press this key.' });
  def('event_key_up', C.events, 'when %1 key is let go', [D('KEY', KEYS_FIRST)], { hat: true });
  def('event_touch', C.events, 'when I touch %1', [OBJ('OBJ', 'any')], { hat: true, tip: 'Runs when this object bumps into another one.' });
  def('event_click', C.events, 'when I am clicked', [], { hat: true, tip: 'Runs when you click this object with the mouse.' });
  def('event_message', C.events, 'when I get the message %1', [T('MSG', 'go')], { hat: true, tip: 'Pairs with "send message".' });
  def('event_timer', C.events, 'every %1 seconds', [V('SECS')], { hat: true, tip: 'Repeats forever on a timer.' });
  def('event_copy', C.events, 'when I start as a copy', [], { hat: true, tip: 'Runs on each new copy made with "create a copy".' });

  // Control
  def('ctl_wait', C.control, 'wait %1 seconds', [V('SECS')]);
  def('ctl_forever', C.control, 'forever %1 %2', [{ type: 'input_dummy' }, S('DO')], { tip: 'Repeat the blocks inside again and again.' });
  Blockly.Blocks['ctl_forever'].init = function () {
    this.jsonInit({ message0: 'forever %1', args0: [S('DO')], previousStatement: null, colour: C.control, tooltip: 'Repeat the blocks inside again and again.' });
  };
  def('ctl_repeat', C.control, 'repeat %1 times %2', [V('TIMES'), S('DO')]);
  def('ctl_if', C.control, 'if %1 then %2', [V('COND'), S('DO')]);
  def('ctl_ifelse', C.control, 'if %1 then %2 else %3', [V('COND'), S('DO'), S('ELSE')]);
  def('ctl_wait_until', C.control, 'wait until %1', [V('COND')]);
  def('ctl_repeat_until', C.control, 'repeat until %1 %2', [V('COND'), S('DO')]);
  def('ctl_broadcast', C.control, 'send message %1', [T('MSG', 'go')], { tip: 'Tell every object something happened.' });
  def('ctl_clone', C.control, 'create a copy of %1', [OBJ('OBJ', 'me')]);
  def('ctl_delete_me', C.control, 'delete me', [], { cap: true });
  def('ctl_stop_all', C.control, 'stop the game', [], { cap: true });

  // Motion
  def('mo_move', C.motion, 'move %1 by %2', [D('DIR', [['forward', 'FORWARD'], ['back', 'BACK'], ['left', 'LEFT'], ['right', 'RIGHT'], ['up', 'UP'], ['down', 'DOWN']]), V('N')],
    { tip: 'Moves in the direction I am facing.' });
  def('mo_change', C.motion, 'change %1 by %2', [D('AXIS', AXES), V('N')], { tip: 'Moves along the world axis.' });
  def('mo_set', C.motion, 'set %1 to %2', [D('AXIS', AXES), V('N')]);
  def('mo_goto', C.motion, 'go to x %1 y %2 z %3', [V('X'), V('Y'), V('Z')]);
  def('mo_turn', C.motion, 'turn %1 %2 degrees', [D('DIR', [['left', 'LEFT'], ['right', 'RIGHT'], ['up', 'UP'], ['down', 'DOWN'], ['roll left', 'ROLL_LEFT'], ['roll right', 'ROLL_RIGHT']]), V('DEG')]);
  def('mo_lookat', C.motion, 'look at %1', [OBJ('OBJ', 'object')]);
  def('mo_size', C.motion, 'set size to %1 %', [V('PCT')]);
  def('mo_glide', C.motion, 'glide for %1 seconds to x %2 y %3 z %4', [V('SECS'), V('X'), V('Y'), V('Z')]);
  def('mo_push', C.motion, 'push by x %1 y %2 z %3', [V('X'), V('Y'), V('Z')], { tip: 'Adds speed. Needs physics.' });
  def('mo_velocity', C.motion, 'set speed to x %1 y %2 z %3', [V('X'), V('Y'), V('Z')], { tip: 'Needs physics.' });

  // Looks
  def('lk_color', C.looks, 'set colour to %1', [V('COLOR')]);
  def('lk_show', C.looks, 'show', []);
  def('lk_hide', C.looks, 'hide', []);
  def('lk_say', C.looks, 'say %1 for %2 seconds', [V('TEXT'), V('SECS')], { tip: 'Shows words on the screen. 0 seconds keeps them there.' });
  def('lk_hud', C.looks, 'show %1 : %2 on screen', [V('LABEL'), V('VALUE')], { tip: 'Good for a score.' });
  def('lk_brightness', C.looks, 'set light brightness to %1 %', [V('N')]);
  def('lk_cam_follow', C.looks, 'camera follows %1 from %2 away, %3 up', [OBJ('OBJ', 'none'), V('DIST'), V('HEIGHT')]);
  def('lk_sky', C.looks, 'set sky colour to %1', [V('COLOR')]);

  // Sound
  def('sd_play', C.sound, 'play sound %1', [{ type: 'field_sound', name: 'FILE' }]);

  // Physics
  def('ph_body', C.physics, 'make me %1', [D('TYPE', [['fall and bump (physics)', 'dynamic'], ['a solid wall that stays put', 'static'], ['moved only by blocks, but solid', 'kinematic'], ['not physical', 'none']])],
    { tip: 'Turns physics on or off for this object.' });
  def('ph_gravity', C.physics, 'set gravity to %1', [V('N')], { tip: '-9.8 is like Earth. 0 floats.' });
  def('ph_bounce', C.physics, 'set bounciness to %1', [V('N')], { tip: '0 is no bounce, 1 bounces forever.' });

  // Sensing
  def('se_key', C.sensing, '%1 key pressed?', [D('KEY', KEYS_FIRST)], { out: 'Boolean' });
  def('se_touching', C.sensing, 'touching %1 ?', [OBJ('OBJ', 'object')], { out: 'Boolean' });
  def('se_distance', C.sensing, 'distance to %1', [OBJ('OBJ', 'object')], { out: null });
  def('se_pos', C.sensing, 'my %1', [D('AXIS', AXES)], { out: null });
  def('se_pos_of', C.sensing, '%1 of %2', [D('AXIS', AXES), OBJ('OBJ', 'object')], { out: null });
  def('se_timer', C.sensing, 'seconds since start', [], { out: null });
  def('se_mouse', C.sensing, 'mouse %1', [D('WHAT', [['left / right (-1 to 1)', 'X'], ['up / down (-1 to 1)', 'Y'], ['button down (0 or 1)', 'DOWN']])], { out: null });

  // Operators
  def('op_arith', C.ops, '%1 %2 %3', [V('A'), D('OP', [['+', 'ADD'], ['-', 'SUB'], ['×', 'MUL'], ['÷', 'DIV'], ['remainder', 'MOD']]), V('B')], { out: null });
  def('op_compare', C.ops, '%1 %2 %3', [V('A'), D('OP', [['=', 'EQ'], ['≠', 'NEQ'], ['<', 'LT'], ['>', 'GT'], ['≤', 'LTE'], ['≥', 'GTE']]), V('B')], { out: 'Boolean' });
  def('op_and', C.ops, '%1 and %2', [V('A'), V('B')], { out: 'Boolean' });
  def('op_or', C.ops, '%1 or %2', [V('A'), V('B')], { out: 'Boolean' });
  def('op_not', C.ops, 'not %1', [V('A')], { out: 'Boolean' });
  def('op_random', C.ops, 'pick random %1 to %2', [V('A'), V('B')], { out: null });
  def('op_fn', C.ops, '%1 of %2', [D('FN', [['absolute', 'ABS'], ['square root', 'SQRT'], ['sin', 'SIN'], ['cos', 'COS'], ['round', 'ROUND'], ['round down', 'FLOOR'], ['round up', 'CEIL']]), V('A')], { out: null });
  def('op_join', C.ops, 'join %1 %2', [V('A'), V('B')], { out: null });

  // Lists
  const LV = (d) => ({ type: 'field_variable', name: 'VAR', variable: d || 'my list' });
  def('list_add', C.lists, 'add %1 to %2', [V('ITEM'), LV()]);
  def('list_remove', C.lists, 'delete item %1 of %2', [V('INDEX'), LV()]);
  def('list_clear', C.lists, 'delete all of %1', [LV()]);
  def('list_get', C.lists, 'item %1 of %2', [V('INDEX'), LV()], { out: null });
  def('list_length', C.lists, 'length of %1', [LV()], { out: null });
  def('list_has', C.lists, '%1 contains %2', [LV(), V('ITEM')], { out: 'Boolean' });

  // Toolbox
  const shadow = (v) => {
    if (typeof v === 'number') return { shadow: { type: 'math_number', fields: { NUM: v } } };
    if (typeof v === 'string') return { shadow: { type: 'text', fields: { TEXT: v } } };
    return v;
  };
  const colour = (hex) => ({ shadow: { type: 'colour_picker', fields: { COLOUR: hex } } });
  const b = (type, inputs, fields) => {
    const o = { kind: 'block', type };
    if (inputs) { o.inputs = {}; for (const k in inputs) o.inputs[k] = shadow(inputs[k]); }
    if (fields) o.fields = fields;
    return o;
  };
  const cat = (name, c, contents) => ({ kind: 'category', name, colour: c, contents });
  window.ZENG_TOOLBOX = {
    kind: 'categoryToolbox',
    contents: [
      cat('Events', C.events, [b('event_start'), b('event_key', null, { KEY: 'SPACE' }), b('event_key_up', null, { KEY: 'SPACE' }), b('event_touch'), b('event_click'),
        b('event_message'), b('event_timer', { SECS: 2 }), b('event_copy')]),
      cat('Control', C.control, [b('ctl_wait', { SECS: 1 }), b('ctl_forever'), b('ctl_repeat', { TIMES: 10 }), b('ctl_if'), b('ctl_ifelse'),
        b('ctl_wait_until'), b('ctl_repeat_until'), b('ctl_broadcast'), b('ctl_clone'), b('ctl_delete_me'), b('ctl_stop_all')]),
      cat('Motion', C.motion, [b('mo_move', { N: 1 }), b('mo_turn', { DEG: 15 }, { DIR: 'LEFT' }), b('mo_change', { N: 1 }, { AXIS: 'X' }), b('mo_set', { N: 0 }, { AXIS: 'Y' }),
        b('mo_goto', { X: 0, Y: 1, Z: 0 }), b('mo_glide', { SECS: 1, X: 0, Y: 1, Z: 0 }), b('mo_lookat'), b('mo_size', { PCT: 100 }),
        b('mo_push', { X: 0, Y: 5, Z: 0 }), b('mo_velocity', { X: 0, Y: 0, Z: -5 })]),
      cat('Looks', C.looks, [b('lk_color', { COLOR: colour('#ff6b35') }), b('lk_show'), b('lk_hide'), b('lk_say', { TEXT: 'Hello!', SECS: 2 }),
        b('lk_hud', { LABEL: 'Score', VALUE: 0 }), b('lk_brightness', { N: 100 }), b('lk_cam_follow', { DIST: 8, HEIGHT: 5 }), b('lk_sky', { COLOR: colour('#87b6ff') })]),
      cat('Sound', C.sound, [b('sd_play')]),
      cat('Physics', C.physics, [b('ph_body', null, { TYPE: 'dynamic' }), b('ph_gravity', { N: -9.8 }), b('ph_bounce', { N: 0.7 })]),
      cat('Sensing', C.sensing, [b('se_key', null, { KEY: 'SPACE' }), b('se_touching'), b('se_distance'), b('se_pos'), b('se_pos_of'), b('se_timer'), b('se_mouse')]),
      cat('Operators', C.ops, [b('op_arith', { A: 1, B: 1 }), b('op_compare', { A: 1, B: 1 }), b('op_and'), b('op_or'), b('op_not'), b('op_random', { A: 1, B: 10 }),
        b('op_fn', { A: 9 }), b('op_join', { A: 'Hello ', B: 'world' }), b('math_number'), b('text'), b('logic_boolean')]),
      { kind: 'category', name: 'Variables', colour: C.vars, custom: 'VARIABLE' },
      cat('Lists', C.lists, [b('list_add', { ITEM: 'thing' }), b('list_remove', { INDEX: 1 }), b('list_clear'), b('list_get', { INDEX: 1 }), b('list_length'), b('list_has', { ITEM: 'thing' })]),
      { kind: 'category', name: 'Functions', colour: C.funcs, custom: 'PROCEDURE' },
    ],
  };
})();
