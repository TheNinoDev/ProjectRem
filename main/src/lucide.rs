//! Lucide icons, rendered with the egui painter.
//!
//! Lucide is a 24x24 stroked icon set (ISC licensed). egui's bundled fonts do
//! not contain these glyphs and SVG support would mean an async texture load
//! per icon, so instead we parse the real Lucide path data once at startup and
//! flatten it into polylines that the painter can stroke directly.
//!
//! The vocabulary is deliberately small - only what Lucide actually uses:
//! `M L C Q A Z` in `d` attributes, plus `<circle> <rect> <line> <ellipse>
//! <polyline> <polygon>`.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Everything the parser needs to emit one polyline.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Quad(f32, f32, f32, f32),
    /// Elliptical arc: rx, ry, x-axis-rotation, large-arc, sweep, x, y
    Arc(f32, f32, f32, bool, bool, f32, f32),
    Close,
}

/// How many line segments one curve is flattened into.
const CURVE_STEPS: usize = 16;
const ARC_STEPS: usize = 24;

// ------------------------------------------------------------------ parsing

/// Split a run of SVG path numbers.
///
/// Two details matter and both are exercised by real Lucide data:
///
/// - a number may start with `.` (`-.5`, `.092`), so a `.` that arrives while a
///   number is already in progress starts a *new* number rather than extending
///   the current one (`1.099.092` is two numbers, not one),
/// - a `-` or `+` is a separator unless it follows `e`/`E` (`1e-5`).
///
/// An unparseable token is skipped rather than turned into a zero: a silent
/// zero shifts every following coordinate and the shape turns into a scribble.
fn parse_numbers(s: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut seen_dot = false;
    let mut seen_exp = false;

    for c in s.chars() {
        match c {
            '0'..='9' => current.push(c),
            // A `.` either continues the number being read, or - when one is
            // already there - starts the next number: `1.099.092` is two
            // numbers, and `-.5` legitimately opens with a dot.
            '.' if !seen_dot && !seen_exp && !current.is_empty() => {
                seen_dot = true;
                current.push(c);
            }
            '.' => {
                out.extend(parse_one(&mut current));
                current.clear();
                seen_dot = true;
                seen_exp = false;
                current.push(c);
            }
            'e' | 'E'
                if !current.is_empty()
                    && !seen_exp
                    && current.contains(|ch: char| ch.is_ascii_digit()) =>
            {
                seen_exp = true;
                current.push(c);
            }
            '-' | '+' if current.is_empty() => current.push(c),
            '-' | '+' if seen_exp => {
                // Sign that belongs to the exponent, e.g. `1e-5`.
                current.push(c);
            }
            '-' | '+' => {
                // Separator: `1-2` is two numbers.
                out.extend(parse_one(&mut current));
                current.clear();
                seen_dot = false;
                seen_exp = false;
                current.push(c);
            }
            _ => {
                out.extend(parse_one(&mut current));
                current.clear();
                seen_dot = false;
                seen_exp = false;
            }
        }
    }
    out.extend(parse_one(&mut current));
    out
}

/// Flush the pending token, dropping it if it does not parse.
fn parse_one(current: &mut String) -> Option<f32> {
    if current.is_empty() {
        return None;
    }
    let token = std::mem::take(current);
    match token.parse::<f32>() {
        Ok(value) if value.is_finite() => Some(value),
        _ => None,
    }
}

