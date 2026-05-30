//! The Pikchr rendering context (`Pik`) and the semantic actions invoked by
//! the grammar. Faithful port of the layout/render machinery in pikchr.y.
//!
//! By design (matching upstream) object geometry is computed during parsing:
//! the grammar actions mutate this context and append objects immediately,
//! then [`Pik::render`] walks the finished list to emit SVG.

use crate::error::PikchrError;
use crate::obj::{prop, Class, PBox, PObj, PPoint, PRel, PText};
use crate::token::{cp, AssignOp, Tok};
use crate::value;

const DEG2RAD: f64 = 0.017453292519943295769;

/// Directions (kept local for readability).
mod dir {
    pub const RIGHT: i32 = 0;
    pub const DOWN: i32 = 1;
    pub const LEFT: i32 = 2;
    pub const UP: i32 = 3;
}

/// Text-position flags (`TP_*`).
pub mod tp {
    pub const LJUST: i32 = 0x0001;
    pub const RJUST: i32 = 0x0002;
    pub const JMASK: i32 = 0x0003;
    pub const ABOVE2: i32 = 0x0004;
    pub const ABOVE: i32 = 0x0008;
    pub const CENTER: i32 = 0x0010;
    pub const BELOW: i32 = 0x0020;
    pub const BELOW2: i32 = 0x0040;
    pub const VMASK: i32 = 0x007c;
    pub const BIG: i32 = 0x0100;
    pub const SMALL: i32 = 0x0200;
    pub const XTRA: i32 = 0x0400;
    pub const SZMASK: i32 = 0x0700;
    pub const ITALIC: i32 = 0x1000;
    pub const BOLD: i32 = 0x2000;
    pub const MONO: i32 = 0x4000;
    pub const ALIGN: i32 = 0x8000;
}

/// `pik_text_position`: fold a text-position keyword into the flag set.
pub fn text_position(prev: i32, kw: crate::token::Kw) -> i32 {
    use crate::token::Kw;
    let mut r = prev;
    match kw {
        Kw::Ljust => r = (r & !tp::JMASK) | tp::LJUST,
        Kw::Rjust => r = (r & !tp::JMASK) | tp::RJUST,
        Kw::Above => r = (r & !tp::VMASK) | tp::ABOVE,
        Kw::Center => r = (r & !tp::VMASK) | tp::CENTER,
        Kw::Below => r = (r & !tp::VMASK) | tp::BELOW,
        Kw::Italic => r |= tp::ITALIC,
        Kw::Bold => r |= tp::BOLD,
        Kw::Mono => r |= tp::MONO,
        Kw::Aligned => r |= tp::ALIGN,
        Kw::Big => {
            if r & tp::BIG != 0 {
                r |= tp::XTRA;
            } else {
                r = (r & !tp::SZMASK) | tp::BIG;
            }
        }
        Kw::Small => {
            if r & tp::SMALL != 0 {
                r |= tp::XTRA;
            } else {
                r = (r & !tp::SZMASK) | tp::SMALL;
            }
        }
        _ => {}
    }
    r
}

/// Emulate C `printf("%g")` (default precision 6): shortest of %e/%f, trailing
/// zeros trimmed. Used so SVG numbers match upstream closely.
pub fn fmt_g(v: f64) -> String {
    fmt_g_prec(v, 6)
}

/// `%.10g` — used by `pik_append_num` (rotate angle, font-size %, etc.).
pub fn fmt_num(v: f64) -> String {
    fmt_g_prec(v, 10)
}

