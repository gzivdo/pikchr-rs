//! The diagram object model: ports of `PPoint`, `PBox`, `PRel`, `PObj`,
//! `PList`, and the object class table (`aClass`) from pikchr.y.

/// Property bitmask values (`A_*`).
pub mod prop {
    pub const WIDTH: u32 = 0x0001;
    pub const HEIGHT: u32 = 0x0002;
    pub const RADIUS: u32 = 0x0004;
    pub const THICKNESS: u32 = 0x0008;
    pub const DASHED: u32 = 0x0010;
    pub const FILL: u32 = 0x0020;
    pub const COLOR: u32 = 0x0040;
    pub const ARROW: u32 = 0x0080;
    pub const FROM: u32 = 0x0100;
    pub const CW: u32 = 0x0200;
    pub const AT: u32 = 0x0400;
    pub const TO: u32 = 0x0800;
    pub const FIT: u32 = 0x1000;
}

/// A point in 2-D space (`PPoint`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PPoint {
    pub x: f64,
    pub y: f64,
}

impl PPoint {
    pub fn new(x: f64, y: f64) -> Self {
        PPoint { x, y }
    }
}

/// A bounding box (`PBox`): south-west and north-east corners.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PBox {
    pub sw: PPoint,
    pub ne: PPoint,
}

impl PBox {
    /// `pik_bbox_init`: an empty (inverted) box.
    pub fn init() -> Self {
        PBox {
            sw: PPoint::new(1.0, 1.0),
            ne: PPoint::new(0.0, 0.0),
        }
    }
    /// `pik_bbox_isempty`.
    pub fn is_empty(&self) -> bool {
        self.sw.x > self.ne.x
    }
    /// `pik_bbox_add_xy`.
    pub fn add_xy(&mut self, x: f64, y: f64) {
        if self.is_empty() {
            self.sw.x = x;
            self.sw.y = y;
            self.ne.x = x;
            self.ne.y = y;
            return;
        }
        if x < self.sw.x {
            self.sw.x = x;
        }
        if x > self.ne.x {
            self.ne.x = x;
        }
        if y < self.sw.y {
            self.sw.y = y;
        }
        if y > self.ne.y {
            self.ne.y = y;
        }
    }
    /// `pik_bbox_addbox`.
    pub fn add_box(&mut self, other: &PBox) {
        if self.is_empty() {
            *self = *other;
            return;
        }
        if other.is_empty() {
            return;
        }
        if other.sw.x < self.sw.x {
            self.sw.x = other.sw.x;
        }
        if other.sw.y < self.sw.y {
            self.sw.y = other.sw.y;
        }
        if other.ne.x > self.ne.x {
            self.ne.x = other.ne.x;
        }
        if other.ne.y > self.ne.y {
            self.ne.y = other.ne.y;
        }
    }
    /// `pik_bbox_addellipse`.
    pub fn add_ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64) {
        self.add_xy(x - rx, y - ry);
        self.add_xy(x + rx, y + ry);
    }
}

/// An absolute-or-relative distance (`PRel`): `value = rAbs + value*rRel`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PRel {
    pub abs: f64,
    pub rel: f64,
}

/// A single text item attached to an object (`PObj.aTxt[i]`).
#[derive(Debug, Clone, PartialEq)]
pub struct PText {
    /// The string *with* surrounding quotes (matching `PToken.z/n`).
    pub text: String,
    /// `TP_*` flags.
    pub e_code: i32,
}

/// Object class (`aClass` entries plus the internal sublist/noop pseudo
/// classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Arc,
    Arrow,
    Box,
    Circle,
    Cylinder,
    Diamond,
    Dot,
    Ellipse,
    File,
    Line,
    Move,
    Oval,
    Spline,
    Text,
    Sublist,
    Noop,
}

impl Class {
    /// `isLine` flag.
    pub fn is_line(self) -> bool {
        matches!(self, Class::Arc | Class::Arrow | Class::Line | Class::Spline | Class::Move)
    }
    /// `eJust`: use box-style text justification.
    pub fn e_just(self) -> bool {
        matches!(self, Class::Box | Class::Cylinder | Class::File | Class::Oval)
    }
    pub fn name(self) -> &'static str {
        match self {
            Class::Arc => "arc",
            Class::Arrow => "arrow",
            Class::Box => "box",
            Class::Circle => "circle",
            Class::Cylinder => "cylinder",
            Class::Diamond => "diamond",
            Class::Dot => "dot",
            Class::Ellipse => "ellipse",
            Class::File => "file",
            Class::Line => "line",
            Class::Move => "move",
            Class::Oval => "oval",
            Class::Spline => "spline",
            Class::Text => "text",
            Class::Sublist => "[]",
            Class::Noop => "noop",
        }
    }
    /// Look up a class by source name (`pik_find_class`).
    pub fn from_name(name: &str) -> Option<Class> {
        Some(match name {
            "arc" => Class::Arc,
            "arrow" => Class::Arrow,
            "box" => Class::Box,
            "circle" => Class::Circle,
            "cylinder" => Class::Cylinder,
            "diamond" => Class::Diamond,
            "dot" => Class::Dot,
            "ellipse" => Class::Ellipse,
            "file" => Class::File,
            "line" => Class::Line,
            "move" => Class::Move,
            "oval" => Class::Oval,
            "spline" => Class::Spline,
            "text" => Class::Text,
            _ => return None,
        })
    }
}

/// A single graphics object (`PObj`).
#[derive(Debug, Clone)]
pub struct PObj {
    pub class: Class,
    /// Source span of the reference token, for error messages.
    pub err_span: (usize, usize),
    pub pt_at: PPoint,
    pub pt_enter: PPoint,
    pub pt_exit: PPoint,
    pub sublist: Option<Vec<usize>>,
    pub name: Option<String>,
    pub w: f64,
    pub h: f64,
    pub rad: f64,
    pub sw: f64,
    pub dotted: f64,
    pub dashed: f64,
    pub fill: f64,
    pub color: f64,
    pub with: PPoint,
    pub e_with: u8,
    pub cw: bool,
    pub larrow: bool,
    pub rarrow: bool,
    pub b_close: bool,
    pub b_chop: bool,
    pub b_alt_auto_fit: bool,
    pub txt: Vec<PText>,
    pub m_prop: u32,
    pub m_calc: u32,
    pub i_layer: i32,
    pub in_dir: i32,
    pub out_dir: i32,
    pub path: Vec<PPoint>,
    pub p_from: Option<usize>,
    pub p_to: Option<usize>,
    pub bbox: PBox,
}

impl PObj {
    pub fn blank(class: Class) -> Self {
        PObj {
            class,
            err_span: (0, 0),
            pt_at: PPoint::default(),
            pt_enter: PPoint::default(),
            pt_exit: PPoint::default(),
            sublist: None,
            name: None,
            w: 0.0,
            h: 0.0,
            rad: 0.0,
            sw: 0.0,
            dotted: 0.0,
            dashed: 0.0,
            fill: -1.0,
            color: 0.0,
            with: PPoint::default(),
            e_with: 0,
            cw: false,
            larrow: false,
            rarrow: false,
            b_close: false,
            b_chop: false,
            b_alt_auto_fit: false,
            txt: Vec::new(),
            m_prop: 0,
            m_calc: 0,
            i_layer: 1000,
            in_dir: 0,
            out_dir: 0,
            path: Vec::new(),
            p_from: None,
            p_to: None,
            bbox: PBox::default(),
        }
    }
}
