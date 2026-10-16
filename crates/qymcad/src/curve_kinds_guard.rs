//! Guard on THE KINDS OF CURVE: a kind of curve of a sketch is told apart in the curve module alone - `model/curve.rs`
//! and `model/curve/` - and nowhere else, the code and its checks alike. Zero, with no marks and no list of old places.
//!
//! A tool that matched on the kinds itself took a line, a circle and an arc and left out what came later: a spline and
//! an ellipse were no curve to the trim, the fillet, the snaps, the pick and the drawing, each of them in its own way.
//! Read through the parts of a kind (`as_line` and the like), laid from them (`LineParts { a, b }.into()`) or asked of
//! the curve module (`circle_by`, `point_on`, `Curve::of`), a curve is one geometry everywhere.
//!
//! WHAT IS COUNTED is a variant of the kind named outside the module - the kind, two colons and a capital, in a pattern
//! or a constructor - and an alias that would hide one (the kind renamed by `as`). A function of the kind, its name in
//! small letters (`spline`, `laid`), is the module's own door and is not counted.
//!
//! THE SAME FOR THE GEOMETRY OF A CURVE (`Curve` of `model/curve`): its variants are named in the curve module alone. The
//! cut of a curve into pieces of its kind, its offset, its edges of a profile, its arrangement into regions and the
//! curve a constraint of the solver refers to are functions of the module, and the operations call them. Outside it, by
//! name and for a reason: the probes, whose oracles read the kind of what a tool made; `gui/session/space.rs`, where the
//! session hands a curve out in words to a script; and `qymcad-meshfit` with `qymcad-kernel/src/recognise.rs`, whose
//! `Curve` is another type of the same name - the curve fitted to an edge of a mesh.

#[cfg(test)]
mod tests {
    /// The places in `text` that name a variant of a kind of curve, or alias the kind, as `line: column` pairs read by a
    /// person.
    fn kinds_named(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let mut from = 0;
            while let Some(k) = line[from..].find("EntityKind") {
                let at = from + k;
                let rest = &line[at + "EntityKind".len()..];
                let variant = rest.strip_prefix("::").is_some_and(|v| v.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
                let alias = rest.trim_start().starts_with("as ");
                if variant || alias {
                    out.push(format!("{}:{}", n + 1, at + 1));
                }
                from = at + "EntityKind".len();
            }
        }
        out
    }

    /// The places in `text` that name a variant of the geometry of a curve - the type, two colons and a capital - as
    /// `line: column` pairs. A name that only ends in `Curve` (`ArrCurve`, `EdgeCurve`) is another type and is not counted.
    fn curves_named(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let mut from = 0;
            while let Some(k) = line[from..].find("Curve::") {
                let at = from + k;
                let own = line[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                let variant = line[at + "Curve::".len()..].chars().next().is_some_and(|c| c.is_ascii_uppercase());
                if own && variant {
                    out.push(format!("{}:{}", n + 1, at + 1));
                }
                from = at + "Curve::".len();
            }
        }
        out
    }

    /// Whether `path` may name a variant of the geometry of a curve, outside the curve module: a probe, the session that
    /// hands a curve out in words, or a crate whose `Curve` is another type (see the head of this file).
    fn may_name_curves(path: &std::path::Path) -> bool {
        let p = path.to_string_lossy().replace('\\', "/");
        let probe = p.contains("/tests/") || p.ends_with("tests.rs");
        probe || p.ends_with("qymcad/src/gui/session/space.rs") || p.contains("qymcad-meshfit/") || p.ends_with("qymcad-kernel/src/recognise.rs")
    }

    /// Whether `path` is the curve module, where the kinds are told apart.
    fn the_curve_module(path: &std::path::Path) -> bool {
        let p = path.to_string_lossy().replace('\\', "/");
        p.ends_with("qymcad-core/src/model/curve.rs") || p.contains("qymcad-core/src/model/curve/")
    }

    #[test]
    fn the_signal_catches_a_variant_and_an_alias_and_spares_a_door() {
        let kind = "EntityKind";
        let text = format!("let a = {kind}::Line {{ a, b }};\nuse qymcad_core::model::{kind} as K;\nlet s = {kind}::spline(n, h, e);\nfn f(k: &{kind}) {{}}\n");
        assert_eq!(kinds_named(&text), ["1:9", "2:25"], "a variant and an alias are counted; a function of the kind and the type itself are not");
    }

    #[test]
    fn the_signal_catches_a_variant_of_a_curve_and_spares_another_type() {
        let curve = "Curve";
        let text = format!("let c = {curve}::Line {{ a, b }};\nlet d = Arr{curve}::Line {{ a, b }};\nlet e = Edge{curve}::Line {{ point, dir }};\nlet f = {curve}::of(s, e);\n");
        assert_eq!(curves_named(&text), ["1:9"], "a variant of the curve is counted; another type ending in Curve and a function of the curve are not");
    }

    #[test]
    fn no_kind_of_curve_is_told_apart_outside_the_curve_module() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(|p| p.parent()).expect("repository root").join("crates");
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).expect("sources are readable").flatten() {
                let path = e.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n != "target") {
                        stack.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") || the_curve_module(&path) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                found.extend(kinds_named(&text).into_iter().map(|at| format!("{}:{at}", path.display())));
                if !may_name_curves(&path) {
                    found.extend(curves_named(&text).into_iter().map(|at| format!("{}:{at}", path.display())));
                }
            }
        }
        assert!(
            found.is_empty(),
            "a kind of curve told apart outside model/curve - read it through the parts of its kind (as_line and the like, \
             model/curve/kinds.rs), lay it from them (LineParts {{ a, b }}.into()), or put what it means into the curve module:\n{}",
            found.join("\n")
        );
    }
}
