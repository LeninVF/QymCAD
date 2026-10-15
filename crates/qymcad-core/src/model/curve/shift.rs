//! A CURVE MOVED AND A CURVE OFFSET: taken through a map of the sheet - a shift, a turn, a mirror, any of them after
//! another - and laid at a distance to one side of itself.
use super::*;
use crate::feature::Ends;

/// A MAP OF THE SHEET that keeps lengths: a point `p` goes to `origin + p.x * x + p.y * y`, `x` and `y` square unit
/// vectors - turned, or turned over for a mirror.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map {
    pub origin: Point2,
    pub x: Point2,
    pub y: Point2,
}

impl Map {
    /// A shift by `by`.
    pub fn shift(by: Point2) -> Map {
        Map { origin: by, x: Point2::new(1.0, 0.0), y: Point2::new(0.0, 1.0) }
    }

    /// A turn by `angle` radians about `centre`.
    pub fn turn(centre: Point2, angle: f64) -> Map {
        let (s, c) = angle.sin_cos();
        let (x, y) = (Point2::new(c, s), Point2::new(-s, c));
        Map { origin: Point2::new(centre.x - (centre.x * x.x + centre.y * y.x), centre.y - (centre.x * x.y + centre.y * y.y)), x, y }
    }

    /// A mirror about the line through `a` and `b`.
    pub fn mirror(line: [Point2; 2]) -> Map {
        let [a, b] = line;
        let d = b - a;
        let l = d.len().max(1e-300);
        let (ux, uy) = (d.x / l, d.y / l);
        // reflection across a line through the origin of direction u: [[2ux^2-1, 2uxuy], [2uxuy, 2uy^2-1]]
        let (x, y) = (Point2::new(2.0 * ux * ux - 1.0, 2.0 * ux * uy), Point2::new(2.0 * ux * uy, 2.0 * uy * uy - 1.0));
        Map { origin: Point2::new(a.x - (a.x * x.x + a.y * y.x), a.y - (a.x * x.y + a.y * y.y)), x, y }
    }

    /// Where point `p` goes.
    pub fn point(&self, p: Point2) -> Point2 {
        Point2::new(self.origin.x + p.x * self.x.x + p.y * self.y.x, self.origin.y + p.x * self.x.y + p.y * self.y.y)
    }

    /// Where vector `v` goes: turned, the shift left out.
    pub fn vector(&self, v: Point2) -> Point2 {
        Point2::new(v.x * self.x.x + v.y * self.y.x, v.x * self.x.y + v.y * self.y.y)
    }

    /// Whether the map turns the sheet over - a mirror - and every way round goes the other way.
    pub fn turns_over(&self) -> bool {
        self.x.x * self.y.y - self.x.y * self.y.x < 0.0
    }
}

/// The least distance from `p` to a polyline.
fn off_polyline(p: Point2, line: &[Point2]) -> f64 {
    line.windows(2).map(|w| Curve::Line { a: w[0], b: w[1] }.nearest(p).off).fold(f64::MAX, f64::min)
}

/// The tolerance an offset of an ellipse or a spline is laid within, in millimetres.
pub const OFFSET_TOLERANCE: f64 = 0.01;

impl Curve {
    /// THE CURVE TAKEN THROUGH `map`, the same kind: an arc turned over runs the other way round.
    pub fn mapped(&self, map: &Map) -> Curve {
        match self {
            Curve::Line { a, b } => Curve::Line { a: map.point(*a), b: map.point(*b) },
            Curve::Round { centre, r, from, sweep } => {
                let start = map.vector(Point2::new(from.cos(), from.sin()));
                let sweep = if map.turns_over() { -sweep } else { *sweep };
                Curve::Round { centre: map.point(*centre), r: *r, from: start.y.atan2(start.x), sweep }
            }
            Curve::Oval { centre, major, minor, from, sweep } => Curve::Oval { centre: map.point(*centre), major: map.vector(*major), minor: map.vector(*minor), from: *from, sweep: *sweep },
            Curve::Spline { nodes, tangents, ends } => Curve::Spline {
                nodes: nodes.iter().map(|p| map.point(*p)).collect(),
                tangents: tangents.iter().map(|t| NodeTangents { arrive: map.vector(t.arrive), leave: map.vector(t.leave) }).collect(),
                ends: *ends,
            },
        }
    }