/// Parse a `<path d="...">` attribute into absolute segments.
///
/// SVG has absolute (`M L C Q A`) and relative (`m l c q a`) commands that
/// differ only in case. Relative coordinates are resolved here, while the
/// current point is still known, so the flattener only ever deals with
/// absolute values.
fn parse_path(d: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0usize;

    // Current point and start of the current subpath.
    let mut cur = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);
    // Control points of the previous curve, needed by the smooth variants
    // (`S` / `T`), which mirror them.
    let mut last_cubic_ctrl: Option<(f32, f32)> = None;
    let mut last_quad_ctrl: Option<(f32, f32)> = None;
    let mut command = ' ';

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            command = c;
            i += 1;
        } else if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }

        let start = i;
        while i < chars.len() && !chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let n = parse_numbers(&chars[start..i].iter().collect::<String>());

        let relative = command.is_ascii_lowercase();
        let upper = command.to_ascii_uppercase();

        // `Z` takes no arguments, so it has to be handled before the
        // "did we get numbers?" check below.
        if upper == 'Z' {
            segs.push(Seg::Close);
            cur = subpath_start;
            last_cubic_ctrl = None;
            last_quad_ctrl = None;
            continue;
        }
        if n.is_empty() {
            continue;
        }

        match upper {
            'M' => {
                for (index, p) in n.chunks_exact(2).enumerate() {
                    // Per the SVG spec a relative `m` is only relative for the
                    // *first* pair; the rest are relative linetos, chained.
                    let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                    let point = (p[0] + ox, p[1] + oy);
                    if index == 0 {
                        segs.push(Seg::Move(point.0, point.1));
                        subpath_start = point;
                    } else {
                        segs.push(Seg::Line(point.0, point.1));
                    }
                    cur = point;
                }
                // Extra pairs after a moveto are implicit linetos.
                command = if relative { 'l' } else { 'L' };
            }
            'L' => {
                for p in n.chunks_exact(2) {
                    // The offset is read inside the loop: a relative command
                    // with several coordinate pairs chains onto its own output,
                    // not onto the point where the command started.
                    let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                    let point = (p[0] + ox, p[1] + oy);
                    segs.push(Seg::Line(point.0, point.1));
                    cur = point;
                }
            }
            // Relative `h` / `v` only move one axis, so they are emitted as an
            // absolute line that keeps the other coordinate.
            'H' => {
                for x in n {
                    let point = (x + if relative { cur.0 } else { 0.0 }, cur.1);
                    segs.push(Seg::Line(point.0, point.1));
                    cur = point;
                }
            }
            'V' => {
                for y in n {
                    let point = (cur.0, y + if relative { cur.1 } else { 0.0 });
                    segs.push(Seg::Line(point.0, point.1));
                    cur = point;
                }
            }
            'C' => {
                for c in n.chunks_exact(6) {
                    if c.len() == 6 {
                        let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                        let a = (c[0] + ox, c[1] + oy);
                        let b = (c[2] + ox, c[3] + oy);
                        let end = (c[4] + ox, c[5] + oy);
                        segs.push(Seg::Cubic(a.0, a.1, b.0, b.1, end.0, end.1));
                        last_cubic_ctrl = Some(b);
                        cur = end;
                    }
                }
                last_quad_ctrl = None;
            }
            'S' => {
                // The first control point mirrors the previous cubic's second
                // control point about the current point.
                for c in n.chunks_exact(4) {
                    if c.len() == 4 {
                        let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                        let a = match last_cubic_ctrl {
                            Some(prev) => (2.0 * cur.0 - prev.0, 2.0 * cur.1 - prev.1),
                            None => cur,
                        };
                        let b = (c[0] + ox, c[1] + oy);
                        let end = (c[2] + ox, c[3] + oy);
                        segs.push(Seg::Cubic(a.0, a.1, b.0, b.1, end.0, end.1));
                        last_cubic_ctrl = Some(b);
                        cur = end;
                    }
                }
                last_quad_ctrl = None;
            }
            'Q' => {
                for q in n.chunks_exact(4) {
                    if q.len() == 4 {
                        let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                        let a = (q[0] + ox, q[1] + oy);
                        let end = (q[2] + ox, q[3] + oy);
                        segs.push(Seg::Quad(a.0, a.1, end.0, end.1));
                        last_quad_ctrl = Some(a);
                        cur = end;
                    }
                }
                last_cubic_ctrl = None;
            }
            'T' => {
                for q in n.chunks_exact(2) {
                    if q.len() == 2 {
                        let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                        let a = match last_quad_ctrl {
                            Some(prev) => (2.0 * cur.0 - prev.0, 2.0 * cur.1 - prev.1),
                            None => cur,
                        };
                        let end = (q[0] + ox, q[1] + oy);
                        segs.push(Seg::Quad(a.0, a.1, end.0, end.1));
                        last_quad_ctrl = Some(a);
                        cur = end;
                    }
                }
                last_cubic_ctrl = None;
            }
            'A' => {
                for a in n.chunks_exact(7) {
                    if a.len() == 7 {
                        let (ox, oy) = if relative { (cur.0, cur.1) } else { (0.0, 0.0) };
                        let end = (a[5] + ox, a[6] + oy);
                        segs.push(Seg::Arc(
                            a[0],
                            a[1],
                            a[2],
                            a[3] != 0.0,
                            a[4] != 0.0,
                            end.0,
                            end.1,
                        ));
                        cur = end;
                    }
                }
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            _ => {}
        }
    }
    segs
}

