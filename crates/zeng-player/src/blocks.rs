//! Compiles a Blockly workspace (its native JSON serialization) into a small AST the VM runs.
use crate::value::Value;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::sync::Arc;

pub type Body = Arc<Vec<Stmt>>;

#[derive(Debug)]
pub struct Stmt {
    pub id: Arc<str>,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Value),
    Var(String),
    Arith(char, Box<Expr>, Box<Expr>),
    Cmp(String, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Rand(Box<Expr>, Box<Expr>),
    Fn(String, Box<Expr>),
    Join(Box<Expr>, Box<Expr>),
    Key(String),
    Pos(String),
    Dist(String),
    Touching(String),
    Timer,
    PosOf(String, String),
    ListGet(String, Box<Expr>),
    ListLen(String),
    ListHas(String, Box<Expr>),
    Call(String, Vec<Expr>),
    Mouse(String),
}

#[derive(Debug)]
pub enum Kind {
    Wait(Expr),
    Forever(Body),
    Repeat(Expr, Body),
    If(Expr, Body, Body),
    WaitUntil(Expr),
    RepeatUntil(Expr, Body),
    Broadcast(String),
    DeleteMe,
    CloneObj(String),
    StopAll,
    Move(String, Expr),
    Change(String, Expr),
    Set(String, Expr),
    Goto(Expr, Expr, Expr),
    Turn(String, Expr),
    LookAt(String),
    Size(Expr),
    Glide(Expr, Expr, Expr, Expr),
    Push(Expr, Expr, Expr),
    Velocity(Expr, Expr, Expr),
    Color(String),
    Show(bool),
    Say(Expr, Expr),
    Hud(Expr, Expr),
    Brightness(Expr),
    CamFollow(String, Expr, Expr),
    Sky(String),
    Sound(String),
    PhysBody(String),
    Gravity(Expr),
    Bounce(Expr),
    SetVar(String, Expr),
    ChangeVar(String, Expr),
    ListAdd(String, Expr),
    ListRemove(String, Expr),
    ListClear(String),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HatKind {
    Start,
    Key(String),
    KeyUp(String),
    Message(String),
    Touch(String),
    Timer(f64),
    Copy,
    Click,
}

#[derive(Debug)]
pub struct Hat {
    pub kind: HatKind,
    pub body: Body,
}

#[derive(Debug)]
pub struct Proc {
    pub params: Vec<String>,
    pub body: Body,
    pub ret: Option<Expr>,
}

#[derive(Debug, Default)]
pub struct Script {
    pub hats: Vec<Hat>,
    pub procs: HashMap<String, Proc>,
}

pub struct Compiler {
    vars: HashMap<String, String>,
    pub errors: Vec<(String, String)>,
}

fn s<'a>(b: &'a Json, field: &str) -> &'a str {
    b["fields"][field].as_str().unwrap_or("")
}

pub fn compile(src: &str) -> Result<(Script, Vec<(String, String)>), String> {
    let root: Json = serde_json::from_str(src).map_err(|e| format!("bad blocks file: {e}"))?;
    let mut c = Compiler { vars: HashMap::new(), errors: vec![] };
    if let Some(vs) = root["variables"].as_array() {
        for v in vs {
            if let (Some(id), Some(name)) = (v["id"].as_str(), v["name"].as_str()) {
                c.vars.insert(id.to_string(), name.to_string());
            }
        }
    }
    let mut script = Script::default();
    let tops = root["blocks"]["blocks"].as_array().cloned().unwrap_or_default();
    for top in &tops {
        c.top(top, &mut script);
    }
    Ok((script, c.errors))
}

impl Compiler {
    fn err(&mut self, b: &Json, msg: String) {
        self.errors.push((b["id"].as_str().unwrap_or("").to_string(), msg));
    }

    fn var_name(&self, b: &Json) -> String {
        let f = &b["fields"]["VAR"];
        if let Some(id) = f["id"].as_str() {
            return self.vars.get(id).cloned().unwrap_or_else(|| id.to_string());
        }
        f.as_str().unwrap_or("").to_string()
    }