    /// THE CURVE LAID `d` TO THE LEFT OF ITSELF - looking along it; a negative `d` to the right: a line and an arc or a
    /// circle exactly, one curve of the same kind; an ellipse and a spline - whose offsets are no ellipse and no spline -
    /// as splines through points of the offset, as many as keep every span within `OFFSET_TOLERANCE` of the true offset. Where the curve bends tighter than `d` its true offset folds into a loop; the loop is cut away at the
    /// place the offset crosses itself, and the offset is the splines on either side of that corner, as every sketcher
    /// trims it; near a free end the part standing nearer the curve than `d` is cut away too. Nothing where the offset
    /// folds away whole: an arc offset past its centre.
    pub fn offset(&self, d: f64) -> Vec<Curve> {
        match self {
            Curve::Line { a, b } => {
                let t = self.tangent(0.0);
                let n = Point2::new(-t.y * d, t.x * d);
                vec![Curve::Line { a: *a + n, b: *b + n }]
            }
            Curve::Round { centre, r, from, sweep } => {
                // the left of a way round counter-clockwise is its centre
                let r2 = if *sweep >= 0.0 { r - d } else { r + d };
                if r2 > 1e-9 {
                    vec![Curve::Round { centre: *centre, r: r2, from: *from, sweep: *sweep }]
                } else {
                    Vec::new()
                }
            }
            Curve::Oval { .. } | Curve::Spline { .. } => {
                let true_at = |t: f64| {
                    let (p, u) = (self.at(t), self.tangent(t));
                    Point2::new(p.x - u.y * d, p.y + u.x * d)
                };
                let closed = self.closed();
                // the true offset, densely, its loops cut away: what the splines are laid through and held to
                let mut dense: Vec<Point2> = (0..=2000).map(|k| true_at(k as f64 / 2000.0)).collect();
                if !closed {
                    // near a free end bending tighter than `d` the offset turns back towards the curve without crossing
                    // itself: on the S of the probes it began 2.981 mm from the curve offset by 3. What stands nearer
                    // than `d` is cut from either end
                    let near = |q: &Point2| self.nearest(*q).off < d.abs() - 1e-6;
                    let head = dense.iter().take_while(|q| near(q)).count();
                    let tail = dense[head..].iter().rev().take_while(|q| near(q)).count();
                    dense.truncate(dense.len() - tail);
                    dense.drain(..head);
                    if dense.len() < 2 {
                        return Vec::new();
                    }
                }
                let ends = if closed { Ends::Closed } else { Ends::Open };
                let runs = without_loops(dense, ends);
                let many = runs.len() > 1;
                runs.iter().map(|run| spline_through(run, if many { Ends::Open } else { ends })).collect()
            }
        }
    }
}

/// THE RUNS OF A POLYLINE LEFT WHEN ITS LOOPS ARE CUT AWAY: where a piece crosses a later one, the points between the
/// two crossings go and the crossing becomes a corner, ending one run and starting the next; a loop inside a loop goes
/// with the outer one. A closed polyline that crosses itself is opened at its first corner.
///
/// The crossings are found among the pieces sharing a cell of a grid as wide as the longest piece, not among all pairs:
/// 2 000 pieces are four million pairs, and a preview lays the offset out every frame.
fn without_loops(mut pts: Vec<Point2>, ends: Ends) -> Vec<Vec<Point2>> {
    let closed = ends == Ends::Closed;
    if closed {
        pts.push(pts[0]);
    }
    let n = pts.len();
    let cell = pts.windows(2).map(|w| w[0].dist(w[1])).fold(1e-9, f64::max);
    let key = |p: Point2| Cell { x: (p.x / cell).floor() as i64, y: (p.y / cell).floor() as i64 };
    let mut grid: std::collections::HashMap<Cell, Vec<usize>> = std::collections::HashMap::new();
    for k in 0..n - 1 {
        let (a, b) = (key(pts[k]), key(pts[k + 1]));
        for x in a.x.min(b.x)..=a.x.max(b.x) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                grid.entry(Cell { x, y }).or_default().push(k);
            }
        }
    }
    // every crossing of a piece with a later one not next to it: the two pieces and the point
    let mut cuts: Vec<Cut> = Vec::new();
    for list in grid.values() {
        for (x, &i) in list.iter().enumerate() {
            for &j in &list[x + 1..] {
                let (i, j) = (i.min(j), i.max(j));
                if j <= i + 1 || (closed && i == 0 && j + 2 == n) || cuts.iter().any(|c| c.first == i && c.last == j) {
                    continue;
                }
                if let Some(at) = crossing([[pts[i], pts[i + 1]], [pts[j], pts[j + 1]]]) {
                    cuts.push(Cut { first: i, last: j, at });
                }
            }
        }
    }
    // the outer loops: from the first piece on, the crossing reaching farthest, the next taken past its end
    cuts.sort_by(|a, b| a.first.cmp(&b.first).then(b.last.cmp(&a.last)));
    let mut outer: Vec<Cut> = Vec::new();
    for c in cuts {
        if outer.last().is_none_or(|o| c.first > o.last) {
            outer.push(c);
        }
    }
    if outer.is_empty() {
        if closed {
            pts.pop();
        }
        return vec![pts];
    }
    let mut out = Vec::new();
    let mut run = vec![pts[0]];
    let mut k = 0;
    for c in &outer {
        run.extend_from_slice(&pts[k + 1..=c.first]);
        run.push(c.at);
        out.push(std::mem::replace(&mut run, vec![c.at]));
        k = c.last;
    }
    run.extend_from_slice(&pts[k + 1..]);
    out.push(run);
    if closed && out.len() > 1 {
        // the run past the last corner goes on round into the run before the first
        let tail = out.pop().unwrap_or_default();
        let head = out.remove(0);
        let mut joined = tail;
        joined.extend_from_slice(&head[1..]);
        out.insert(0, joined);
    }
    out.retain(|r| r.len() >= 2);
    out
}