/// A single `<path d="...">`, `<circle>`, `<line>`, ... as polylines in 24x24
/// user space.
#[derive(Debug, Clone)]
pub struct Icon {
    pub polylines: Vec<Vec<Pos2>>,
    /// Filled dots (the key hole and friends) with their radius.
    pub dots: Vec<(Pos2, f32)>,
}

impl Icon {
    fn new() -> Self {
        Self { polylines: Vec::new(), dots: Vec::new() }
    }

    fn push_path(&mut self, d: &str) {
        self.polylines.extend(flatten(&parse_path(d)));
    }

    fn push_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) {
        let mut points = Vec::with_capacity(ARC_STEPS + 1);
        for i in 0..=ARC_STEPS {
            let a = i as f32 / ARC_STEPS as f32 * std::f32::consts::TAU;
            points.push(Pos2::new(cx + a.cos() * rx, cy + a.sin() * ry));
        }
        self.polylines.push(points);
    }

    fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let r = r.min(w / 2.0).min(h / 2.0);
        if r <= 0.01 {
            self.polylines.push(vec![
                Pos2::new(x, y),
                Pos2::new(x + w, y),
                Pos2::new(x + w, y + h),
                Pos2::new(x, y + h),
                Pos2::new(x, y),
            ]);
            return;
        }
        let mut points = Vec::with_capacity(4 * 6 + 2);
        let corners = [
            (x + w - r, y + r, -std::f32::consts::FRAC_PI_2, 0.0),
            (x + w - r, y + h - r, 0.0, std::f32::consts::FRAC_PI_2),
            (x + r, y + h - r, std::f32::consts::FRAC_PI_2, std::f32::consts::PI),
            (x + r, y + r, std::f32::consts::PI, 3.0 * std::f32::consts::FRAC_PI_2),
        ];
        for (cx, cy, from, to) in corners {
            for i in 0..=6 {
                let a = from + (to - from) * (i as f32 / 6.0);
                points.push(Pos2::new(cx + a.cos() * r, cy + a.sin() * r));
            }
        }
        points.push(points[0]);
        self.polylines.push(points);
    }
}

/// Turns path segments into polylines.
///
/// All segments are absolute by the time they get here; `parse_path` resolves
/// the relative SVG commands while it still knows the current point.
fn flatten(segs: &[Seg]) -> Vec<Vec<Pos2>> {
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut current: Vec<Pos2> = Vec::new();
    let mut pos = Pos2::ZERO;
    let mut subpath_start = Pos2::ZERO;

    for seg in segs {
        match *seg {
            Seg::Move(x, y) => {
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                pos = Pos2::new(x, y);
                subpath_start = pos;
                current.push(pos);
            }
            Seg::Line(x, y) => {
                let next = Pos2::new(x, y);
                current.push(next);
                pos = next;
            }
            Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                let next = Pos2::new(x, y);
                let (c1, c2) = (Pos2::new(c1x, c1y), Pos2::new(c2x, c2y));
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    current.push(cubic_at(pos, c1, c2, next, t));
                }
                pos = next;
            }
            Seg::Quad(qx, qy, x, y) => {
                let next = Pos2::new(x, y);
                let q = Pos2::new(qx, qy);
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    current.push(quad_at(pos, q, next, t));
                }
                pos = next;
            }
            Seg::Arc(rx, ry, rotation, large_arc, sweep, x, y) => {
                let next = Pos2::new(x, y);
                current.extend(arc_points(pos, next, rx, ry, rotation, large_arc, sweep));
                pos = next;
            }
            Seg::Close => {
                current.push(subpath_start);
                pos = subpath_start;
            }
        }
    }

    if current.len() > 1 {
        out.push(current);
    }
    out
}

fn cubic_at(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    Pos2::new(
        a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    )
}

fn quad_at(p0: Pos2, p1: Pos2, p2: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    Pos2::new(
        u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
        u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
    )
}

