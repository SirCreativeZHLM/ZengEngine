#!/usr/bin/env python3
"""Generates templates/* (starter games) with Blockly-format scripts. Run from the repo root."""
import json, math, os, struct, wave, itertools

_ids = itertools.count(1)


def blk(t, fields=None, inputs=None, nxt=None, extra=None):
    b = {"type": t, "id": f"b{next(_ids)}"}
    if fields: b["fields"] = fields
    if inputs:
        b["inputs"] = {k: wrap(v) for k, v in inputs.items()}
    if nxt: b["next"] = {"block": nxt}
    if extra: b.update(extra)
    return b


def wrap(v):
    if isinstance(v, bool): return {"shadow": {"type": "logic_boolean", "fields": {"BOOL": "TRUE" if v else "FALSE"}}}
    if isinstance(v, (int, float)): return {"shadow": {"type": "math_number", "fields": {"NUM": v}}}
    if isinstance(v, str): return {"shadow": {"type": "text", "fields": {"TEXT": v}}}
    if isinstance(v, dict) and "type" in v and "block" not in v and "shadow" not in v: return {"block": v}
    return v


def colour(hexv): return {"shadow": {"type": "colour_picker", "fields": {"COLOUR": hexv}}}


def seq(*bs):
    bs = [b for b in bs if b]
    for a, b in zip(bs, bs[1:]): a["next"] = {"block": b}
    return bs[0]


def stack(*bs): return {"block": seq(*bs)}
def var(name): return blk("variables_get", {"VAR": {"id": "v_" + name}})
def setvar(name, v): return blk("variables_set", {"VAR": {"id": "v_" + name}}, {"VALUE": v})
def chg(name, v): return blk("math_change", {"VAR": {"id": "v_" + name}}, {"DELTA": v})
def hud(label, v): return blk("lk_hud", None, {"LABEL": label, "VALUE": v})
def keyif(k, *body): return blk("ctl_if", None, {"COND": blk("se_key", {"KEY": k}), "DO": stack(*body)})


def script(*tops, vars=()):
    return {"blocks": {"languageVersion": 0, "blocks": [dict(t, x=20, y=20 + 220 * i) for i, t in enumerate(tops)]},
            "variables": [{"name": n, "id": "v_" + n} for n in vars]}


def entity(name, pos=(0, 0, 0), shape=None, mat=None, **kw):
    e = {"name": name, "transform": {"position": list(pos), "rotation": kw.pop("rot", [0, 0, 0]), "scale": kw.pop("scale", [1, 1, 1])}}
    if shape: e["shape"] = shape
    if mat: e["material"] = mat
    e.update(kw)
    return e


def mat(c, metallic=0.0, rough=0.5, emissive=(0, 0, 0)):
    return {"color": list(c), "metallic": metallic, "roughness": rough, "emissive": list(emissive)}


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f: f.write(text)


def tone(path, freqs, dur=0.18):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    rate = 22050
    n = int(rate * dur)
    with wave.open(path, "w") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
        frames = bytearray()
        for i in range(n):
            t = i / rate
            f = freqs[min(len(freqs) - 1, int(i / n * len(freqs)))]
            amp = math.exp(-5 * t / dur) * 0.4
            frames += struct.pack("<h", int(32767 * amp * math.sin(2 * math.pi * f * t)))
        w.writeframes(bytes(frames))


def make(dirname, title, blurb, order, entities, scripts_map, env=None, sounds=None):
    root = os.path.join("templates", dirname)
    write(f"{root}/template.json", json.dumps({"title": title, "blurb": blurb, "order": order}, indent=2))
    write(f"{root}/project.json", json.dumps({"name": title, "version": "0.1.0", "main_scene": "scenes/main.scene.json", "quality": "medium"}, indent=2))
    write(f"{root}/scenes/main.scene.json", json.dumps({"environment": env or {}, "entities": entities}, indent=2))
    for rel, s in scripts_map.items(): write(f"{root}/{rel}", json.dumps(s, indent=1))
    for name, freqs in (sounds or {}).items(): tone(f"{root}/assets/sounds/{name}", freqs)


SUN = lambda: entity("Sun", (6, 10, 5), light={"type": "directional", "illuminance": 11000, "shadows": True}, **{}) | {"transform": {"position": [6, 10, 5], "rotation": [0, 0, 0], "scale": [1, 1, 1], "look_at": [0, 0, 0]}}
FLOOR = lambda c=(0.36, 0.55, 0.38): entity("Floor", (0, -0.5, 0), {"type": "cube", "size": 1}, mat(c, 0, 0.9), scale=[30, 1, 30], physics={"body": "static", "bounce": 0.3, "friction": 0.8})
ENV = {"sky": [0.5, 0.7, 0.95], "ambient": 500, "gravity": -9.81, "bloom": True}