/// A CELL OF THE GRID the pieces of a polyline are sorted into, by its column and row.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Cell {
    x: i64,
    y: i64,
}

/// A CROSSING OF A POLYLINE WITH ITSELF: the earlier piece, the later one, and the point.
struct Cut {
    first: usize,
    last: usize,
    at: Point2,
}

/// Where two straight pieces cross strictly inside both, if they do.
fn crossing([a, b]: [[Point2; 2]; 2]) -> Option<Point2> {
    let (r, s) = (a[1] - a[0], b[1] - b[0]);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-18 {
        return None;
    }
    let q = b[0] - a[0];
    let (t, u) = ((q.x * s.y - q.y * s.x) / den, (q.x * r.y - q.y * r.x) / den);
    (t > 1e-12 && t < 1.0 - 1e-12 && u > 1e-12 && u < 1.0 - 1e-12).then(|| Point2::new(a[0].x + r.x * t, a[0].y + r.y * t))
}

/// A SPLINE THROUGH POINTS OF `run`, chosen evenly along it and doubled until every span stands within
/// `OFFSET_TOLERANCE` of the run at a quarter, a half and three quarters of it - a loop when `closed`. By the middles
/// alone a span ending at a corner of the run strayed 0.019 mm past it.
fn spline_through(run: &[Point2], ends: Ends) -> Curve {
    let closed = ends == Ends::Closed;
    let mut along = vec![0.0];
    for w in run.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + w[0].dist(w[1]));
    }
    let total = along.last().copied().unwrap_or(0.0).max(1e-12);
    let at_length = |l: f64| {
        let k = along.partition_point(|x| *x < l).clamp(1, run.len() - 1);
        let (l0, l1) = (along[k - 1], along[k]);
        let share = if l1 > l0 { (l - l0) / (l1 - l0) } else { 0.0 };
        Point2::new(run[k - 1].x + (run[k].x - run[k - 1].x) * share, run[k - 1].y + (run[k].y - run[k - 1].y) * share)
    };
    let mut n = 16;
    loop {
        let count = if closed { n } else { n + 1 };
        let nodes: Vec<Point2> = (0..count).map(|k| at_length(total * k as f64 / n as f64)).collect();
        let tangents = (0..nodes.len()).map(|i| tess::spline_tangents_at(&nodes, &[], i, ends)).collect();
        let laid = Curve::Spline { nodes, tangents, ends };
        let spans = laid.spans();
        let worst = (0..spans).flat_map(|k| [0.25, 0.5, 0.75].map(|q| (k as f64 + q) / spans as f64)).map(|t| off_polyline(laid.at(t), run)).fold(0.0, f64::max);
        if worst <= OFFSET_TOLERANCE || n >= 4096 {
            return laid;
        }
        n *= 2;
    }
}