/// SVG endpoint -> centre parameterisation, then sampled.
fn arc_points(
    from: Pos2,
    to: Pos2,
    rx: f32,
    ry: f32,
    rotation_deg: f32,
    large_arc: bool,
    sweep: bool,
) -> Vec<Pos2> {
    if (to - from).length() < 1e-6 {
        return Vec::new();
    }
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx < 1e-6 || ry < 1e-6 {
        return vec![to];
    }

    let phi = rotation_deg.to_radians();
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // Scale up the radii if they are too small to span the chord.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let sign = if large_arc != sweep { 1.0 } else { -1.0 };
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let co = if den.abs() < 1e-12 { 0.0 } else { sign * (num.max(0.0) / den).sqrt() };

    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (from.x + to.x) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (from.y + to.y) / 2.0;

    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }

    (1..=ARC_STEPS)
        .map(|i| {
            let t = theta1 + delta * (i as f32 / ARC_STEPS as f32);
            let px = rx * t.cos();
            let py = ry * t.sin();
            Pos2::new(
                cos_phi * px - sin_phi * py + cx,
                sin_phi * px + cos_phi * py + cy,
            )
        })
        .collect()
}

// -------------------------------------------------------------- icon table

/// Parses one Lucide `<svg>` body into an [`Icon`].
fn parse_svg(body: &str) -> Icon {
    let mut icon = Icon::new();
    let mut rest = body;

    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>').map(|e| start + e) else { break };
        let tag = &rest[start..end + 1];
        rest = &rest[end + 1..];

        let name = tag
            .trim_start_matches('<')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");

        let attr = |key: &str| -> Option<String> {
            let needle = format!("{key}=\"");
            let at = tag.find(&needle)? + needle.len();
            let close = tag[at..].find('"')? + at;
            Some(tag[at..close].to_owned())
        };
        let num = |key: &str| -> f32 { attr(key).and_then(|v| v.parse().ok()).unwrap_or(0.0) };

        match name {
            "path" => {
                if let Some(d) = attr("d") {
                    icon.push_path(&d);
                }
            }
            "circle" => {
                let (cx, cy, r) = (num("cx"), num("cy"), num("r"));
                if is_filled(tag) {
                    // Lucide uses a couple of filled dots, e.g. the key hole.
                    icon.dots.push((Pos2::new(cx, cy), r));
                } else {
                    icon.push_ellipse(cx, cy, r, r);
                }
            }
            "ellipse" => icon.push_ellipse(num("cx"), num("cy"), num("rx"), num("ry")),
            "rect" => {
                icon.push_rect(num("x"), num("y"), num("width"), num("height"), num("rx"));
            }
            "line" => icon.polylines.push(vec![
                Pos2::new(num("x1"), num("y1")),
                Pos2::new(num("x2"), num("y2")),
            ]),
            "polyline" | "polygon" => {
                if let Some(points) = attr("points") {
                    let n = parse_numbers(&points);
                    let pts: Vec<Pos2> =
                        n.chunks_exact(2).map(|p| Pos2::new(p[0], p[1])).collect();
                    if pts.len() > 1 {
                        let mut pts = pts;
                        if name == "polygon" {
                            pts.push(pts[0]);
                        }
                        icon.polylines.push(pts);
                    }
                }
            }
            _ => {}
        }
    }
    icon
}

/// `fill="currentColor"` (rather than `fill="none"`) means "draw this solid".
fn is_filled(tag: &str) -> bool {
    match tag.find("fill=\"") {
        Some(at) => !tag[at + 6..].starts_with("none"),
        None => false,
    }
}

macro_rules! icons {
    ($($name:ident => $svg:expr),* $(,)?) => {
        /// Every icon the app knows about. The full Lucide set is generated
        /// in, so not every variant is referenced yet.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(dead_code)]
        pub enum Name { $($name),* }

        impl Name {
            fn build(self) -> Icon {
                match self { $(Name::$name => parse_svg($svg)),* }
            }
        }
    };
}

include!("icons_data.rs");

/// The parsed icon, cached per name so the SVG is only walked once.
pub fn icon(name: Name) -> &'static Icon {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<Name, &'static Icon>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    guard.entry(name).or_insert_with(|| Box::leak(Box::new(name.build())))
}