    fn top(&mut self, b: &Json, script: &mut Script) {
        let ty = b["type"].as_str().unwrap_or("");
        let hat = match ty {
            "event_start" => Some(HatKind::Start),
            "event_key" => Some(HatKind::Key(s(b, "KEY").to_string())),
            "event_key_up" => Some(HatKind::KeyUp(s(b, "KEY").to_string())),
            "event_message" => Some(HatKind::Message(s(b, "MSG").to_string())),
            "event_touch" => Some(HatKind::Touch(s(b, "OBJ").to_string())),
            "event_timer" => Some(HatKind::Timer(self.expr_lit(b, "SECS").max(0.05))),
            "event_copy" => Some(HatKind::Copy),
            "event_click" => Some(HatKind::Click),
            _ => None,
        };
        if let Some(kind) = hat {
            let body = self.chain(&b["next"]["block"]);
            script.hats.push(Hat { kind, body });
            return;
        }
        if ty == "procedures_defnoreturn" || ty == "procedures_defreturn" {
            let name = s(b, "NAME").to_string();
            let params = b["extraState"]["params"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["name"].as_str().map(String::from)).collect())
                .unwrap_or_default();
            let body = self.chain(&b["inputs"]["STACK"]["block"]);
            let ret = if ty == "procedures_defreturn" { Some(self.expr(b, "RETURN")) } else { None };
            script.procs.insert(name, Proc { params, body, ret });
        }
        // Loose blocks that are not attached to a hat never run.
    }

    fn expr_lit(&mut self, b: &Json, input: &str) -> f64 {
        match self.expr(b, input) {
            Expr::Lit(v) => v.num(),
            _ => 1.0,
        }
    }

    fn chain(&mut self, first: &Json) -> Body {
        let mut out = vec![];
        let mut cur = first;
        while cur.is_object() {
            if let Some(kind) = self.stmt(cur) {
                let id: Arc<str> = cur["id"].as_str().unwrap_or("").into();
                out.push(Stmt { id, kind });
            }
            cur = &cur["next"]["block"];
        }
        Arc::new(out)
    }

    fn sub(&mut self, b: &Json, input: &str) -> Body {
        self.chain(&b["inputs"][input]["block"])
    }

    fn expr(&mut self, b: &Json, input: &str) -> Expr {
        let inp = &b["inputs"][input];
        let blk = if inp["block"].is_object() { &inp["block"] } else { &inp["shadow"] };
        if !blk.is_object() {
            return Expr::Lit(Value::Num(0.0));
        }
        self.expr_block(blk)
    }

    fn bx(&mut self, b: &Json, input: &str) -> Box<Expr> {
        Box::new(self.expr(b, input))
    }

    fn expr_block(&mut self, b: &Json) -> Expr {
        let ty = b["type"].as_str().unwrap_or("");
        match ty {
            "math_number" => Expr::Lit(Value::Num(b["fields"]["NUM"].as_f64().unwrap_or(0.0))),
            "text" => Expr::Lit(Value::Str(s(b, "TEXT").to_string())),
            "logic_boolean" => Expr::Lit(Value::Bool(s(b, "BOOL") == "TRUE")),
            "colour_picker" => Expr::Lit(Value::Str(s(b, "COLOUR").to_string())),
            "variables_get" => Expr::Var(self.var_name(b)),
            "op_arith" => {
                let op = match s(b, "OP") { "SUB" => '-', "MUL" => '*', "DIV" => '/', "MOD" => '%', _ => '+' };
                Expr::Arith(op, self.bx(b, "A"), self.bx(b, "B"))
            }
            "op_compare" => Expr::Cmp(s(b, "OP").to_string(), self.bx(b, "A"), self.bx(b, "B")),
            "op_and" => Expr::And(self.bx(b, "A"), self.bx(b, "B")),
            "op_or" => Expr::Or(self.bx(b, "A"), self.bx(b, "B")),
            "op_not" => Expr::Not(self.bx(b, "A")),
            "op_random" => Expr::Rand(self.bx(b, "A"), self.bx(b, "B")),
            "op_fn" => Expr::Fn(s(b, "FN").to_string(), self.bx(b, "A")),
            "op_join" => Expr::Join(self.bx(b, "A"), self.bx(b, "B")),
            "se_key" => Expr::Key(s(b, "KEY").to_string()),
            "se_pos" => Expr::Pos(s(b, "AXIS").to_string()),
            "se_distance" => Expr::Dist(s(b, "OBJ").to_string()),
            "se_touching" => Expr::Touching(s(b, "OBJ").to_string()),
            "se_timer" => Expr::Timer,
            "se_pos_of" => Expr::PosOf(s(b, "AXIS").to_string(), s(b, "OBJ").to_string()),
            "se_mouse" => Expr::Mouse(s(b, "WHAT").to_string()),
            "list_get" => Expr::ListGet(self.var_name(b), self.bx(b, "INDEX")),
            "list_length" => Expr::ListLen(self.var_name(b)),
            "list_has" => Expr::ListHas(self.var_name(b), self.bx(b, "ITEM")),
            "procedures_callreturn" => Expr::Call(self.call_name(b), self.call_args(b)),
            _ => {
                self.err(b, format!("I don't know the block \"{ty}\""));
                Expr::Lit(Value::Num(0.0))
            }
        }
    }

    fn call_name(&self, b: &Json) -> String {
        b["extraState"]["name"].as_str().unwrap_or("").to_string()
    }

    fn call_args(&mut self, b: &Json) -> Vec<Expr> {
        let n = b["extraState"]["params"].as_array().map_or(0, |a| a.len());
        (0..n).map(|i| self.expr(b, &format!("ARG{i}"))).collect()
    }

    fn stmt(&mut self, b: &Json) -> Option<Kind> {
        let ty = b["type"].as_str().unwrap_or("");
        let o = |x: &str| s(b, x).to_string();
        Some(match ty {
            "ctl_wait" => Kind::Wait(self.expr(b, "SECS")),
            "ctl_forever" => Kind::Forever(self.sub(b, "DO")),
            "ctl_repeat" => Kind::Repeat(self.expr(b, "TIMES"), self.sub(b, "DO")),
            "ctl_if" => Kind::If(self.expr(b, "COND"), self.sub(b, "DO"), Arc::new(vec![])),
            "ctl_ifelse" => Kind::If(self.expr(b, "COND"), self.sub(b, "DO"), self.sub(b, "ELSE")),
            "ctl_wait_until" => Kind::WaitUntil(self.expr(b, "COND")),
            "ctl_repeat_until" => Kind::RepeatUntil(self.expr(b, "COND"), self.sub(b, "DO")),
            "ctl_broadcast" => Kind::Broadcast(o("MSG")),
            "ctl_delete_me" => Kind::DeleteMe,
            "ctl_clone" => Kind::CloneObj(o("OBJ")),
            "ctl_stop_all" => Kind::StopAll,
            "mo_move" => Kind::Move(o("DIR"), self.expr(b, "N")),
            "mo_change" => Kind::Change(o("AXIS"), self.expr(b, "N")),
            "mo_set" => Kind::Set(o("AXIS"), self.expr(b, "N")),
            "mo_goto" => Kind::Goto(self.expr(b, "X"), self.expr(b, "Y"), self.expr(b, "Z")),
            "mo_turn" => Kind::Turn(o("DIR"), self.expr(b, "DEG")),
            "mo_lookat" => Kind::LookAt(o("OBJ")),
            "mo_size" => Kind::Size(self.expr(b, "PCT")),
            "mo_glide" => Kind::Glide(self.expr(b, "SECS"), self.expr(b, "X"), self.expr(b, "Y"), self.expr(b, "Z")),
            "mo_push" => Kind::Push(self.expr(b, "X"), self.expr(b, "Y"), self.expr(b, "Z")),
            "mo_velocity" => Kind::Velocity(self.expr(b, "X"), self.expr(b, "Y"), self.expr(b, "Z")),
            "lk_color" => {
                let e = self.expr(b, "COLOR");
                match e {
                    Expr::Lit(v) => Kind::Color(v.to_string()),
                    _ => Kind::Color(String::new()),
                }
            }
            "lk_show" => Kind::Show(true),
            "lk_hide" => Kind::Show(false),
            "lk_say" => Kind::Say(self.expr(b, "TEXT"), self.expr(b, "SECS")),
            "lk_hud" => Kind::Hud(self.expr(b, "LABEL"), self.expr(b, "VALUE")),
            "lk_brightness" => Kind::Brightness(self.expr(b, "N")),
            "lk_cam_follow" => Kind::CamFollow(o("OBJ"), self.expr(b, "DIST"), self.expr(b, "HEIGHT")),
            "lk_sky" => match self.expr(b, "COLOR") {
                Expr::Lit(v) => Kind::Sky(v.to_string()),
                _ => Kind::Sky(String::new()),
            },
            "sd_play" => Kind::Sound(o("FILE")),
            "ph_body" => Kind::PhysBody(o("TYPE")),
            "ph_gravity" => Kind::Gravity(self.expr(b, "N")),
            "ph_bounce" => Kind::Bounce(self.expr(b, "N")),
            "variables_set" => Kind::SetVar(self.var_name(b), self.expr(b, "VALUE")),
            "math_change" => Kind::ChangeVar(self.var_name(b), self.expr(b, "DELTA")),
            "list_add" => Kind::ListAdd(self.var_name(b), self.expr(b, "ITEM")),
            "list_remove" => Kind::ListRemove(self.var_name(b), self.expr(b, "INDEX")),
            "list_clear" => Kind::ListClear(self.var_name(b)),
            "procedures_callnoreturn" => Kind::Call(self.call_name(b), self.call_args(b)),
            _ => {
                self.err(b, format!("I don't know the block \"{ty}\""));
                return None;
            }
        })
    }
}