/// Emulate C `printf("%.*g", p, v)`.
pub fn fmt_g_prec(v: f64, mut p: i32) -> String {
    if p <= 0 {
        p = 1;
    }
    if v == 0.0 {
        // Preserve negative zero like C's printf ("-0").
        return if v.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    if !v.is_finite() {
        return if v.is_nan() {
            "nan".to_string()
        } else if v > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    let exp = v.abs().log10().floor() as i32;
    if exp < -4 || exp >= p {
        // %e style with (p-1) digits after the point, trimmed.
        let prec = (p - 1) as usize;
        let s = format!("{:.*e}", prec, v);
        trim_e(&s)
    } else {
        // %f style with (p-1-exp) digits after the point, trimmed.
        let prec = (p - 1 - exp).max(0) as usize;
        let s = format!("{:.*}", prec, v);
        trim_f(&s)
    }
}

fn trim_f(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

fn trim_e(s: &str) -> String {
    // Rust formats exponent as "1.5e2"; C as "1.5e+02". Normalize mantissa
    // trimming and exponent sign/width to match C.
    let (mant, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e),
        None => return s.to_string(),
    };
    let mant = trim_f(mant);
    let (sign, digits) = if let Some(rest) = exp.strip_prefix('-') {
        ('-', rest)
    } else {
        ('+', exp.trim_start_matches('+'))
    };
    format!("{mant}e{sign}{:0>2}", digits)
}

/// The complete parse + render context.
pub struct Pik {
    /// Arena of every object created. Objects are referenced by index.
    pub objects: Vec<PObj>,
    /// The object list currently being built (indices into `objects`).
    pub list: Vec<usize>,
    /// Saved lists for `[...]` sublists.
    list_stack: Vec<Vec<usize>>,
    /// Script-defined variables (most-recent first), overriding builtins.
    vars: Vec<(String, f64)>,
    /// Current layout direction.
    e_dir: i32,
    /// Index of the object under construction.
    cur: Option<usize>,
    /// Output accumulator.
    out: String,
    /// First error seen (processing stops after an error).
    pub err: Option<PikchrError>,
    flags_dark: bool,

    bbox: PBox,
    r_scale: f64,
    font_scale: f64,
    char_width: f64,
    char_height: f64,
    w_arrow: f64,
    h_arrow: f64,
    layout_vars_done: bool,
    fgcolor: i32,
    bgcolor: i32,

    // Path under construction for line-oriented objects.
    a_tpath: Vec<PPoint>,
    n_tpath: usize,
    m_tpath: i32,
    then_flag: bool,
    same_path: bool,
    /// Last object resolved by name/reference (`p->lastRef`), used to find the
    /// concrete objects a line connects for `chop`.
    last_ref: Option<usize>,

    src: String,
}

impl Pik {
    pub fn new(src: &str, dark_mode: bool) -> Self {
        Pik {
            objects: Vec::new(),
            list: Vec::new(),
            list_stack: Vec::new(),
            vars: Vec::new(),
            e_dir: dir::RIGHT,
            cur: None,
            out: String::new(),
            err: None,
            flags_dark: dark_mode,
            bbox: PBox::init(),
            r_scale: 144.0,
            font_scale: 1.0,
            char_width: 0.0,
            char_height: 0.0,
            w_arrow: 0.0,
            h_arrow: 0.0,
            layout_vars_done: false,
            fgcolor: 0,
            bgcolor: 0xffffff,
            a_tpath: vec![PPoint::default(); 1000],
            n_tpath: 0,
            m_tpath: 0,
            then_flag: false,
            same_path: false,
            last_ref: None,
            src: src.to_string(),
        }
    }

    // ----- error handling ------------------------------------------------

    pub fn error(&mut self, span: Option<(usize, usize)>, msg: &str) {
        if self.err.is_some() {
            return;
        }
        let (line, col) = match span {
            Some((start, _)) => self.line_col(start),
            None => (1, 1),
        };
        self.err = Some(PikchrError::new(msg, line, col));
    }

    fn line_col(&self, byte: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, c) in self.src.char_indices() {
            if i >= byte {
                break;
            }
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    pub fn has_err(&self) -> bool {
        self.err.is_some()
    }

    // ----- variable / value lookup --------------------------------------

    /// `pik_value`: variables override builtins. Returns (value, found).
    pub fn value_miss(&self, name: &str) -> (f64, bool) {
        for (n, v) in &self.vars {
            if n == name {
                return (*v, false);
            }
        }
        match value::builtin(name) {
            Some(v) => (v, false),
            None => (0.0, true),
        }
    }

    pub fn value(&self, name: &str) -> f64 {
        self.value_miss(name).0
    }

    /// `pik_get_var`: variable, then builtin, then color name; else error.
    pub fn get_var(&mut self, t: &Tok) -> f64 {
        let (v, miss) = self.value_miss(&t.text);
        if !miss {
            return v;
        }
        let c = value::lookup_color(&t.text);
        if c > -90.0 {
            return c;
        }
        self.error(Some((t.start, t.end)), "no such variable");
        0.0
    }

    pub fn lookup_color(&mut self, t: &Tok) -> f64 {
        let v = value::lookup_color(&t.text);
        if v <= -90.0 {
            self.error(Some((t.start, t.end)), "not a known color name");
            return 0.0;
        }
        v
    }

    /// `pik_set_var`: assign a variable, honoring compound operators.
    pub fn set_var(&mut self, name: &Tok, val: f64, op: AssignOp) {
        let newval = match op {
            AssignOp::Set => val,
            other => {
                let cur = self.value(&name.text);
                match other {
                    AssignOp::Plus => cur + val,
                    AssignOp::Minus => cur - val,
                    AssignOp::Star => cur * val,
                    AssignOp::Slash => {
                        if val == 0.0 {
                            self.error(Some((name.start, name.end)), "division by zero");
                            return;
                        }
                        cur / val
                    }
                    AssignOp::Set => val,
                }
            }
        };
        for (n, v) in &mut self.vars {
            if *n == name.text {
                *v = newval;
                return;
            }
        }
        self.vars.insert(0, (name.text.clone(), newval));
        self.layout_vars_done = false;
    }

    // ----- builtin functions (pik_func) ---------------------------------

    pub fn func(&mut self, f: &Tok, a: f64, b: f64) -> f64 {
        use crate::token::fnc::*;
        match f.e_code {
            ABS => a.abs(),
            COS => a.cos(),
            INT => a.trunc(),
            SIN => a.sin(),
            SQRT => {
                if a < 0.0 {
                    self.error(Some((f.start, f.end)), "sqrt of negative value");
                    0.0
                } else {
                    a.sqrt()
                }
            }
            MAX => a.max(b),
            MIN => a.min(b),
            _ => 0.0,
        }
    }

    pub fn dist(a: PPoint, b: PPoint) -> f64 {
        (b.x - a.x).hypot(b.y - a.y)
    }

    // ----- object construction ------------------------------------------

    /// `pik_elem_new`. Exactly one of (class id, string, sublist) is given;
    /// all-none yields a noop placeholder.
    pub fn elem_new(
        &mut self,
        id: Option<&Tok>,
        s: Option<&Tok>,
        sublist: Option<Vec<usize>>,
    ) -> Option<usize> {
        if self.has_err() {
            return None;
        }
        // Determine the class first.
        let class = if sublist.is_some() {
            Class::Sublist
        } else if s.is_some() {
            Class::Text
        } else if let Some(idt) = id {
            match Class::from_name(&idt.text) {
                Some(c) => c,
                None => {
                    self.error(Some((idt.start, idt.end)), "unknown object type");
                    return None;
                }
            }
        } else {
            Class::Noop
        };

        let mut o = PObj::blank(class);
        // Reference point / entry edge from prior object & direction.
        if self.list.is_empty() {
            o.pt_at = PPoint::new(0.0, 0.0);
            o.e_with = cp::C;
        } else {
            let prior = self.objects[*self.list.last().unwrap()].pt_exit;
            o.pt_at = prior;
            o.e_with = match self.e_dir {
                dir::LEFT => cp::E,
                dir::UP => cp::S,
                dir::DOWN => cp::N,
                _ => cp::W,
            };
        }
        o.with = o.pt_at;
        o.in_dir = self.e_dir;
        o.out_dir = self.e_dir;
        let (layer, miss) = self.value_miss("layer");
        o.i_layer = if miss { 1000 } else { value::pik_round(layer) };
        if o.i_layer < 0 {
            o.i_layer = 0;
        }
        if let Some(idt) = id {
            o.err_span = (idt.start, idt.end);
        } else if let Some(st) = s {
            o.err_span = (st.start, st.end);
        }

        // Reset path-building state for the new current object.
        self.n_tpath = 1;
        self.a_tpath[0] = o.pt_at;
        self.then_flag = false;
        self.same_path = false;

        let idx = self.objects.len();

        if class == Class::Sublist {
            o.sublist = sublist;
            self.objects.push(o);
            self.cur = Some(idx);
            self.sublist_init(idx);
            return Some(idx);
        }

        // Common style defaults for real classes (not noop).
        if class != Class::Noop {
            o.sw = self.value("thickness");
            o.fill = self.value("fill");
            o.color = self.value("color");
        }
        self.objects.push(o);
        self.cur = Some(idx);

        match class {
            Class::Noop => {
                let o = &mut self.objects[idx];
                o.pt_enter = o.pt_at;
                o.pt_exit = o.pt_at;
            }
            Class::Text => {
                self.class_init(idx);
                if let Some(st) = s {
                    self.add_txt(st, st.e_code);
                }
            }
            _ => self.class_init(idx),
        }
        Some(idx)
    }

    /// `pik_elem_setname`.
    pub fn elem_setname(&mut self, idx: Option<usize>, name: &Tok) {
        if let Some(i) = idx {
            self.objects[i].name = Some(name.text.clone());
        }
    }

    /// Append a finished statement to the current list (`pik_elist_append`;
    /// in this port `self.list` is the single accumulator, matching how
    /// upstream keeps `p->list` pointed at the growing list).
    pub fn append_stmt(&mut self, obj: Option<usize>) {
        if let Some(i) = obj {
            self.list.push(i);
        }
    }

    /// Enter a `[...]` sublist: save and clear the current list (`savelist`).
    pub fn begin_sublist(&mut self) {
        let saved = std::mem::take(&mut self.list);
        self.list_stack.push(saved);
    }

    /// Leave a `[...]` sublist: take the inner list and restore the outer one.
    pub fn end_sublist(&mut self) -> Vec<usize> {
        let sub = std::mem::take(&mut self.list);
        self.list = self.list_stack.pop().unwrap_or_default();
        sub
    }

    /// Render the top-level accumulated list.
    pub fn render_top(&mut self) {
        let l = std::mem::take(&mut self.list);
        self.render(l);
    }

    // ----- class initializers (xInit) -----------------------------------

    fn class_init(&mut self, idx: usize) {
        let class = self.objects[idx].class;
        match class {
            Class::Arrow => {
                let (w, h, rad) = (self.value("linewid"), self.value("lineht"), self.value("linerad"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = rad;
                o.rarrow = true;
            }
            Class::Line => {
                let (w, h, rad) = (self.value("linewid"), self.value("lineht"), self.value("linerad"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = rad;
            }
            Class::Spline => {
                let (w, h) = (self.value("linewid"), self.value("lineht"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = 1000.0; // splineInit
            }
            Class::Arc => {
                let w = self.value("arcrad");
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = w;
            }
            Class::Box => {
                let (w, h, rad) = (self.value("boxwid"), self.value("boxht"), self.value("boxrad"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = rad;
            }
            Class::Circle => {
                let r2 = self.value("circlerad") * 2.0;
                let o = &mut self.objects[idx];
                o.w = r2;
                o.h = r2;
                o.rad = 0.5 * r2;
            }
            Class::Cylinder => {
                let (w, h, rad) = (self.value("cylwid"), self.value("cylht"), self.value("cylrad"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = rad;
            }
            Class::Diamond => {
                let (w, h) = (self.value("diamondwid"), self.value("diamondht"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.b_alt_auto_fit = true;
            }
            Class::Dot => {
                let rad = self.value("dotrad");
                let o = &mut self.objects[idx];
                o.rad = rad;
                o.w = rad * 6.0;
                o.h = rad * 6.0;
                o.fill = o.color;
            }
            Class::Ellipse => {
                let (w, h) = (self.value("ellipsewid"), self.value("ellipseht"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
            }
            Class::File => {
                let (w, h, rad) = (self.value("filewid"), self.value("fileht"), self.value("filerad"));
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = h;
                o.rad = rad;
            }
            Class::Move => {
                let w = self.value("movewid");
                let o = &mut self.objects[idx];
                o.w = w;
                o.h = w;
                o.fill = -1.0;
                o.color = -1.0;
                o.sw = -1.0;
            }
            Class::Oval => {
                let (h, w) = (self.value("ovalht"), self.value("ovalwid"));
                let o = &mut self.objects[idx];
                o.h = h;
                o.w = w;
                o.rad = 0.5 * h.min(w);
            }
            Class::Text => {
                let o = &mut self.objects[idx];
                o.sw = 0.0;
            }
            Class::Sublist | Class::Noop => {}
        }
    }

    fn sublist_init(&mut self, idx: usize) {
        // Bounding box over the sublist; ptAt becomes the bbox center.
        let sub = self.objects[idx].sublist.clone().unwrap_or_default();
        let mut bbox = PBox::init();
        for &c in &sub {
            bbox.add_box(&self.objects[c].bbox);
        }
        let o = &mut self.objects[idx];
        o.bbox = bbox;
        if bbox.is_empty() {
            o.w = 0.0;
            o.h = 0.0;
            o.pt_at = PPoint::new(0.0, 0.0);
        } else {
            o.w = bbox.ne.x - bbox.sw.x;
            o.h = bbox.ne.y - bbox.sw.y;
            o.pt_at = PPoint::new((bbox.ne.x + bbox.sw.x) / 2.0, (bbox.ne.y + bbox.sw.y) / 2.0);
        }
    }
}

// ===== compass-point offsets (xOffset) ==================================

const RX_K: f64 = 0.29289321881345252392; // 1 - cos(45deg), for rounded corners
const COS45: f64 = 0.70710678118654747608;

fn box_offset(w: f64, h: f64, rad: f64, cp: u8) -> PPoint {
    let w2 = 0.5 * w;
    let h2 = 0.5 * h;
    let mut r = rad;
    let rx = if r <= 0.0 {
        0.0
    } else {
        if r > w2 {
            r = w2;
        }
        if r > h2 {
            r = h2;
        }
        RX_K * r
    };
    match cp {
        cp::C => PPoint::new(0.0, 0.0),
        cp::N => PPoint::new(0.0, h2),
        cp::NE => PPoint::new(w2 - rx, h2 - rx),
        cp::E => PPoint::new(w2, 0.0),
        cp::SE => PPoint::new(w2 - rx, rx - h2),
        cp::S => PPoint::new(0.0, -h2),
        cp::SW => PPoint::new(rx - w2, rx - h2),
        cp::W => PPoint::new(-w2, 0.0),
        cp::NW => PPoint::new(rx - w2, h2 - rx),
        _ => PPoint::new(0.0, 0.0),
    }
}

fn ellipse_offset(w: f64, h: f64, cp: u8) -> PPoint {
    let w1 = w * 0.5;
    let w2 = w1 * COS45;
    let h1 = h * 0.5;
    let h2 = h1 * COS45;
    match cp {
        cp::C => PPoint::new(0.0, 0.0),
        cp::N => PPoint::new(0.0, h1),
        cp::NE => PPoint::new(w2, h2),
        cp::E => PPoint::new(w1, 0.0),
        cp::SE => PPoint::new(w2, -h2),
        cp::S => PPoint::new(0.0, -h1),
        cp::SW => PPoint::new(-w2, -h2),
        cp::W => PPoint::new(-w1, 0.0),
        cp::NW => PPoint::new(-w2, h2),
        _ => PPoint::new(0.0, 0.0),
    }
}

fn cylinder_offset(w: f64, h: f64, rad: f64, cp: u8) -> PPoint {
    let w2 = w * 0.5;
    let h1 = h * 0.5;
    let h2 = h1 - rad;
    match cp {
        cp::C => PPoint::new(0.0, 0.0),
        cp::N => PPoint::new(0.0, h1),
        cp::NE => PPoint::new(w2, h2),
        cp::E => PPoint::new(w2, 0.0),
        cp::SE => PPoint::new(w2, -h2),
        cp::S => PPoint::new(0.0, -h1),
        cp::SW => PPoint::new(-w2, -h2),
        cp::W => PPoint::new(-w2, 0.0),
        cp::NW => PPoint::new(-w2, h2),
        _ => PPoint::new(0.0, 0.0),
    }
}

fn diamond_offset(w: f64, h: f64, cp: u8) -> PPoint {
    let w2 = 0.5 * w;
    let w4 = 0.25 * w;
    let h2 = 0.5 * h;
    let h4 = 0.25 * h;
    match cp {
        cp::C => PPoint::new(0.0, 0.0),
        cp::N => PPoint::new(0.0, h2),
        cp::NE => PPoint::new(w4, h4),
        cp::E => PPoint::new(w2, 0.0),
        cp::SE => PPoint::new(w4, -h4),
        cp::S => PPoint::new(0.0, -h2),
        cp::SW => PPoint::new(-w4, -h4),
        cp::W => PPoint::new(-w2, 0.0),
        cp::NW => PPoint::new(-w4, h4),
        _ => PPoint::new(0.0, 0.0),
    }
}

fn file_offset(w: f64, h: f64, rad: f64, cp: u8) -> PPoint {
    let w2 = 0.5 * w;
    let h2 = 0.5 * h;
    let mn = w2.min(h2);
    let mut rx = rad;
    if rx > mn {
        rx = mn;
    }
    if rx < mn * 0.25 {
        rx = mn * 0.25;
    }
    rx *= 0.5;
    match cp {
        cp::C => PPoint::new(0.0, 0.0),
        cp::N => PPoint::new(0.0, h2),
        cp::NE => PPoint::new(w2 - rx, h2 - rx),
        cp::E => PPoint::new(w2, 0.0),
        cp::SE => PPoint::new(w2, -h2),
        cp::S => PPoint::new(0.0, -h2),
        cp::SW => PPoint::new(-w2, -h2),
        cp::W => PPoint::new(-w2, 0.0),
        cp::NW => PPoint::new(-w2, h2),
        _ => PPoint::new(0.0, 0.0),
    }
}

/// `pik_elem_offset`: dispatch the compass-point offset by class.
fn elem_offset(o: &PObj, cp: u8) -> PPoint {
    match o.class {
        Class::Circle | Class::Ellipse => ellipse_offset(o.w, o.h, cp),
        Class::Cylinder => cylinder_offset(o.w, o.h, o.rad, cp),
        Class::Diamond => diamond_offset(o.w, o.h, cp),
        Class::File => file_offset(o.w, o.h, o.rad, cp),
        Class::Dot => PPoint::new(0.0, 0.0),
        _ => box_offset(o.w, o.h, o.rad, cp),
    }
}

// ===== chop (line trimming at object boundaries) ========================

/// True if a class has an `xChop` method (i.e. can trim a line at its edge).
fn is_chopper(class: Class) -> bool {
    matches!(
        class,
        Class::Box
            | Class::Circle
            | Class::Cylinder
            | Class::Diamond
            | Class::Dot
            | Class::Ellipse
            | Class::File
            | Class::Oval
            | Class::Text
    )
}

/// `boxChop`.
fn box_chop(o: &PObj, pt: PPoint) -> PPoint {
    if o.w <= 0.0 || o.h <= 0.0 {
        return o.pt_at;
    }
    let dx = (pt.x - o.pt_at.x) * o.h / o.w;
    let dy = pt.y - o.pt_at.y;
    let cp = if dx > 0.0 {
        if dy >= 2.414 * dx {
            cp::N
        } else if dy >= 0.414 * dx {
            cp::NE
        } else if dy >= -0.414 * dx {
            cp::E
        } else if dy > -2.414 * dx {
            cp::SE
        } else {
            cp::S
        }
    } else if dy >= -2.414 * dx {
        cp::N
    } else if dy >= -0.414 * dx {
        cp::NW
    } else if dy >= 0.414 * dx {
        cp::W
    } else if dy > 2.414 * dx {
        cp::SW
    } else {
        cp::S
    };
    let off = elem_offset(o, cp);
    PPoint::new(o.pt_at.x + off.x, o.pt_at.y + off.y)
}

/// `circleChop`.
fn circle_chop(o: &PObj, pt: PPoint) -> PPoint {
    let dx = pt.x - o.pt_at.x;
    let dy = pt.y - o.pt_at.y;
    let dist = dx.hypot(dy);
    if dist < o.rad || dist <= 0.0 {
        return o.pt_at;
    }
    PPoint::new(o.pt_at.x + dx * o.rad / dist, o.pt_at.y + dy * o.rad / dist)
}

/// `ellipseChop`.
fn ellipse_chop(o: &PObj, pt: PPoint) -> PPoint {
    if o.w <= 0.0 || o.h <= 0.0 {
        return o.pt_at;
    }
    let dx = pt.x - o.pt_at.x;
    let dy = pt.y - o.pt_at.y;
    let s = o.h / o.w;
    let dq = dx * s;
    let dist = dq.hypot(dy);
    if dist < o.h {
        return o.pt_at;
    }
    PPoint::new(
        o.pt_at.x + 0.5 * dq * o.h / (dist * s),
        o.pt_at.y + 0.5 * dy * o.h / dist,
    )
}

fn chop_of(o: &PObj, from: PPoint) -> PPoint {
    match o.class {
        Class::Circle | Class::Dot => circle_chop(o, from),
        Class::Ellipse => ellipse_chop(o, from),
        _ => box_chop(o, from),
    }
}

/// `arcControlPoint`: the Bézier control point for an arc from `f` to `t`.
fn arc_control_point(cw: bool, f: PPoint, t: PPoint) -> PPoint {
    let mut m = PPoint::new(0.5 * (f.x + t.x), 0.5 * (f.y + t.y));
    let dx = t.x - f.x;
    let dy = t.y - f.y;
    if cw {
        m.x -= 0.5 * dy;
        m.y += 0.5 * dx;
    } else {
        m.x += 0.5 * dy;
        m.y -= 0.5 * dx;
    }
    m
}

/// `radiusMidpoint`: point `r` back from `t` toward `f`; flags when `r` had to
/// be clamped to the midpoint.
fn radius_midpoint(f: PPoint, t: PPoint, r: f64) -> (PPoint, bool) {
    let dx = t.x - f.x;
    let dy = t.y - f.y;
    let dist = dx.hypot(dy);
    if dist <= 0.0 {
        return (t, false);
    }
    let dx = dx / dist;
    let dy = dy / dist;
    let (r, mid) = if r > 0.5 * dist {
        (0.5 * dist, true)
    } else {
        (r, false)
    };
    (PPoint::new(t.x - r * dx, t.y - r * dy), mid)
}

// ===== direction, exit points, attribute setters ========================

impl Pik {
    /// `pik_set_direction`.
    pub fn set_direction(&mut self, e_dir: i32) {
        self.e_dir = e_dir;
        if let Some(&last) = self.list.last() {
            self.elem_set_exit(last, e_dir);
        }
    }

    /// `pik_elem_set_exit`.
    fn elem_set_exit(&mut self, idx: usize, e_dir: i32) {
        let o = &mut self.objects[idx];
        o.out_dir = e_dir;
        if !o.class.is_line() || o.b_close {
            o.pt_exit = o.pt_at;
            match e_dir {
                dir::LEFT => o.pt_exit.x -= o.w * 0.5,
                dir::UP => o.pt_exit.y += o.h * 0.5,
                dir::DOWN => o.pt_exit.y -= o.h * 0.5,
                _ => o.pt_exit.x += o.w * 0.5,
            }
        }
    }

    /// The current object, but only if there is one and no error has occurred.
    /// Attribute setters use this to no-op after an error — upstream stops
    /// tokenizing on the first error, but our parser consumes the whole stream.
    fn guard_cur(&self) -> Option<usize> {
        if self.has_err() {
            None
        } else {
            self.cur
        }
    }

    /// `pik_param_ok`: verify a property may be set; record it.
    fn param_ok(&mut self, span: (usize, usize), m_this: u32) -> bool {
        let idx = match self.guard_cur() {
            Some(i) => i,
            None => return true,
        };
        let o = &self.objects[idx];
        if o.m_prop & m_this != 0 {
            self.error(Some(span), "value is already set");
            return true;
        }
        if o.m_calc & m_this != 0 {
            self.error(Some(span), "value already fixed by prior constraints");
            return true;
        }
        self.objects[idx].m_prop |= m_this;
        false
    }

    /// `pik_set_numprop` for HEIGHT/WIDTH/RADIUS/DIAMETER/THICKNESS.
    pub fn set_numprop(&mut self, kind: NumProp, span: (usize, usize), val: PRel) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        let mask = match kind {
            NumProp::Height => prop::HEIGHT,
            NumProp::Width => prop::WIDTH,
            NumProp::Radius | NumProp::Diameter => prop::RADIUS,
            NumProp::Thickness => prop::THICKNESS,
        };
        if self.param_ok(span, mask) {
            return;
        }
        {
            let o = &mut self.objects[idx];
            match kind {
                NumProp::Height => o.h = o.h * val.rel + val.abs,
                NumProp::Width => o.w = o.w * val.rel + val.abs,
                NumProp::Radius => o.rad = o.rad * val.rel + val.abs,
                NumProp::Diameter => o.rad = o.rad * val.rel + 0.5 * val.abs,
                NumProp::Thickness => o.sw = o.sw * val.rel + val.abs,
            }
        }
        // Class-specific xNumProp constraints.
        let class = self.objects[idx].class;
        match class {
            Class::Circle => {
                let o = &mut self.objects[idx];
                match kind {
                    NumProp::Diameter | NumProp::Radius => {
                        o.w = 2.0 * o.rad;
                        o.h = o.w;
                    }
                    NumProp::Width => {
                        o.h = o.w;
                        o.rad = 0.5 * o.w;
                    }
                    NumProp::Height => {
                        o.w = o.h;
                        o.rad = 0.5 * o.w;
                    }
                    _ => {}
                }
            }
            Class::Oval => {
                let o = &mut self.objects[idx];
                o.rad = 0.5 * o.h.min(o.w);
            }
            Class::Dot => {} // dotNumProp only reacts to color/fill
            _ => {}
        }
    }

    /// `pik_set_clrprop`.
    pub fn set_clrprop(&mut self, is_fill: bool, span: (usize, usize), clr: f64) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        let mask = if is_fill { prop::FILL } else { prop::COLOR };
        if self.param_ok(span, mask) {
            return;
        }
        let class = self.objects[idx].class;
        let o = &mut self.objects[idx];
        if is_fill {
            o.fill = clr;
        } else {
            o.color = clr;
        }
        // dotNumProp: keep fill==color synced for dots.
        if class == Class::Dot {
            if is_fill {
                o.color = o.fill;
            } else {
                o.fill = o.color;
            }
        }
    }

    /// `pik_set_dashed` (dotted/dashed; default value = dashwid).
    pub fn set_dashed(&mut self, dotted: bool, val: Option<f64>) {
        let v = val.unwrap_or_else(|| self.value("dashwid"));
        let idx = match self.guard_cur() { Some(i) => i, None => return }; let o = &mut self.objects[idx];
        if dotted {
            o.dotted = v;
            o.dashed = 0.0;
        } else {
            o.dashed = v;
            o.dotted = 0.0;
        }
    }

    /// Boolean style flags (cw/ccw/arrows/invis/thick/thin/solid).
    pub fn set_bool(&mut self, b: BoolProp) {
        let thickness = if matches!(b, BoolProp::Solid) {
            self.value("thickness")
        } else {
            0.0
        };
        let idx = match self.guard_cur() { Some(i) => i, None => return }; let o = &mut self.objects[idx];
        match b {
            BoolProp::Cw => o.cw = true,
            BoolProp::Ccw => o.cw = false,
            BoolProp::Larrow => {
                o.larrow = true;
                o.rarrow = false;
            }
            BoolProp::Rarrow => {
                o.larrow = false;
                o.rarrow = true;
            }
            BoolProp::Lrarrow => {
                o.larrow = true;
                o.rarrow = true;
            }
            BoolProp::Invis => o.sw = -0.00001,
            BoolProp::Thick => o.sw *= 1.5,
            BoolProp::Thin => o.sw *= 0.67,
            BoolProp::Solid => {
                o.sw = thickness;
                o.dotted = 0.0;
                o.dashed = 0.0;
            }
        }
    }

    pub fn close_path(&mut self, span: (usize, usize)) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if self.objects[idx].class.is_line() {
            self.objects[idx].b_close = true;
        } else {
            self.error(Some(span), "use with line-oriented objects only");
        }
    }

    // ----- path building for line objects -------------------------------

    fn reset_samepath(&mut self) {
        if self.same_path {
            self.same_path = false;
            self.n_tpath = 1;
        }
    }

    fn next_rpath(&mut self) -> usize {
        let n = self.n_tpath - 1;
        if n + 1 >= self.a_tpath.len() {
            return n;
        }
        let nn = n + 1;
        self.n_tpath += 1;
        self.a_tpath[nn] = self.a_tpath[nn - 1];
        self.m_tpath = 0;
        nn
    }

    /// `pik_then`.
    pub fn then(&mut self, span: (usize, usize)) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if !self.objects[idx].class.is_line() {
            self.error(Some(span), "use with line-oriented objects only");
            return;
        }
        let n = self.n_tpath as i32 - 1;
        if n < 1 && (self.objects[idx].m_prop & prop::FROM) == 0 {
            self.error(Some(span), "no prior path points");
            return;
        }
        self.then_flag = true;
    }

    /// `pik_add_direction`: "up 0.5", "left 3", "down", a bare distance, etc.
    pub fn add_direction(&mut self, pdir: Option<i32>, span: Option<(usize, usize)>, val: PRel) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if !self.objects[idx].class.is_line() {
            self.error(span, "use with line-oriented objects only");
            return;
        }
        self.reset_samepath();
        let mut n = self.n_tpath - 1;
        if self.then_flag || self.m_tpath == 3 || n == 0 {
            n = self.next_rpath();
            self.then_flag = false;
        }
        let d = pdir.unwrap_or(self.e_dir);
        let (w, h) = (self.objects[idx].w, self.objects[idx].h);
        match d {
            dir::UP => {
                if self.m_tpath & 2 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].y += val.abs + h * val.rel;
                self.m_tpath |= 2;
            }
            dir::DOWN => {
                if self.m_tpath & 2 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].y -= val.abs + h * val.rel;
                self.m_tpath |= 2;
            }
            dir::RIGHT => {
                if self.m_tpath & 1 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].x += val.abs + w * val.rel;
                self.m_tpath |= 1;
            }
            dir::LEFT => {
                if self.m_tpath & 1 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].x -= val.abs + w * val.rel;
                self.m_tpath |= 1;
            }
            _ => {}
        }
        self.objects[idx].out_dir = d;
    }

    // ----- text ----------------------------------------------------------

    /// `pik_add_txt`: attach a string literal as a text item.
    pub fn add_txt(&mut self, s: &Tok, e_code: i32) {
        let idx = match self.guard_cur() { Some(i) => i, None => return }; let o = &mut self.objects[idx];
        if o.txt.len() >= 5 {
            return;
        }
        o.txt.push(PText {
            text: s.text.clone(),
            e_code,
        });
    }

    /// `pik_size_to_fit`: size the object to enclose its text.
    pub fn size_to_fit(&mut self, span: (usize, usize), e_which: i32) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if self.objects[idx].txt.is_empty() {
            self.error(Some(span), "no text to fit to");
            return;
        }
        self.compute_layout_settings();
        self.txt_vertical_layout(idx);
        let mut bbox = PBox::init();
        self.append_txt_measure(idx, &mut bbox);
        let at = self.objects[idx].pt_at;
        let alt = self.objects[idx].b_alt_auto_fit;
        let w = if (e_which & 1) != 0 || alt {
            (bbox.ne.x - bbox.sw.x) + self.char_width
        } else {
            0.0
        };
        let h = if (e_which & 2) != 0 || alt {
            let h1 = bbox.ne.y - at.y;
            let h2 = at.y - bbox.sw.y;
            2.0 * h1.max(h2) + 0.5 * self.char_height
        } else {
            0.0
        };
        self.xfit(idx, w, h);
        self.objects[idx].m_prop |= prop::FIT;
    }

    /// `xFit` dispatch.
    fn xfit(&mut self, idx: usize, w: f64, h: f64) {
        let class = self.objects[idx].class;
        let o = &mut self.objects[idx];
        match class {
            Class::Box | Class::Ellipse | Class::Text => {
                if w > 0.0 {
                    o.w = w;
                }
                if h > 0.0 {
                    o.h = h;
                }
            }
            Class::Circle => {
                let mut mx = 0.0_f64;
                if w > 0.0 {
                    mx = w;
                }
                if h > mx {
                    mx = h;
                }
                if w * h > 0.0 && (w * w + h * h) > mx * mx {
                    mx = w.hypot(h);
                }
                if mx > 0.0 {
                    o.rad = 0.5 * mx;
                    o.w = mx;
                    o.h = mx;
                }
            }
            Class::Cylinder => {
                if w > 0.0 {
                    o.w = w;
                }
                if h > 0.0 {
                    o.h = h + 0.25 * o.rad + o.sw;
                }
            }
            Class::File => {
                if w > 0.0 {
                    o.w = w;
                }
                if h > 0.0 {
                    o.h = h + 2.0 * o.rad;
                }
            }
            Class::Diamond => {
                if o.w <= 0.0 {
                    o.w = w * 1.5;
                }
                if o.h <= 0.0 {
                    o.h = h * 1.5;
                }
                if o.w > 0.0 && o.h > 0.0 {
                    let x = o.w * h / o.h + w;
                    let y = o.h * x / o.w;
                    o.w = x;
                    o.h = y;
                }
            }
            Class::Oval => {
                if w > 0.0 {
                    o.w = w;
                }
                if h > 0.0 {
                    o.h = h;
                }
                if o.w < o.h {
                    o.w = o.h;
                }
                o.rad = 0.5 * o.h.min(o.w);
            }
            _ => {}
        }
    }
}

/// Which numeric property is being set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumProp {
    Height,
    Width,
    Radius,
    Diameter,
    Thickness,
}

/// Which boolean style flag is being set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolProp {
    Cw,
    Ccw,
    Larrow,
    Rarrow,
    Lrarrow,
    Invis,
    Thick,
    Thin,
    Solid,
}

/// Strip the surrounding quotes from a string literal token text.
pub fn strip_quotes(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}


/// Font scale from TP_* flags (`pik_font_scale`).
fn font_scale(e_code: i32) -> f64 {
    const TP_BIG: i32 = 0x0100;
    const TP_SMALL: i32 = 0x0200;
    const TP_XTRA: i32 = 0x0400;
    let mut scale = 1.0;
    if e_code & TP_BIG != 0 {
        scale *= 1.25;
    }
    if e_code & TP_SMALL != 0 {
        scale *= 0.8;
    }
    if e_code & TP_XTRA != 0 {
        scale *= scale;
    }
    scale
}

/// Row heights and justification margin for an object's text block.
#[derive(Default)]
struct TxtLayout {
    hc: f64,
    ha1: f64,
    ha2: f64,
    hb1: f64,
    hb2: f64,
    jw: f64,
    ybase: f64,
}

/// Per-character width estimates (`awChar`), 100 = average. ASCII 0x20..=0x7e.
const AWCHAR: [u8; 95] = [
    45, 55, 62, 115, 90, 132, 125, 40, 55, 55, 71, 115, 45, 48, 45, 50, 91, 91, 91, 91, 91, 91, 91,
    91, 91, 91, 50, 50, 120, 120, 120, 78, 142, 102, 105, 110, 115, 105, 98, 105, 125, 58, 58, 107,
    95, 145, 125, 115, 95, 115, 107, 95, 97, 118, 102, 150, 100, 93, 100, 58, 50, 58, 119, 72, 72,
    86, 92, 80, 92, 85, 52, 92, 92, 47, 47, 88, 48, 135, 92, 86, 92, 92, 69, 75, 58, 92, 80, 121,
    81, 80, 76, 91, 49, 91, 118,
];

/// `pik_text_length`: 100 * estimated average character width of a string
/// literal (text includes its surrounding quotes, as in `PToken.z/n`).
fn text_length(text: &str, mono: bool) -> i32 {
    let b = text.as_bytes();
    let n = b.len();
    let std_avg = 100i32;
    let mono_avg = 82i32;
    let mut cnt = 0i32;
    let mut j = 1usize;
    while j + 1 < n {
        let mut c = b[j];
        if c == b'\\' && b[j + 1] != b'&' {
            j += 1;
            c = b[j];
        } else if c == b'&' {
            let mut k = j + 1;
            while k < j + 7 && k < n && b[k] != 0 && b[k] != b';' {
                k += 1;
            }
            if k < n && b[k] == b';' {
                j = k;
            }
            cnt += (if mono { mono_avg } else { std_avg }) * 3 / 2;
            j += 1;
            continue;
        }
        if c & 0xc0 == 0xc0 {
            while j + 1 < n - 1 && (b[j + 1] & 0xc0) == 0x80 {
                j += 1;
            }
            cnt += if mono { mono_avg } else { std_avg };
            j += 1;
            continue;
        }
        if mono {
            cnt += mono_avg;
        } else if (0x20..=0x7e).contains(&c) {
            cnt += AWCHAR[(c - 0x20) as usize] as i32;
        } else {
            cnt += std_avg;
        }
        j += 1;
    }
    cnt
}

// ===== layout finalizer + render pipeline ===============================

impl Pik {
    /// `pik_after_adding_attributes`: finalize geometry once a statement's
    /// attributes have all been parsed.
    pub fn after_adding_attributes(&mut self, idx: usize) -> usize {
        if self.has_err() {
            return idx;
        }
        let class = self.objects[idx].class;
        let is_line = class.is_line();

        if !is_line {
            // Auto-fit block objects with non-positive dimensions.
            let (w, h, ntxt) = {
                let o = &self.objects[idx];
                (o.w, o.h, o.txt.len())
            };
            if h <= 0.0 {
                if ntxt == 0 {
                    self.objects[idx].h = 0.0;
                } else if w <= 0.0 {
                    self.size_to_fit(self.objects[idx].err_span, 3);
                } else {
                    self.size_to_fit(self.objects[idx].err_span, 2);
                }
            }
            if self.objects[idx].w <= 0.0 {
                if ntxt == 0 {
                    self.objects[idx].w = 0.0;
                } else {
                    self.size_to_fit(self.objects[idx].err_span, 1);
                }
            }
            // Move so that the WITH edge lands on the WITH point.
            let e_with = self.objects[idx].e_with;
            let ofst = elem_offset(&self.objects[idx], e_with);
            let o = &self.objects[idx];
            let dx = (o.with.x - ofst.x) - o.pt_at.x;
            let dy = (o.with.y - ofst.y) - o.pt_at.y;
            if dx != 0.0 || dy != 0.0 {
                self.elem_move(idx, dx, dy);
            }
        }

        // A line with no explicit movement gets one default-length step.
        if is_line && self.n_tpath < 2 {
            self.next_rpath();
            let (w, h, in_dir) = {
                let o = &self.objects[idx];
                (o.w, o.h, o.in_dir)
            };
            match in_dir {
                dir::DOWN => self.a_tpath[1].y -= h,
                dir::LEFT => self.a_tpath[1].x -= w,
                dir::UP => self.a_tpath[1].y += h,
                _ => self.a_tpath[1].x += w,
            }
            if class == Class::Arc {
                let cw = self.objects[idx].cw;
                let out_dir = (in_dir + if cw { 1 } else { 3 }) % 4;
                self.objects[idx].out_dir = out_dir;
                self.e_dir = out_dir;
                match out_dir {
                    dir::DOWN => self.a_tpath[1].y -= h,
                    dir::LEFT => self.a_tpath[1].x -= w,
                    dir::UP => self.a_tpath[1].y += h,
                    _ => self.a_tpath[1].x += w,
                }
            }
        }

        self.objects[idx].bbox = PBox::init();

        // xCheck: "dot" and "arc" have one.
        if class == Class::Dot {
            let (at, rad) = {
                let o = &self.objects[idx];
                (o.pt_at, o.rad)
            };
            let o = &mut self.objects[idx];
            o.w = 0.0;
            o.h = 0.0;
            o.bbox.add_ellipse(at.x, at.y, rad, rad);
        } else if class == Class::Arc {
            self.arc_check(idx);
            if self.has_err() {
                return idx;
            }
        }

        if is_line {
            let n = self.n_tpath;
            let mut path: Vec<PPoint> = self.a_tpath[..n].to_vec();
            // "chop": trim the line where it meets a choppable target object.
            let (b_chop, p_to, p_from) = {
                let o = &self.objects[idx];
                (o.b_chop, o.p_to, o.p_from)
            };
            if b_chop && n >= 2 {
                let nt = self.autochop(path[n - 2], path[n - 1], p_to);
                path[n - 1] = nt;
                let nf = self.autochop(path[1], path[0], p_from);
                path[0] = nf;
            }
            {
                let o = &mut self.objects[idx];
                o.path = path;
                o.pt_enter = o.path[0];
                o.pt_exit = o.path[n - 1];
                // Accumulate vertices into the existing bbox (arcCheck may have
                // already added the curve's extent).
                let mut bbox = o.bbox;
                for pt in &o.path {
                    bbox.add_xy(pt.x, pt.y);
                }
                o.bbox = bbox;
                o.pt_at.x = (bbox.ne.x + bbox.sw.x) / 2.0;
                o.pt_at.y = (bbox.ne.y + bbox.sw.y) / 2.0;
                o.w = bbox.ne.x - bbox.sw.x;
                o.h = bbox.ne.y - bbox.sw.y;
            }
            if self.objects[idx].b_close {
                let in_dir = self.objects[idx].in_dir;
                self.elem_set_exit(idx, in_dir);
            }
        } else {
            let (at, w, h, in_dir, out_dir) = {
                let o = &self.objects[idx];
                (o.pt_at, o.w, o.h, o.in_dir, o.out_dir)
            };
            let w2 = w / 2.0;
            let h2 = h / 2.0;
            let mut enter = at;
            let mut exit = at;
            match in_dir {
                dir::LEFT => enter.x += w2,
                dir::UP => enter.y -= h2,
                dir::DOWN => enter.y += h2,
                _ => enter.x -= w2,
            }
            match out_dir {
                dir::LEFT => exit.x -= w2,
                dir::UP => exit.y += h2,
                dir::DOWN => exit.y -= h2,
                _ => exit.x += w2,
            }
            let o = &mut self.objects[idx];
            o.pt_enter = enter;
            o.pt_exit = exit;
            o.bbox.add_xy(at.x - w2, at.y - h2);
            o.bbox.add_xy(at.x + w2, at.y + h2);
        }
        self.e_dir = self.objects[idx].out_dir;
        idx
    }

    /// `pik_elem_move`.
    fn elem_move(&mut self, idx: usize, dx: f64, dy: f64) {
        {
            let o = &mut self.objects[idx];
            o.pt_at.x += dx;
            o.pt_at.y += dy;
            o.pt_enter.x += dx;
            o.pt_enter.y += dy;
            o.pt_exit.x += dx;
            o.pt_exit.y += dy;
            o.bbox.ne.x += dx;
            o.bbox.ne.y += dy;
            o.bbox.sw.x += dx;
            o.bbox.sw.y += dy;
            for p in &mut o.path {
                p.x += dx;
                p.y += dy;
            }
        }
        if let Some(sub) = self.objects[idx].sublist.clone() {
            for c in sub {
                self.elem_move(c, dx, dy);
            }
        }
    }

    /// `pik_autochop`: trim the segment `from -> to` at the boundary of the
    /// target object (or whichever choppable object is centered at `to`).
    fn autochop(&self, from: PPoint, to: PPoint, target: Option<usize>) -> PPoint {
        let chopper = match target {
            Some(i) if is_chopper(self.objects[i].class) => Some(i),
            _ => self.find_chopper(&self.list, to, from),
        };
        match chopper {
            Some(i) => chop_of(&self.objects[i], from),
            None => to,
        }
    }

    /// `pik_find_chopper`: a choppable object centered at `center` whose bbox
    /// does not contain `other`, searched newest-first incl. sublists.
    fn find_chopper(&self, list: &[usize], center: PPoint, other: PPoint) -> Option<usize> {
        for &i in list.iter().rev() {
            let o = &self.objects[i];
            if is_chopper(o.class)
                && o.pt_at.x == center.x
                && o.pt_at.y == center.y
                && !o.bbox.contains_point(&other)
            {
                return Some(i);
            }
            if let Some(sub) = &o.sublist {
                if let Some(found) = self.find_chopper(sub, center, other) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// `pik_compute_layout_settings`.
    fn compute_layout_settings(&mut self) {
        if self.layout_vars_done {
            return;
        }
        let mut thickness = self.value("thickness");
        if thickness <= 0.01 {
            thickness = 0.01;
        }
        let w_arrow = 0.5 * self.value("arrowwid");
        self.w_arrow = w_arrow / thickness;
        self.h_arrow = self.value("arrowht") / thickness;
        let mut fs = self.value("fontscale");
        if fs <= 0.0 {
            fs = 1.0;
        }
        self.font_scale = fs;
        self.r_scale = 144.0;
        self.char_width = self.value("charwid") * fs;
        self.char_height = self.value("charht") * fs;
        self.layout_vars_done = true;
    }

    /// `pik_render`: produce the final SVG into the output buffer.
    pub fn render(&mut self, list: Vec<usize>) {
        if self.has_err() {
            return;
        }
        // An empty top-level list emits nothing; pikchr() then returns the
        // "empty diagram" comment (matching upstream's zOut==0 check).
        if list.is_empty() {
            return;
        }
        self.compute_layout_settings();
        let mut thickness = self.value("thickness");
        if thickness <= 0.01 {
            thickness = 0.01;
        }
        let margin = self.value("margin") + thickness;
        let w_arrow = self.w_arrow * thickness;

        let (fg, fgmiss) = self.value_miss("fgcolor");
        self.fgcolor = if fgmiss {
            value::pik_round(value::lookup_color("fgcolor").max(0.0))
        } else {
            value::pik_round(fg)
        };
        let (bg, bgmiss) = self.value_miss("bgcolor");
        self.bgcolor = if bgmiss {
            value::pik_round(value::lookup_color("bgcolor").max(0.0))
        } else {
            value::pik_round(bg)
        };

        // Bounding box over everything.
        self.bbox = PBox::init();
        self.bbox_add_elist(&list, w_arrow);
        self.bbox.ne.x += margin + self.value("rightmargin");
        self.bbox.ne.y += margin + self.value("topmargin");
        self.bbox.sw.x -= margin + self.value("leftmargin");
        self.bbox.sw.y -= margin + self.value("bottommargin");

        let w = self.bbox.ne.x - self.bbox.sw.x;
        let h = self.bbox.ne.y - self.bbox.sw.y;
        let mut wsvg = value::pik_round(self.r_scale * w);
        let mut hsvg = value::pik_round(self.r_scale * h);

        self.out.push_str(
            "<svg xmlns='http://www.w3.org/2000/svg' style='font-size:initial;'",
        );
        let pik_scale = self.value("scale");
        if pik_scale >= 0.001 && pik_scale <= 1000.0 && (pik_scale < 0.99 || pik_scale > 1.01) {
            wsvg = value::pik_round(wsvg as f64 * pik_scale);
            hsvg = value::pik_round(hsvg as f64 * pik_scale);
            self.out.push_str(&format!(" width=\"{wsvg}\" height=\"{hsvg}\""));
        }
        self.out.push_str(&format!(
            " viewBox=\"0 0 {} {}\">\n",
            fmt_g(self.r_scale * w),
            fmt_g(self.r_scale * h)
        ));
        self.elist_render(&list);
        self.out.push_str("</svg>\n");
    }

    /// `pik_bbox_add_elist`.
    fn bbox_add_elist(&mut self, list: &[usize], w_arrow: f64) {
        for &idx in list {
            if self.objects[idx].sw >= 0.0 {
                let b = self.objects[idx].bbox;
                self.bbox.add_box(&b);
            }
            self.measure_txt(idx);
            if let Some(sub) = self.objects[idx].sublist.clone() {
                self.bbox_add_elist(&sub, w_arrow);
            }
            let o = &self.objects[idx];
            if o.class.is_line() && !o.path.is_empty() {
                if o.larrow {
                    let p = o.path[0];
                    self.bbox.add_ellipse(p.x, p.y, w_arrow, w_arrow);
                }
                if o.rarrow {
                    let p = *o.path.last().unwrap();
                    self.bbox.add_ellipse(p.x, p.y, w_arrow, w_arrow);
                }
            }
        }
    }

    /// `pik_elem_render`: a `<!-- ... -->` diagnostic comment per object,
    /// emitted when the (undocumented) `debug = 1` variable is set.
    fn elem_render(&mut self, idx: usize) {
        let o = &self.objects[idx];
        let (name, class, w, h, at, enter, exit, out_dir, ntxt, txt0) = (
            o.name.clone(),
            o.class.name(),
            o.w,
            o.h,
            o.pt_at,
            o.pt_enter,
            o.pt_exit,
            o.out_dir,
            o.txt.len(),
            o.txt.first().map(|t| strip_quotes(&t.text)),
        );
        self.put("<!-- ");
        if let Some(n) = name {
            self.put(&escape_text(n.as_bytes(), false, false));
            self.put(": ");
        }
        self.put(&escape_text(class.as_bytes(), false, false));
        if ntxt > 0 {
            self.put(" \"");
            if let Some(t) = txt0 {
                self.put(&escape_text(t.as_bytes(), true, false));
            }
            self.put("\"");
        }
        self.put(&format!(" w={}", fmt_num(w)));
        self.put(&format!(" h={}", fmt_num(h)));
        self.put(&format!(" center={},{}", fmt_num(at.x), fmt_num(at.y)));
        self.put(&format!(" enter={},{}", fmt_num(enter.x), fmt_num(enter.y)));
        let zdir = match out_dir {
            dir::LEFT => " left",
            dir::UP => " up",
            dir::DOWN => " down",
            _ => " right",
        };
        self.put(&format!(" exit={},{}", fmt_num(exit.x), fmt_num(exit.y)));
        self.put(zdir);
        self.put(" -->\n");
    }

    /// `pik_elist_render`: render objects, honoring layers.
    fn elist_render(&mut self, list: &[usize]) {
        let m_debug = value::pik_round(self.value("debug"));
        let mut next_layer = 0i32;
        loop {
            let mut more = false;
            let this_layer = next_layer;
            next_layer = i32::MAX;
            for &idx in list {
                let layer = self.objects[idx].i_layer;
                if layer > this_layer {
                    if layer < next_layer {
                        next_layer = layer;
                    }
                    more = true;
                    continue;
                } else if layer < this_layer {
                    continue;
                }
                if m_debug & 1 != 0 {
                    self.elem_render(idx);
                }
                self.render_obj(idx);
                if let Some(sub) = self.objects[idx].sublist.clone() {
                    self.elist_render(&sub);
                }
            }
            if !more {
                break;
            }
        }
        // Optional debug labels: a colored dot + name at each named object.
        let (cl, miss) = self.value_miss("debug_label_color");
        if !miss && cl >= 0.0 {
            for &idx in list {
                if let Some(name) = self.objects[idx].name.clone() {
                    let at = self.objects[idx].pt_at;
                    self.render_label_dot(at, &name, cl);
                }
            }
        }
    }

    /// Render one debug label: a small filled dot with the object's name above
    /// it (mirrors the `debug_label_color` path of `pik_elist_render`).
    fn render_label_dot(&mut self, at: PPoint, name: &str, cl: f64) {
        let mut dot = PObj::blank(Class::Dot);
        dot.rad = 0.015;
        dot.sw = 0.015;
        dot.fill = cl;
        dot.color = cl;
        dot.pt_at = at;
        dot.txt.push(PText {
            // Upstream uses the bare name (no quotes) as the text token.
            text: name.to_string(),
            e_code: tp::ABOVE,
        });
        let idx = self.objects.len();
        self.objects.push(dot);
        self.render_dot(idx);
        self.objects.pop();
    }

    fn render_obj(&mut self, idx: usize) {
        match self.objects[idx].class {
            Class::Box | Class::Oval => self.render_box(idx),
            Class::Circle => self.render_circle(idx),
            Class::Ellipse => self.render_ellipse(idx),
            Class::Cylinder => self.render_cylinder(idx),
            Class::Diamond => self.render_diamond(idx),
            Class::File => self.render_file(idx),
            Class::Dot => self.render_dot(idx),
            Class::Arc => self.arc_render(idx),
            Class::Line | Class::Arrow | Class::Spline => self.spline_render(idx),
            Class::Text => self.emit_txt(idx),
            Class::Move | Class::Sublist | Class::Noop => {}
        }
    }
}

// ===== SVG emitter helpers ==============================================

impl Pik {
    fn put(&mut self, s: &str) {
        self.out.push_str(s);
    }
    fn append_x(&mut self, z1: &str, v: f64, z2: &str) {
        let v = v - self.bbox.sw.x;
        self.out
            .push_str(&format!("{z1}{}{z2}", fmt_g(self.r_scale * v)));
    }
    fn append_y(&mut self, z1: &str, v: f64, z2: &str) {
        let v = self.bbox.ne.y - v;
        self.out
            .push_str(&format!("{z1}{}{z2}", fmt_g(self.r_scale * v)));
    }
    fn append_xy(&mut self, z1: &str, x: f64, y: f64) {
        let x = x - self.bbox.sw.x;
        let y = self.bbox.ne.y - y;
        self.out.push_str(&format!(
            "{z1}{},{}",
            fmt_g(self.r_scale * x),
            fmt_g(self.r_scale * y)
        ));
    }
    fn append_dis(&mut self, z1: &str, v: f64, z2: &str) {
        self.out
            .push_str(&format!("{z1}{}{z2}", fmt_g(self.r_scale * v)));
    }
    fn append_arc(&mut self, r1: f64, r2: f64, x: f64, y: f64) {
        let x = x - self.bbox.sw.x;
        let y = self.bbox.ne.y - y;
        self.out.push_str(&format!(
            "A{} {} 0 0 0 {} {}",
            fmt_g(self.r_scale * r1),
            fmt_g(self.r_scale * r2),
            fmt_g(self.r_scale * x),
            fmt_g(self.r_scale * y)
        ));
    }
    fn append_clr(&mut self, z1: &str, v: f64, z2: &str, bg: bool) {
        let mut x = value::pik_round(v);
        if x == 0 && self.fgcolor > 0 && !bg {
            x = self.fgcolor;
        } else if bg && x >= 0xffffff && self.bgcolor > 0 {
            x = self.bgcolor;
        } else if self.flags_dark {
            x = color_to_dark_mode(x, bg);
        }
        let r = (x >> 16) & 0xff;
        let g = (x >> 8) & 0xff;
        let b = x & 0xff;
        self.out
            .push_str(&format!("{z1}rgb({r},{g},{b}){z2}"));
    }

    /// `pik_append_style`.
    fn append_style(&mut self, idx: usize, e_fill: i32) {
        let (fill, color, sw, dotted, dashed, npath, rad) = {
            let o = &self.objects[idx];
            (o.fill, o.color, o.sw, o.dotted, o.dashed, o.path.len(), o.rad)
        };
        let mut clr_is_bg = false;
        self.put(" style=\"");
        if fill >= 0.0 && e_fill != 0 {
            let mut fill_is_bg = true;
            if fill == color {
                if e_fill == 2 {
                    fill_is_bg = false;
                }
                if e_fill == 3 {
                    clr_is_bg = true;
                }
            }
            self.append_clr("fill:", fill, ";", fill_is_bg);
        } else {
            self.put("fill:none;");
        }
        if sw >= 0.0 && color >= 0.0 {
            let mut sw = sw;
            self.append_dis("stroke-width:", sw, ";");
            if npath > 2 && rad <= sw {
                self.put("stroke-linejoin:round;");
            }
            self.append_clr("stroke:", color, ";", clr_is_bg);
            if dotted > 0.0 {
                let v = dotted;
                if sw < 2.1 / self.r_scale {
                    sw = 2.1 / self.r_scale;
                }
                self.append_dis("stroke-dasharray:", sw, "");
                self.append_dis(",", v, ";");
            } else if dashed > 0.0 {
                let v = dashed;
                self.append_dis("stroke-dasharray:", v, "");
                self.append_dis(",", v, ";");
            }
        }
    }

    // ----- per-class renderers ------------------------------------------

    fn render_box(&mut self, idx: usize) {
        let (w, h, rad, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            let w2 = 0.5 * w;
            let h2 = 0.5 * h;
            if rad <= 0.0 {
                self.append_xy("<path d=\"M", pt.x - w2, pt.y - h2);
                self.append_xy("L", pt.x + w2, pt.y - h2);
                self.append_xy("L", pt.x + w2, pt.y + h2);
                self.append_xy("L", pt.x - w2, pt.y + h2);
                self.put("Z\" ");
            } else {
                let mut rad = rad;
                if rad > w2 {
                    rad = w2;
                }
                if rad > h2 {
                    rad = h2;
                }
                let x0 = pt.x - w2;
                let x1 = x0 + rad;
                let x3 = pt.x + w2;
                let x2 = x3 - rad;
                let y0 = pt.y - h2;
                let y1 = y0 + rad;
                let y3 = pt.y + h2;
                let y2 = y3 - rad;
                self.append_xy("<path d=\"M", x1, y0);
                if x2 > x1 {
                    self.append_xy("L", x2, y0);
                }
                self.append_arc(rad, rad, x3, y1);
                if y2 > y1 {
                    self.append_xy("L", x3, y2);
                }
                self.append_arc(rad, rad, x2, y3);
                if x2 > x1 {
                    self.append_xy("L", x1, y3);
                }
                self.append_arc(rad, rad, x0, y2);
                if y2 > y1 {
                    self.append_xy("L", x0, y1);
                }
                self.append_arc(rad, rad, x1, y0);
                self.put("Z\" ");
            }
            self.append_style(idx, 3);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_circle(&mut self, idx: usize) {
        let (_, _, rad, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            self.append_x("<circle cx=\"", pt.x, "\"");
            self.append_y(" cy=\"", pt.y, "\"");
            self.append_dis(" r=\"", rad, "\" ");
            self.append_style(idx, 3);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_ellipse(&mut self, idx: usize) {
        let (w, h, _, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            self.append_x("<ellipse cx=\"", pt.x, "\"");
            self.append_y(" cy=\"", pt.y, "\"");
            self.append_dis(" rx=\"", w / 2.0, "\"");
            self.append_dis(" ry=\"", h / 2.0, "\" ");
            self.append_style(idx, 3);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_dot(&mut self, idx: usize) {
        let (_, _, rad, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            self.append_x("<circle cx=\"", pt.x, "\"");
            self.append_y(" cy=\"", pt.y, "\"");
            self.append_dis(" r=\"", rad, "\"");
            self.append_style(idx, 2);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_diamond(&mut self, idx: usize) {
        let (w, h, _, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            let w2 = 0.5 * w;
            let h2 = 0.5 * h;
            self.append_xy("<path d=\"M", pt.x - w2, pt.y);
            self.append_xy("L", pt.x, pt.y - h2);
            self.append_xy("L", pt.x + w2, pt.y);
            self.append_xy("L", pt.x, pt.y + h2);
            self.put("Z\" ");
            self.append_style(idx, 3);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_cylinder(&mut self, idx: usize) {
        let (w, h, rad, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            let w2 = 0.5 * w;
            let h2 = 0.5 * h;
            let mut rad = rad;
            if rad > h2 {
                rad = h2;
            } else if rad < 0.0 {
                rad = 0.0;
            }
            self.append_xy("<path d=\"M", pt.x - w2, pt.y + h2 - rad);
            self.append_xy("L", pt.x - w2, pt.y - h2 + rad);
            self.append_arc(w2, rad, pt.x + w2, pt.y - h2 + rad);
            self.append_xy("L", pt.x + w2, pt.y + h2 - rad);
            self.append_arc(w2, rad, pt.x - w2, pt.y + h2 - rad);
            self.append_arc(w2, rad, pt.x + w2, pt.y + h2 - rad);
            self.put("\" ");
            self.append_style(idx, 3);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    fn render_file(&mut self, idx: usize) {
        let (w, h, rad, pt, sw) = self.geom(idx);
        if sw >= 0.0 {
            let w2 = 0.5 * w;
            let h2 = 0.5 * h;
            let mn = w2.min(h2);
            let mut rad = rad;
            if rad > mn {
                rad = mn;
            }
            if rad < mn * 0.25 {
                rad = mn * 0.25;
            }
            self.append_xy("<path d=\"M", pt.x - w2, pt.y - h2);
            self.append_xy("L", pt.x + w2, pt.y - h2);
            self.append_xy("L", pt.x + w2, pt.y + (h2 - rad));
            self.append_xy("L", pt.x + (w2 - rad), pt.y + h2);
            self.append_xy("L", pt.x - w2, pt.y + h2);
            self.put("Z\" ");
            self.append_style(idx, 1);
            self.put("\" />\n");
            self.append_xy("<path d=\"M", pt.x + (w2 - rad), pt.y + h2);
            self.append_xy("L", pt.x + (w2 - rad), pt.y + (h2 - rad));
            self.append_xy("L", pt.x + w2, pt.y + (h2 - rad));
            self.put("\" ");
            self.append_style(idx, 0);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    /// `lineRender`: straight polyline through the path points. Arrowheads chop
    /// the stored path endpoints in place (so aligned text sees the trimmed
    /// path, matching upstream).
    fn line_render(&mut self, idx: usize) {
        let (sw, larrow, rarrow, b_close) = {
            let o = &self.objects[idx];
            (o.sw, o.larrow, o.rarrow, o.b_close)
        };
        let n = self.objects[idx].path.len();
        if sw > 0.0 && n >= 2 {
            if larrow {
                let (a, b) = (self.objects[idx].path[1], self.objects[idx].path[0]);
                let nb = self.draw_arrowhead(idx, a, b);
                self.objects[idx].path[0] = nb;
            }
            if rarrow {
                let (a, b) = (self.objects[idx].path[n - 2], self.objects[idx].path[n - 1]);
                let nb = self.draw_arrowhead(idx, a, b);
                self.objects[idx].path[n - 1] = nb;
            }
            let path = self.objects[idx].path.clone();
            let mut z = "<path d=\"M";
            for p in &path {
                self.append_xy(z, p.x, p.y);
                z = "L";
            }
            if b_close {
                self.put("Z");
            } else {
                self.objects[idx].fill = -1.0;
            }
            self.put("\" ");
            let efill = if b_close { 3 } else { 0 };
            self.append_style(idx, efill);
            self.put("\" />\n");
        }
        self.emit_txt(idx);
    }

    /// `splineRender`: rounded-corner spline; falls back to a straight line
    /// when there are fewer than 3 points or no radius (covers line/arrow).
    fn spline_render(&mut self, idx: usize) {
        let (sw, rad, larrow, rarrow) = {
            let o = &self.objects[idx];
            (o.sw, o.rad, o.larrow, o.rarrow)
        };
        if sw > 0.0 {
            let n = self.objects[idx].path.len();
            if n < 3 || rad <= 0.0 {
                self.line_render(idx);
                return;
            }
            if larrow {
                let (a, b) = (self.objects[idx].path[1], self.objects[idx].path[0]);
                let nb = self.draw_arrowhead(idx, a, b);
                self.objects[idx].path[0] = nb;
            }
            if rarrow {
                let (a, b) = (self.objects[idx].path[n - 2], self.objects[idx].path[n - 1]);
                let nb = self.draw_arrowhead(idx, a, b);
                self.objects[idx].path[n - 1] = nb;
            }
            let path = self.objects[idx].path.clone();
            self.radius_path(idx, &path, rad);
        }
        self.emit_txt(idx);
    }

    /// `radiusPath`: emit a path that rounds each interior vertex by radius `r`.
    fn radius_path(&mut self, idx: usize, a: &[PPoint], r: f64) {
        let n = a.len();
        let b_close = self.objects[idx].b_close;
        let i_last = if b_close { n } else { n - 1 };
        self.append_xy("<path d=\"M", a[0].x, a[0].y);
        let (m, _) = radius_midpoint(a[0], a[1], r);
        self.append_xy(" L ", m.x, m.y);
        let mut an = a[n - 1];
        for i in 1..i_last {
            an = if i < n - 1 { a[i + 1] } else { a[0] };
            let (m, is_mid) = radius_midpoint(an, a[i], r);
            self.append_xy(" Q ", a[i].x, a[i].y);
            self.append_xy(" ", m.x, m.y);
            if !is_mid {
                let (m2, _) = radius_midpoint(a[i], an, r);
                self.append_xy(" L ", m2.x, m2.y);
            }
        }
        self.append_xy(" L ", an.x, an.y);
        if b_close {
            self.put("Z");
        } else {
            self.objects[idx].fill = -1.0;
        }
        self.put("\" ");
        let efill = if b_close { 3 } else { 0 };
        self.append_style(idx, efill);
        self.put("\" />\n");
    }

    /// `arcRender`: a quadratic Bézier from start to end.
    fn arc_render(&mut self, idx: usize) {
        let (sw, cw, larrow, rarrow, path) = {
            let o = &self.objects[idx];
            (o.sw, o.cw, o.larrow, o.rarrow, o.path.clone())
        };
        if path.len() < 2 || sw < 0.0 {
            self.emit_txt(idx);
            return;
        }
        let mut f = path[0];
        let mut t = path[1];
        let m = arc_control_point(cw, f, t);
        if larrow {
            f = self.draw_arrowhead(idx, m, f);
            self.objects[idx].path[0] = f;
        }
        if rarrow {
            t = self.draw_arrowhead(idx, m, t);
            self.objects[idx].path[1] = t;
        }
        self.append_xy("<path d=\"M", f.x, f.y);
        self.append_xy("Q", m.x, m.y);
        self.append_xy(" ", t.x, t.y);
        self.put("\" ");
        self.append_style(idx, 0);
        self.put("\" />\n");
        self.emit_txt(idx);
    }

    /// `arcCheck`: extend the bbox along the sampled quadratic curve.
    fn arc_check(&mut self, idx: usize) {
        if self.n_tpath > 2 {
            let span = self.objects[idx].err_span;
            self.error(Some(span), "arc geometry error");
            return;
        }
        let f = self.a_tpath[0];
        let t = self.a_tpath[1];
        let cw = self.objects[idx].cw;
        let sw = self.objects[idx].sw;
        let m = arc_control_point(cw, f, t);
        for i in 1..16 {
            let t1 = 0.0625 * i as f64;
            let t2 = 1.0 - t1;
            let a = t2 * t2;
            let b = 2.0 * t1 * t2;
            let c = t1 * t1;
            let x = a * f.x + b * m.x + c * t.x;
            let y = a * f.y + b * m.y + c * t.y;
            self.objects[idx].bbox.add_ellipse(x, y, sw, sw);
        }
    }

    /// `pik_draw_arrowhead`: emit the arrowhead polygon and return the new
    /// (shortened) endpoint `t`.
    fn draw_arrowhead(&mut self, idx: usize, f: PPoint, t: PPoint) -> PPoint {
        let (color, sw) = {
            let o = &self.objects[idx];
            (o.color, o.sw)
        };
        let mut dx = t.x - f.x;
        let mut dy = t.y - f.y;
        let dist = dx.hypot(dy);
        let mut h = self.h_arrow * sw;
        let w = self.w_arrow * sw;
        if color < 0.0 || sw <= 0.0 || dist <= 0.0 {
            return t;
        }
        dx /= dist;
        dy /= dist;
        let mut e1 = dist - h;
        if e1 < 0.0 {
            e1 = 0.0;
            h = dist;
        }
        let ddx = -w * dy;
        let ddy = w * dx;
        let bx = f.x + e1 * dx;
        let by = f.y + e1 * dy;
        self.append_xy("<polygon points=\"", t.x, t.y);
        self.append_xy(" ", bx - ddx, by - ddy);
        self.append_xy(" ", bx + ddx, by + ddy);
        self.append_clr("\" style=\"fill:", color, "\"/>\n", false);
        // pik_chop(f, t, h/2): shorten t toward f by h/2.
        let amt = h / 2.0;
        let mut nt = t;
        let d2 = (t.x - f.x).hypot(t.y - f.y);
        if d2 <= amt {
            nt = f;
        } else {
            let r = 1.0 - amt / d2;
            nt.x = f.x + r * (t.x - f.x);
            nt.y = f.y + r * (t.y - f.y);
        }
        nt
    }

    fn geom(&self, idx: usize) -> (f64, f64, f64, PPoint, f64) {
        let o = &self.objects[idx];
        (o.w, o.h, o.rad, o.pt_at, o.sw)
    }

    // ----- text rendering / measuring (approximate; refined in P6) ------

    /// Measure text into the global bbox (`pik_append_txt` with pBox).
    fn measure_txt(&mut self, idx: usize) {
        if self.objects[idx].txt.is_empty() {
            return;
        }
        self.txt_vertical_layout(idx);
        let mut tb = PBox::init();
        self.append_txt_measure(idx, &mut tb);
        self.bbox.add_box(&tb);
    }

    /// Compute the per-row heights / justification margin (`pik_append_txt`
    /// preamble). Assumes `txt_vertical_layout` has already run.
    fn txt_layout(&self, idx: usize) -> TxtLayout {
        let o = &self.objects[idx];
        let sw = if o.sw >= 0.0 { o.sw } else { 0.0 };
        let mut l = TxtLayout::default();
        let mut all_mask = 0i32;
        for t in &o.txt {
            all_mask |= t.e_code;
        }
        if o.class.is_line() {
            l.hc = sw * 1.5;
        } else if o.rad > 0.0 && o.class == Class::Cylinder {
            l.ybase = -0.75 * o.rad;
        }
        let cap = |sel: i32| -> f64 {
            let mut h = 0.0_f64;
            for t in &o.txt {
                if t.e_code & sel != 0 {
                    h = h.max(font_scale(t.e_code) * self.char_height);
                }
            }
            h
        };
        if all_mask & tp::CENTER != 0 {
            for t in &o.txt {
                if t.e_code & tp::CENTER != 0 {
                    let s = font_scale(t.e_code) * self.char_height;
                    if l.hc < s {
                        l.hc = s;
                    }
                }
            }
        }
        if all_mask & tp::ABOVE != 0 {
            l.ha1 = cap(tp::ABOVE);
            if all_mask & tp::ABOVE2 != 0 {
                l.ha2 = cap(tp::ABOVE2);
            }
        }
        if all_mask & tp::BELOW != 0 {
            l.hb1 = cap(tp::BELOW);
            if all_mask & tp::BELOW2 != 0 {
                l.hb2 = cap(tp::BELOW2);
            }
        }
        l.jw = if o.class.e_just() {
            0.5 * (o.w - 0.5 * (self.char_width + sw))
        } else {
            0.0
        };
        l
    }

    /// Per-item base offset (nx, y) relative to the object center, shared by
    /// the measure and render paths.
    fn txt_item_offset(&self, l: &TxtLayout, e_code: i32) -> (f64, f64) {
        let mut nx = 0.0;
        let mut y = l.ybase;
        if e_code & tp::ABOVE2 != 0 {
            y += 0.5 * l.hc + l.ha1 + 0.5 * l.ha2;
        }
        if e_code & tp::ABOVE != 0 {
            y += 0.5 * l.hc + 0.5 * l.ha1;
        }
        if e_code & tp::BELOW != 0 {
            y -= 0.5 * l.hc + 0.5 * l.hb1;
        }
        if e_code & tp::BELOW2 != 0 {
            y -= 0.5 * l.hc + l.hb1 + 0.5 * l.hb2;
        }
        if e_code & tp::LJUST != 0 {
            nx -= l.jw;
        }
        if e_code & tp::RJUST != 0 {
            nx += l.jw;
        }
        (nx, y)
    }

    /// Expand `bbox` to enclose all text of object `idx` (`pik_append_txt`
    /// measuring branch). This drives auto-fit and the overall canvas size.
    fn append_txt_measure(&self, idx: usize, bbox: &mut PBox) {
        let o = &self.objects[idx];
        if o.txt.is_empty() {
            return;
        }
        let l = self.txt_layout(idx);
        let x = o.pt_at.x;
        let orig_y = o.pt_at.y;
        for t in &o.txt {
            let xtra = font_scale(t.e_code);
            let (nx, y) = self.txt_item_offset(&l, t.e_code);
            let mut cw =
                text_length(&t.text, t.e_code & tp::MONO != 0) as f64 * self.char_width * xtra * 0.01;
            let ch = self.char_height * 0.5 * xtra;
            if (t.e_code & (tp::BOLD | tp::MONO)) == tp::BOLD {
                cw *= 1.1;
            }
            let (mut x0, mut y0, mut x1, mut y1);
            if t.e_code & tp::RJUST != 0 {
                x0 = nx;
                y0 = y - ch;
                x1 = nx - cw;
                y1 = y + ch;
            } else if t.e_code & tp::LJUST != 0 {
                x0 = nx;
                y0 = y - ch;
                x1 = nx + cw;
                y1 = y + ch;
            } else {
                x0 = nx + cw / 2.0;
                y0 = y + ch;
                x1 = nx - cw / 2.0;
                y1 = y - ch;
            }
            if t.e_code & tp::ALIGN != 0 && o.path.len() >= 2 {
                let nn = o.path.len();
                let dx = o.path[nn - 1].x - o.path[0].x;
                let dy = o.path[nn - 1].y - o.path[0].y;
                if dx != 0.0 || dy != 0.0 {
                    let dist = dx.hypot(dy);
                    let (dx, dy) = (dx / dist, dy / dist);
                    let tt = dx * x0 - dy * y0;
                    y0 = dy * x0 - dx * y0;
                    x0 = tt;
                    let tt = dx * x1 - dy * y1;
                    y1 = dy * x1 - dx * y1;
                    x1 = tt;
                }
            }
            bbox.add_xy(x + x0, orig_y + y0);
            bbox.add_xy(x + x1, orig_y + y1);
        }
    }

    /// Emit the `<text>` elements for object `idx` (`pik_append_txt` rendering
    /// branch). Text content/attributes are cosmetic; geometry comes from the
    /// measuring branch above.
    fn emit_txt(&mut self, idx: usize) {
        if self.objects[idx].txt.is_empty() {
            return;
        }
        self.txt_vertical_layout(idx);
        let l = self.txt_layout(idx);
        let items = self.objects[idx].txt.clone();
        let x = self.objects[idx].pt_at.x;
        let orig_y = self.objects[idx].pt_at.y;
        let color = self.objects[idx].color;
        let font_scale_global = self.font_scale;
        let path = self.objects[idx].path.clone();
        for t in &items {
            let (mut nx, y) = self.txt_item_offset(&l, t.e_code);
            nx += x;
            let y = y + orig_y;
            self.append_x("<text x=\"", nx, "\"");
            self.append_y(" y=\"", y, "\"");
            if t.e_code & tp::RJUST != 0 {
                self.put(" text-anchor=\"end\"");
            } else if t.e_code & tp::LJUST != 0 {
                self.put(" text-anchor=\"start\"");
            } else {
                self.put(" text-anchor=\"middle\"");
            }
            if t.e_code & tp::ITALIC != 0 {
                self.put(" font-style=\"italic\"");
            }
            if t.e_code & tp::BOLD != 0 {
                self.put(" font-weight=\"bold\"");
            }
            if t.e_code & tp::MONO != 0 {
                self.put(" font-family=\"monospace\"");
            }
            if color >= 0.0 {
                self.append_clr(" fill=\"", color, "\"", false);
            }
            let xtra = font_scale(t.e_code) * font_scale_global;
            if !(0.99..1.01).contains(&xtra) {
                self.put(&format!(" font-size=\"{}%\"", fmt_num(xtra * 100.0)));
            }
            if t.e_code & tp::ALIGN != 0 && path.len() >= 2 {
                let nn = path.len();
                let dx = path[nn - 1].x - path[0].x;
                let dy = path[nn - 1].y - path[0].y;
                if dx != 0.0 || dy != 0.0 {
                    let ang = dy.atan2(dx) * -180.0 / std::f64::consts::PI;
                    self.put(&format!(" transform=\"rotate({}", fmt_num(ang)));
                    self.append_xy(" ", x, orig_y);
                    self.put(")\"");
                }
            }
            self.put(" dominant-baseline=\"central\">");
            let inner = strip_quotes(&t.text);
            let mut content = String::new();
            render_text_content(&mut content, inner.as_bytes());
            self.put(&content);
            self.put("</text>\n");
        }
    }

    /// `pik_txt_vertical_layout`: assign each text item exactly one vertical
    /// slot (ABOVE2/ABOVE/CENTER/BELOW/BELOW2).
    fn txt_vertical_layout(&mut self, idx: usize) {
        let txt = &mut self.objects[idx].txt;
        let n = txt.len();
        if n == 0 {
            return;
        }
        if n == 1 {
            if txt[0].e_code & tp::VMASK == 0 {
                txt[0].e_code |= tp::CENTER;
            }
            return;
        }
        // Demote an extra ABOVE to ABOVE2.
        let mut j = 0;
        let mut m_just = 0;
        for i in (0..n).rev() {
            if txt[i].e_code & tp::ABOVE != 0 {
                if j == 0 {
                    j = 1;
                    m_just = txt[i].e_code & tp::JMASK;
                } else if j == 1 && m_just != 0 && (txt[i].e_code & m_just) == 0 {
                    j = 2;
                } else {
                    txt[i].e_code = (txt[i].e_code & !tp::VMASK) | tp::ABOVE2;
                    break;
                }
            }
        }
        // Demote an extra BELOW to BELOW2.
        j = 0;
        m_just = 0;
        for i in 0..n {
            if txt[i].e_code & tp::BELOW != 0 {
                if j == 0 {
                    j = 1;
                    m_just = txt[i].e_code & tp::JMASK;
                } else if j == 1 && m_just != 0 && (txt[i].e_code & m_just) == 0 {
                    j = 2;
                } else {
                    txt[i].e_code = (txt[i].e_code & !tp::VMASK) | tp::BELOW2;
                    break;
                }
            }
        }
        let mut all_slots = 0;
        for t in txt.iter() {
            all_slots |= t.e_code & tp::VMASK;
        }
        let mut free = [0i32; 5];
        let mut islot = 0;
        if n == 2 && ((txt[0].e_code | txt[1].e_code) & tp::JMASK) == (tp::LJUST | tp::RJUST) {
            free[0] = tp::CENTER;
            free[1] = tp::CENTER;
            islot = 2;
        } else {
            if n >= 4 && all_slots & tp::ABOVE2 == 0 {
                free[islot] = tp::ABOVE2;
                islot += 1;
            }
            if all_slots & tp::ABOVE == 0 {
                free[islot] = tp::ABOVE;
                islot += 1;
            }
            if n & 1 != 0 {
                free[islot] = tp::CENTER;
                islot += 1;
            }
            if all_slots & tp::BELOW == 0 {
                free[islot] = tp::BELOW;
                islot += 1;
            }
            if n >= 4 && all_slots & tp::BELOW2 == 0 {
                free[islot] = tp::BELOW2;
                islot += 1;
            }
        }
        let _ = islot;
        let mut k = 0;
        for t in txt.iter_mut() {
            if t.e_code & tp::VMASK == 0 {
                t.e_code |= free[k];
                k += 1;
            }
        }
    }

    // ----- references / positioning -------------------------------------

    /// `pik_place_of_elem`.
    pub fn place_of_elem(&mut self, obj: Option<usize>, edge: Option<&Tok>) -> PPoint {
        let i = match obj {
            Some(i) => i,
            None => return PPoint::default(),
        };
        let e = match edge {
            None => return self.objects[i].pt_at,
            Some(e) => e,
        };
        let cp = e.e_edge;
        if cp >= 1 && cp < crate::token::cp::END {
            let off = elem_offset(&self.objects[i], cp);
            let at = self.objects[i].pt_at;
            PPoint::new(at.x + off.x, at.y + off.y)
        } else if cp == crate::token::cp::START {
            self.objects[i].pt_enter
        } else {
            self.objects[i].pt_exit
        }
    }

    fn search_list(&self, basis: Option<usize>) -> Option<Vec<usize>> {
        match basis {
            None => Some(self.list.clone()),
            Some(b) => self.objects[b].sublist.clone(),
        }
    }

    /// `pik_find_byname`.
    pub fn find_byname(&mut self, basis: Option<usize>, name: &Tok) -> Option<usize> {
        let list = match self.search_list(basis) {
            Some(l) => l,
            None => {
                self.error(Some((name.start, name.end)), "no such object");
                return None;
            }
        };
        // Explicitly tagged objects first.
        for &i in list.iter().rev() {
            if self.objects[i].name.as_deref() == Some(name.text.as_str()) {
                self.last_ref = Some(i);
                return Some(i);
            }
        }
        // Then any object whose text exactly matches the name.
        for &i in list.iter().rev() {
            for t in &self.objects[i].txt {
                if strip_quotes(&t.text) == name.text {
                    self.last_ref = Some(i);
                    return Some(i);
                }
            }
        }
        self.error(Some((name.start, name.end)), "no such object");
        None
    }

    /// `pik_find_nth`. The `nth` token's text identifies the class ("last"/
    /// "previous" = any, "[" = sublist), and `e_code` is the (signed) ordinal.
    pub fn find_nth(&mut self, basis: Option<usize>, nth: &Tok) -> Option<usize> {
        let list = match self.search_list(basis) {
            Some(l) => l,
            None => {
                self.error(Some((nth.start, nth.end)), "no such object");
                return None;
            }
        };
        let class: Option<Class> = if nth.text == "last" || nth.text == "previous" {
            None
        } else if nth.text == "[" {
            Some(Class::Sublist)
        } else {
            match Class::from_name(&nth.text) {
                Some(c) => Some(c),
                None => {
                    self.error(Some((nth.start, nth.end)), "no such object type");
                    return None;
                }
            }
        };
        let mut n = nth.e_code;
        if n < 0 {
            for &i in list.iter().rev() {
                if let Some(c) = class {
                    if self.objects[i].class != c {
                        continue;
                    }
                }
                n += 1;
                if n == 0 {
                    self.last_ref = Some(i);
                    return Some(i);
                }
            }
        } else {
            for &i in &list {
                if let Some(c) = class {
                    if self.objects[i].class != c {
                        continue;
                    }
                }
                n -= 1;
                if n == 0 {
                    self.last_ref = Some(i);
                    return Some(i);
                }
            }
        }
        self.error(Some((nth.start, nth.end)), "no such object");
        None
    }

    /// `pik_nth_value`: convert "2nd"/"first" to an ordinal.
    pub fn nth_value(&mut self, nth: &Tok) -> i32 {
        let digits: String = nth.text.chars().take_while(|c| c.is_ascii_digit()).collect();
        let mut i: i32 = digits.parse().unwrap_or(0);
        if i > 1000 {
            self.error(Some((nth.start, nth.end)), "value too big - max '1000th'");
            i = 1;
        }
        if i == 0 && nth.text == "first" {
            i = 1;
        }
        i
    }

    pub fn this_obj(&self) -> Option<usize> {
        self.cur
    }

    /// `chop` attribute.
    pub fn set_chop(&mut self) {
        if let Some(idx) = self.guard_cur() {
            self.objects[idx].b_chop = true;
        }
    }

    /// `pik_last_ref_object`: the last referenced object iff centered at `pt`.
    fn last_ref_object(&mut self, pt: PPoint) -> Option<usize> {
        let res = self.last_ref.filter(|&i| {
            let at = self.objects[i].pt_at;
            at.x == pt.x && at.y == pt.y
        });
        self.last_ref = None;
        res
    }

    /// `pik_set_at`.
    pub fn set_at(&mut self, edge: Option<&Tok>, at_pt: PPoint, err: &Tok) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if self.objects[idx].class.is_line() {
            self.error(
                Some((err.start, err.end)),
                "use \"from\" and \"to\" to position this object",
            );
            return;
        }
        if self.objects[idx].m_prop & prop::AT != 0 {
            self.error(Some((err.start, err.end)), "location fixed by prior \"at\"");
            return;
        }
        self.objects[idx].m_prop |= prop::AT;
        let mut e_with = edge.map(|e| e.e_edge).unwrap_or(cp::C);
        if e_with >= cp::END {
            const E_DIR_TO_CP: [u8; 4] = [cp::E, cp::S, cp::W, cp::N];
            let o = &self.objects[idx];
            let d = if e_with == cp::END {
                o.out_dir
            } else {
                (o.in_dir + 2) % 4
            };
            e_with = E_DIR_TO_CP[d as usize];
        }
        let o = &mut self.objects[idx];
        o.e_with = e_with;
        o.with = at_pt;
    }

    /// `pik_set_from`.
    pub fn set_from(&mut self, span: (usize, usize), pt: PPoint) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if !self.objects[idx].class.is_line() {
            self.error(Some(span), "use \"at\" to position this object");
            return;
        }
        if self.objects[idx].m_prop & prop::FROM != 0 {
            self.error(Some(span), "line start location already fixed");
            return;
        }
        if self.objects[idx].b_close {
            self.error(Some(span), "polygon is closed");
            return;
        }
        if self.n_tpath > 1 {
            let dx = pt.x - self.a_tpath[0].x;
            let dy = pt.y - self.a_tpath[0].y;
            for i in 1..self.n_tpath {
                self.a_tpath[i].x += dx;
                self.a_tpath[i].y += dy;
            }
        }
        self.a_tpath[0] = pt;
        self.m_tpath = 3;
        self.objects[idx].m_prop |= prop::FROM;
        let from = self.last_ref_object(pt);
        self.objects[idx].p_from = from;
    }

    /// `pik_add_to`.
    pub fn add_to(&mut self, span: (usize, usize), pt: PPoint) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if !self.objects[idx].class.is_line() {
            self.error(Some(span), "use \"at\" to position this object");
            return;
        }
        if self.objects[idx].b_close {
            self.error(Some(span), "polygon is closed");
            return;
        }
        self.reset_samepath();
        let mut n = self.n_tpath - 1;
        if n == 0 || self.m_tpath == 3 || self.then_flag {
            n = self.next_rpath();
        }
        self.a_tpath[n] = pt;
        self.m_tpath = 3;
        let to = self.last_ref_object(pt);
        self.objects[idx].p_to = to;
    }

    /// `pik_move_hdg`: "then [dist] heading ANGLE" or "then [dist] EDGEPT".
    pub fn move_hdg(
        &mut self,
        dist: PRel,
        heading: Option<f64>,
        edge: Option<u8>,
        span: (usize, usize),
    ) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        let r_dist = dist.abs + self.value("linewid") * dist.rel;
        if !self.objects[idx].class.is_line() {
            self.error(Some(span), "use with line-oriented objects only");
            return;
        }
        self.reset_samepath();
        let mut n = self.next_rpath();
        while n < 1 {
            n = self.next_rpath();
        }
        let mut r_hdg = if let Some(a) = heading {
            a.rem_euclid(360.0)
        } else {
            let e = edge.unwrap_or(0);
            if e == cp::C {
                self.error(Some(span), "syntax error");
                return;
            }
            hdg_angle(e)
        };
        self.objects[idx].out_dir = if r_hdg <= 45.0 {
            dir::UP
        } else if r_hdg <= 135.0 {
            dir::RIGHT
        } else if r_hdg <= 225.0 {
            dir::DOWN
        } else if r_hdg <= 315.0 {
            dir::LEFT
        } else {
            dir::UP
        };
        r_hdg *= DEG2RAD;
        self.a_tpath[n].x += r_dist * r_hdg.sin();
        self.a_tpath[n].y += r_dist * r_hdg.cos();
        self.m_tpath = 2;
    }

    /// `pik_evenwith`: "DIR until even with POSITION".
    pub fn evenwith(&mut self, d: i32, span: (usize, usize), place: PPoint) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        if !self.objects[idx].class.is_line() {
            self.error(Some(span), "use with line-oriented objects only");
            return;
        }
        self.reset_samepath();
        let mut n = self.n_tpath - 1;
        if self.then_flag || self.m_tpath == 3 || n == 0 {
            n = self.next_rpath();
            self.then_flag = false;
        }
        match d {
            dir::DOWN | dir::UP => {
                if self.m_tpath & 2 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].y = place.y;
                self.m_tpath |= 2;
            }
            dir::RIGHT | dir::LEFT => {
                if self.m_tpath & 1 != 0 {
                    n = self.next_rpath();
                }
                self.a_tpath[n].x = place.x;
                self.m_tpath |= 1;
            }
            _ => {}
        }
        self.objects[idx].out_dir = d;
    }

    /// `pik_same` / `same as`.
    pub fn same(&mut self, span: (usize, usize), other: Option<usize>) {
        let idx = match self.guard_cur() { Some(i) => i, None => return };
        let class = self.objects[idx].class;
        let other = match other {
            Some(o) => o,
            None => {
                let found = self
                    .list
                    .iter()
                    .rev()
                    .copied()
                    .find(|&i| self.objects[i].class == class);
                match found {
                    Some(o) => o,
                    None => {
                        self.error(Some(span), "no prior objects of the same type");
                        return;
                    }
                }
            }
        };
        let src = self.objects[other].clone();
        if !src.path.is_empty() && class.is_line() {
            let dx = self.a_tpath[0].x - src.path[0].x;
            let dy = self.a_tpath[0].y - src.path[0].y;
            for i in 1..src.path.len() {
                self.a_tpath[i] = PPoint::new(src.path[i].x + dx, src.path[i].y + dy);
            }
            self.n_tpath = src.path.len();
            self.m_tpath = 3;
            self.same_path = true;
        }
        let o = &mut self.objects[idx];
        if !class.is_line() {
            o.w = src.w;
            o.h = src.h;
        }
        o.rad = src.rad;
        o.sw = src.sw;
        o.dashed = src.dashed;
        o.dotted = src.dotted;
        o.fill = src.fill;
        o.color = src.color;
        o.cw = src.cw;
        o.larrow = src.larrow;
        o.rarrow = src.rarrow;
        o.b_close = src.b_close;
        o.b_chop = src.b_chop;
        o.i_layer = src.i_layer;
    }

    /// `pik_behind`.
    pub fn behind(&mut self, other: Option<usize>) {
        if let Some(o) = other {
            let idx = match self.guard_cur() { Some(i) => i, None => return };
            let ol = self.objects[o].i_layer;
            if self.objects[idx].i_layer >= ol {
                self.objects[idx].i_layer = ol - 1;
            }
        }
    }

    /// `pik_nth_vertex`.
    pub fn nth_vertex(&mut self, nth: &Tok, err: &Tok, obj: Option<usize>) -> PPoint {
        let i = match obj {
            Some(i) => i,
            None => return self.a_tpath[0],
        };
        if !self.objects[i].class.is_line() {
            self.error(Some((err.start, err.end)), "object is not a line");
            return PPoint::default();
        }
        let digits: String = nth.text.chars().take_while(|c| c.is_ascii_digit()).collect();
        let n: usize = digits.parse().unwrap_or(0);
        let np = self.objects[i].path.len();
        if n < 1 || n > np {
            self.error(Some((nth.start, nth.end)), "no such vertex");
            return PPoint::default();
        }
        self.objects[i].path[n - 1]
    }
    pub fn property_of(&mut self, obj: Option<usize>, prop_tok: &Tok) -> f64 {
        if let Some(i) = obj {
            let o = &self.objects[i];
            match prop_tok.text.as_str() {
                "width" | "wid" => o.w,
                "height" | "ht" => o.h,
                "radius" | "rad" => o.rad,
                "diameter" => o.rad * 2.0,
                "thickness" => o.sw,
                "fill" => o.fill,
                "color" => o.color,
                "x" => o.pt_at.x,
                "y" => o.pt_at.y,
                "top" => o.bbox.ne.y,
                "bottom" => o.bbox.sw.y,
                "left" => o.bbox.sw.x,
                "right" => o.bbox.ne.x,
                _ => 0.0,
            }
        } else {
            0.0
        }
    }
    pub fn position_between(frac: f64, p1: PPoint, p2: PPoint) -> PPoint {
        PPoint::new(p1.x + frac * (p2.x - p1.x), p1.y + frac * (p2.y - p1.y))
    }
    pub fn position_at_angle(dist: f64, hdg: f64, pt: PPoint) -> PPoint {
        let r = hdg * DEG2RAD;
        PPoint::new(pt.x + dist * r.sin(), pt.y + dist * r.cos())
    }
    pub fn position_at_hdg(&mut self, dist: f64, edge: &Tok, pt: PPoint) -> PPoint {
        let hdg = hdg_angle(edge.e_edge);
        Pik::position_at_angle(dist, hdg, pt)
    }

    pub fn assert_eq(&mut self, x: f64, op: &Tok, y: f64) -> Option<usize> {
        if x != y {
            self.error(
                Some((op.start, op.end)),
                &format!("assertion failed: {x} != {y}"),
            );
        }
        None
    }
    pub fn position_assert(&mut self, x: PPoint, op: &Tok, y: PPoint) -> Option<usize> {
        if x != y {
            self.error(Some((op.start, op.end)), "position assertion failed");
        }
        None
    }
    pub fn add_macro(&mut self, _id: &Tok, _code: &Tok) {
        // Macro expansion happens at tokenize time (the `define` statement is
        // otherwise a no-op here).
    }

    // ----- print (diagnostic output prepended to the result) ------------

    /// `pritem ::= rvalue` etc: append a number (`pik_append_num`, %.10g).
    pub fn print_num(&mut self, v: f64) {
        self.out.push_str(&fmt_num(v));
    }
    /// `pritem ::= STRING`: append string contents, escaping only `<`/`>`
    /// (`pik_append_text` with flags 0).
    pub fn print_str(&mut self, s: &Tok) {
        let inner = strip_quotes(&s.text);
        for c in inner.chars() {
            match c {
                '<' => self.out.push_str("&lt;"),
                '>' => self.out.push_str("&gt;"),
                other => self.out.push(other),
            }
        }
    }
    /// `prsep ::= COMMA`: a single separating space.
    pub fn print_sep(&mut self) {
        self.out.push(' ');
    }
    /// End of a `print` statement.
    pub fn print_br(&mut self) {
        self.out.push_str("<br>\n");
    }
    /// Value of a builtin name (for `print fill` / `color` / `thickness`).
    pub fn value_of(&self, name: &str) -> f64 {
        self.value(name)
    }

    pub fn finish(self) -> String {
        self.out
    }
    pub fn output(&self) -> &str {
        &self.out
    }
    pub fn set_direction_dir(&mut self, d: i32) {
        self.set_direction(d);
    }
    pub fn cur_dir(&self) -> i32 {
        self.e_dir
    }
}

/// `pik_color_to_dark_mode`.
fn color_to_dark_mode(x: i32, is_bg: bool) -> i32 {
    let x = 0xffffff - x;
    let mut r = (x >> 16) & 0xff;
    let mut g = (x >> 8) & 0xff;
    let mut b = x & 0xff;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    r = mn + (mx - r);
    g = mn + (mx - g);
    b = mn + (mx - b);
    if is_bg {
        if mx > 127 {
            r = (127 * r) / mx;
            g = (127 * g) / mx;
            b = (127 * b) / mx;
        }
    } else if mn < 128 && mx > mn {
        r = 127 + ((r - mn) * 128) / (mx - mn);
        g = 127 + ((g - mn) * 128) / (mx - mn);
        b = 127 + ((b - mn) * 128) / (mx - mn);
    }
    r * 0x10000 + g * 0x100 + b
}

/// Compass-point heading angle in degrees (`pik_hdg_angle`).
fn hdg_angle(edge: u8) -> f64 {
    match edge {
        cp::N => 0.0,
        cp::NE => 45.0,
        cp::E => 90.0,
        cp::SE => 135.0,
        cp::S => 180.0,
        cp::SW => 225.0,
        cp::W => 270.0,
        cp::NW => 315.0,
        _ => 0.0,
    }
}

/// `pik_isentity`: true if `z` (starting with '&') is a valid HTML entity.
fn is_entity(z: &[u8]) -> bool {
    let n = z.len();
    if n < 4 || z[0] != b'&' {
        return false;
    }
    let body = &z[1..];
    if body[0] == b'#' {
        let d = &body[1..];
        for i in 0..d.len() {
            if i > 1 && d[i] == b';' {
                return true;
            } else if !d[i].is_ascii_digit() {
                return false;
            }
        }
    } else {
        for i in 0..body.len() {
            let c = body[i];
            if i > 1 && c == b';' {
                return true;
            } else if i > 0 && c.is_ascii_digit() {
                continue;
            } else if !(c.is_ascii_uppercase() || c.is_ascii_lowercase()) {
                return false;
            }
        }
    }
    false
}

/// `pik_append_text`: always escape `<`/`>`; with `b_space` turn spaces into
/// U+00A0; with `b_amp` turn `&` into `&amp;` unless it begins a valid entity.
fn append_text_escaped(out: &mut String, z: &[u8], b_space: bool, b_amp: bool) {
    let n = z.len();
    let mut start = 0;
    let mut i = 0;
    let flush = |out: &mut String, seg: &[u8]| {
        if !seg.is_empty() {
            out.push_str(std::str::from_utf8(seg).unwrap_or(""));
        }
    };
    while i < n {
        let c = z[i];
        let is_break = c == b'<' || c == b'>' || (c == b' ' && b_space) || (c == b'&' && b_amp);
        if is_break {
            flush(out, &z[start..i]);
            match c {
                b'<' => out.push_str("&lt;"),
                b'>' => out.push_str("&gt;"),
                b' ' => out.push('\u{00a0}'),
                b'&' => {
                    if is_entity(&z[i..]) {
                        out.push('&');
                    } else {
                        out.push_str("&amp;");
                    }
                }
                _ => {}
            }
            start = i + 1;
        }
        i += 1;
    }
    flush(out, &z[start..n]);
}

/// `pik_append_text` as a standalone string, for the given flags.
fn escape_text(z: &[u8], b_space: bool, b_amp: bool) -> String {
    let mut s = String::new();
    append_text_escaped(&mut s, z, b_space, b_amp);
    s
}

/// Render the inner text of a string token (quotes already stripped),
/// mirroring the backslash handling of `pik_append_txt`'s render loop.
fn render_text_content(out: &mut String, z: &[u8]) {
    let total = z.len() as isize;
    let mut off = 0usize;
    let mut nz = total;
    while nz > 0 {
        let seg = &z[off..off + nz as usize];
        let mut j = 0usize;
        while j < seg.len() && seg[j] != b'\\' {
            j += 1;
        }
        if j > 0 {
            append_text_escaped(out, &seg[..j], true, true);
        }
        if j < seg.len() && (j + 1 == seg.len() || seg[j + 1] == b'\\') {
            out.push_str("&#92;");
            j += 1;
        }
        let consumed = j + 1;
        off += consumed;
        nz -= consumed as isize;
    }
}