/// Draw `name` centred in `rect`, scaled from Lucide's 24x24 grid.
pub fn paint(painter: &Painter, name: Name, rect: Rect, color: Color32, width: f32) {
    let icon = icon(name);
    let scale = (rect.width().min(rect.height())) / 24.0;
    let stroke = Stroke::new((width * scale).max(0.6), color);
    let origin = rect.min;
    let map = |p: Pos2| Pos2::new(origin.x + p.x * scale, origin.y + p.y * scale);

    for polyline in &icon.polylines {
        if polyline.len() < 2 {
            continue;
        }
        let points: Vec<Pos2> = polyline.iter().copied().map(map).collect();
        if polyline.len() == 2 {
            // Straight segments can be one cheap shape.
            painter.add(egui::Shape::closed_line(points, stroke));
        } else {
            painter.add(egui::Shape::line(points, stroke));
        }
    }
    for (dot, r) in &icon.dots {
        painter.circle_filled(map(*dot), r.max(1.0) * scale, color);
    }
}

/// Allocate `size x size` in `ui` and paint `name` into it.
/// Returns the response so callers can still make it clickable.
#[allow(dead_code, reason = "handy for inline icon slots")]
pub fn allocate(ui: &mut egui::Ui, name: Name, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint(ui.painter(), name, rect, color, 2.0);
    response
}

#[cfg(test)]
mod tests {
    use super::*;


    /// Every icon the app knows about, so the tests can cover the whole
    /// generated set instead of a hand-picked few.
    const ALL_ICONS: &[Name] = &[
            Name::MessageCircle,
            Name::Bot,
            Name::User,
            Name::Settings,
            Name::Puzzle,
            Name::Home,
            Name::ChevronDown,
            Name::ChevronRight,
            Name::ChevronLeft,
            Name::Hash,
            Name::Search,
            Name::X,
            Name::Check,
            Name::Plus,
            Name::Minus,
            Name::Download,
            Name::Folder,
            Name::FolderOpen,
            Name::HardDrive,
            Name::RefreshCw,
            Name::ExternalLink,
            Name::Cpu,
            Name::Database,
            Name::DatabaseZap,
            Name::Trash,
            Name::Trash2,
            Name::Save,
            Name::Volume2,
            Name::Mic,
            Name::Headphones,
            Name::Languages,
            Name::Globe,
            Name::Info,
            Name::Monitor,
            Name::Clock,
            Name::Activity,
            Name::UserPlus,
            Name::LogIn,
            Name::LogOut,
            Name::KeyRound,
            Name::Github,
            Name::Image,
            Name::ShieldCheck,
            Name::LoaderCircle,
            Name::CircleUser,
            Name::AtSign,
            Name::Link,
            Name::Unlink,
            Name::Eye,
            Name::EyeOff,
            Name::Power,
            Name::Sparkles,
            Name::CircleAlert,
            Name::CircleCheck,
            Name::TriangleAlert,
            Name::SquareArrowOutUpRight,
            Name::Type,
            Name::List,
            Name::LayoutGrid,
            Name::Wifi,
            Name::WifiOff,
    ];
    fn a_circle_becomes_a_ring() {
        let parsed = Name::User.build();
        // `<circle r="4">` plus one path for the shoulders.
        assert!(parsed.polylines.len() >= 2);
        let ring = parsed
            .polylines
            .iter()
            .find(|p| {
                p.iter().all(|q| (q.x - 12.0).abs() < 4.2 && (q.y - 7.0).abs() < 4.2)
            })
            .expect("the head circle should be present");
        assert!(ring.len() > 8, "a circle should be flattened into many points");
    }

    #[test]
    fn arcs_are_flattened_not_dropped() {
        // `refresh-cw` is built almost entirely from elliptical arcs.
        let parsed = Name::RefreshCw.build();
        let total: usize = parsed.polylines.iter().map(|p| p.len()).sum();
        assert!(total > 40, "arcs were not flattened: only {total} points");
    }

    #[test]
    fn numbers_parse_with_signs_and_decimals() {
        let n = parse_numbers("-4.25 3 .5 1e2");
        assert_eq!(n.len(), 4);
        assert!((n[0] + 4.25).abs() < 1e-5);
        assert!((n[2] - 0.5).abs() < 1e-5);
        assert!((n[3] - 100.0).abs() < 1e-2);
    }

    /// Lucide packs numbers without separators, and a second `.` starts a new
    /// number. Reading `1.099.092` as one token silently produced a zero, which
    /// shifted every following coordinate - the icons rendered as scribbles.
    #[test]
    fn a_second_dot_starts_a_new_number() {
        let n = parse_numbers("1.099.092");
        assert_eq!(n.len(), 2, "got {n:?}");
        assert!((n[0] - 1.099).abs() < 1e-5);
        assert!((n[1] - 0.092).abs() < 1e-5);
    }