# ---- empty ----
make("empty", "Blank World", "A floor, a light and one cube. Start from nothing.", 3,
     [FLOOR(), SUN(), entity("Cube", (0, 0.5, 0), {"type": "cube", "size": 1}, mat((0.9, 0.35, 0.25)))], {}, ENV)

# ---- coin collector ----
player = script(
    blk("event_start", None, None, seq(
        setvar("score", 0), hud("Score", var("score")),
        blk("lk_cam_follow", {"OBJ": "Player"}, {"DIST": 9, "HEIGHT": 7}),
        blk("ctl_forever", None, {"DO": stack(
            keyif("W", blk("mo_move", {"DIR": "FORWARD"}, {"N": 0.12})),
            keyif("S", blk("mo_move", {"DIR": "BACK"}, {"N": 0.08})),
            keyif("A", blk("mo_turn", {"DIR": "LEFT"}, {"DEG": 3})),
            keyif("D", blk("mo_turn", {"DIR": "RIGHT"}, {"DEG": 3})))}))),
    vars=["score"])
coin = lambda: script(
    blk("event_start", None, None, blk("ctl_forever", None, {"DO": stack(blk("mo_turn", {"DIR": "LEFT"}, {"DEG": 3}))})),
    blk("event_touch", {"OBJ": "Player"}, None, seq(
        chg("score", 1), hud("Score", var("score")), blk("sd_play", {"FILE": "sounds/pop.wav"}),
        blk("ctl_if", None, {"COND": blk("op_compare", {"OP": "GTE"}, {"A": var("score"), "B": 6}),
                              "DO": stack(blk("lk_say", None, {"TEXT": "You got them all!", "SECS": 0}), blk("sd_play", {"FILE": "sounds/win.wav"}))}),
        blk("ctl_delete_me"))),
    vars=["score"])
coins = [(4, 0), (-5, 3), (2, -6), (-3, -5), (7, 5), (-7, -2)]
ents = [FLOOR(), SUN(),
        entity("Player", (0, 0.5, 0), {"type": "cube", "size": 1}, mat((0.95, 0.4, 0.2), 0.1, 0.4), script="scripts/Player.blocks.json")]
files = {"scripts/Player.blocks.json": player}
for i, (x, z) in enumerate(coins, 1):
    ents.append(entity(f"Coin {i}", (x, 0.8, z), {"type": "cylinder", "radius": 0.4, "height": 0.12}, mat((1, 0.8, 0.1), 0.9, 0.3, (0.5, 0.35, 0)),
                       rot=[0, 90, 0], script=f"scripts/Coin {i}.blocks.json"))
    files[f"scripts/Coin {i}.blocks.json"] = coin()
for i, (x, z, h) in enumerate([(-9, 0, 2), (9, -4, 3), (0, 9, 2.5)], 1):
    ents.append(entity(f"Pillar {i}", (x, h / 2 - 0.5 + 0.5, z), {"type": "cylinder", "radius": 0.8, "height": h}, mat((0.6, 0.6, 0.7), 0.1, 0.7), physics={"body": "static", "bounce": 0.2, "friction": 0.6}))
make("coin-collector", "Coin Collector", "Walk around with W A S D and grab every coin. Teaches keys, score and touching.", 1, ents, files, ENV,
     {"pop.wav": [660, 880], "win.wav": [523, 659, 784, 1046]})

# ---- bouncy balls ----
spawner = script(
    blk("event_start", None, None, blk("ctl_forever", None, {"DO": stack(
        blk("mo_goto", None, {"X": blk("op_random", None, {"A": -5, "B": 5}), "Y": 9, "Z": blk("op_random", None, {"A": -5, "B": 5})}),
        blk("ctl_clone", {"OBJ": "_ME_"}),
        blk("ctl_wait", None, {"SECS": 0.4}))})),
    blk("event_copy", None, None, seq(
        blk("ph_body", {"TYPE": "dynamic"}), blk("ph_bounce", None, {"N": 0.85}),
        blk("ctl_wait", None, {"SECS": 10}), blk("ctl_delete_me"))))
make("bouncy-balls", "Bouncy Balls", "Balls rain down and bounce. Teaches timers, copies and physics.", 2,
     [FLOOR((0.3, 0.35, 0.5)), SUN(),
      entity("Ball", (0, 9, 0), {"type": "sphere", "radius": 0.5}, mat((0.95, 0.45, 0.2), 0.4, 0.25), script="scripts/Ball.blocks.json",
             physics={"body": "kinematic", "bounce": 0.8, "friction": 0.4}),
      entity("Ramp", (0, 1, 0), {"type": "cube", "size": 1}, mat((0.8, 0.8, 0.85), 0.2, 0.5), rot=[0, 0, 20], scale=[6, 0.3, 4], physics={"body": "static", "bounce": 0.6, "friction": 0.3})],
     {"scripts/Ball.blocks.json": spawner}, ENV)
print("templates written")