    #[test]
    fn packed_and_separated_forms_agree() {
        let packed = parse_numbers("10 10 0 1 0-4.777-4.719");
        assert_eq!(packed.len(), 7);
        assert!((packed[5] + 4.777).abs() < 1e-5);
        assert!((packed[6] + 4.719).abs() < 1e-5);

        // `l3.413-.998` and `a2 2 0 0 1 1.099.092` straight out of Lucide.
        assert_eq!(parse_numbers("3.413-.998"), vec![3.413, -0.998]);
        assert_eq!(parse_numbers("1.099.092"), vec![1.099, 0.092]);
        assert_eq!(parse_numbers("-.5"), vec![-0.5]);
        assert_eq!(parse_numbers("1e-3"), vec![0.001]);
    }

    #[test]
    fn an_unparseable_token_is_dropped_not_zeroed() {
        // A zero here would silently displace the rest of the path.
        assert_eq!(parse_numbers("1..5"), vec![1.0, 0.5]);
        assert!(parse_numbers("nan").is_empty());
    }

    /// Every real icon must stay inside its 24x24 box. This is the check that
    /// caught the packed-number bug, so it covers the whole set rather than the
    /// handful of icons a visual spot-check would look at.
    #[test]
    fn every_icon_stays_inside_its_viewbox() {
        for name in ALL_ICONS {
            let parsed = name.build();
            assert!(
                !parsed.polylines.is_empty() || !parsed.dots.is_empty(),
                "{name:?} parsed to nothing"
            );
            for polyline in &parsed.polylines {
                assert!(polyline.len() >= 2, "{name:?} has a degenerate polyline");
                for point in polyline {
                    assert!(
                        (-0.5..=24.5).contains(&point.x) && (-0.5..=24.5).contains(&point.y),
                        "{name:?} point {point:?} escaped the 24x24 viewBox"
                    );
                }
            }
        }
    }

    #[test]
    fn message_circle_is_a_round_bubble() {
        // Regression: the closing `a10 10 0 1 0-4.777-4.719` arc used to end at
        // (6.7, 31.0), which is outside the icon entirely.
        let parsed = Name::MessageCircle.build();
        let all: Vec<egui::Pos2> = parsed.polylines.concat();
        let bounds = |f: fn(&egui::Pos2) -> f32| {
            all.iter().map(f).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let (x0, x1) = bounds(|p| p.x);
        let (y0, y1) = bounds(|p| p.y);
        assert!((x0 - 2.0).abs() < 0.2 && (x1 - 22.0).abs() < 0.2, "x {x0:.2}..{x1:.2}");
        assert!((y0 - 2.0).abs() < 0.2 && (y1 - 22.0).abs() < 0.2, "y {y0:.2}..{y1:.2}");
    }

    /// Relative commands (`m l c q a h v s t z`) differ from absolute ones only
    /// in case. Treating them as absolute is a silent, hard-to-spot bug, so the
    /// resolution is pinned down here.
    #[test]
    fn relative_commands_are_resolved_against_the_current_point() {
        // m 10 10 l 5 0  =>  (10,10) -> (15,10)
        let segs = parse_path("m10 10 l5 0");
        assert_eq!(segs.len(), 2);
        match (segs[0], segs[1]) {
            (Seg::Move(x, y), Seg::Line(ex, ey)) => {
                assert!((x - 10.0).abs() < 1e-5 && (y - 10.0).abs() < 1e-5);
                assert!((ex - 15.0).abs() < 1e-5 && (ey - 10.0).abs() < 1e-5);
            }
            other => panic!("unexpected segments: {other:?}"),
        }
    }

    #[test]
    fn relative_linetos_after_a_moveto_chain() {
        // Per the SVG spec the pairs after a relative `m` are relative linetos,
        // each chained onto the previous point, not all relative to the origin.
        let segs = parse_path("m10 10 5 0 5 0");
        assert_eq!(segs.len(), 3);
        match (segs[1], segs[2]) {
            (Seg::Line(x1, _), Seg::Line(x2, _)) => {
                assert!((x1 - 15.0).abs() < 1e-5, "first lineto at {x1}, want 15");
                assert!((x2 - 20.0).abs() < 1e-5, "second lineto at {x2}, want 20");
            }
            other => panic!("unexpected segments: {other:?}"),
        }
    }

    #[test]
    fn relative_h_and_v_keep_the_other_axis() {
        // h5 from (3,7) is (8,7); v4 after that is (8,11).
        let segs = parse_path("M3 7 h5 v4");
        assert_eq!(segs.len(), 3);
        match (segs[1], segs[2]) {
            (Seg::Line(x1, y1), Seg::Line(x2, y2)) => {
                assert!((x1 - 8.0).abs() < 1e-5 && (y1 - 7.0).abs() < 1e-5);
                assert!((x2 - 8.0).abs() < 1e-5 && (y2 - 11.0).abs() < 1e-5);
            }
            other => panic!("unexpected segments: {other:?}"),
        }
    }

    #[test]
    fn close_returns_to_the_subpath_start() {
        // A `Z` carries no coordinates, so it must not be skipped as "no args".
        let segs = parse_path("M4 4 L20 4 L20 20 Z");
        assert_eq!(segs.len(), 4);
        assert!(matches!(segs[3], Seg::Close));

        let lines = flatten(&segs);
        assert_eq!(lines.len(), 1);
        let closed = &lines[0];
        assert_eq!(*closed.first().unwrap(), Pos2::new(4.0, 4.0));
        assert_eq!(*closed.last().unwrap(), Pos2::new(4.0, 4.0), "Z must close the loop");
    }

    #[test]
    fn absolute_and_relative_paths_land_in_the_same_place() {
        // Same geometry, written two ways. If the parser is right they must
        // flatten to matching polylines.
        let absolute = flatten(&parse_path("M6 6 h12 v12 h-12 Z"));
        let relative = flatten(&parse_path("m6 6 h12 v12 h-12 z"));
        assert_eq!(absolute.len(), 1);
        assert_eq!(relative.len(), 1);
        assert_eq!(absolute[0].len(), relative[0].len());
        for (a, b) in absolute[0].iter().zip(relative[0].iter()) {
            assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn dump() {
        let d = r#"M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719"#;
        for s in parse_path(d) {
            println!("{s:?}");
        }
    }

    #[test]
    fn the_user_icon_stays_inside_its_box() {
        // Regression: the shoulders path uses a relative `a`, and parsing it
        // as absolute once pushed the geometry to y = -10.
        let parsed = Name::User.build();
        for polyline in &parsed.polylines {
            for point in polyline {
                assert!(
                    (0.0..=24.0).contains(&point.x) && (0.0..=24.0).contains(&point.y),
                    "point {point:?} left the 24x24 box"
                );
            }
        }
    }

    /// A relative command may carry several parameter sets at once. Each one is
    /// relative to the point the previous one ended at - not to the point the
    /// command started at. Getting this wrong misplaces every segment after the
    /// first, which is what happened to the globe's four chained arcs.
    #[test]
    fn chained_relative_commands_advance_the_current_point() {
        // The four arcs of Lucide's globe, which must land on the cardinal
        // points of the 12..22 / 2..12 box.
        let segs = parse_path(
            "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
        );
        let ends: Vec<(f32, f32)> = segs
            .iter()
            .filter_map(|s| match *s {
                Seg::Arc(_, _, _, _, _, x, y) => Some((x, y)),
                _ => None,
            })
            .collect();
        assert_eq!(ends.len(), 4);
        for (got, want) in ends.iter().zip([(16.0, 12.0), (12.0, 22.0), (8.0, 12.0), (12.0, 2.0)]) {
            assert!(
                (got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3,
                "arc ended at {got:?}, expected {want:?}"
            );
        }
    }

    /// Same chaining rule for curves.
    #[test]
    fn chained_relative_curves_advance_the_current_point() {
        // Two relative cubics: the second starts where the first ended.
        let segs = parse_path("M2 2 c4 0 6 4 6 8 c0 4 2 8 6 8");
        let ends: Vec<(f32, f32)> = segs
            .iter()
            .filter_map(|s| match *s {
                Seg::Cubic(_, _, _, _, x, y) => Some((x, y)),
                _ => None,
            })
            .collect();
        assert_eq!(ends.len(), 2);
        assert!((ends[0].0 - 8.0).abs() < 1e-3 && (ends[0].1 - 10.0).abs() < 1e-3);
        assert!((ends[1].0 - 14.0).abs() < 1e-3 && (ends[1].1 - 18.0).abs() < 1e-3);
    }
}

