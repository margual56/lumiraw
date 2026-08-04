//! The named grades, as control points.

use crate::curve::{Curve, Stack};

/// A named grade.
pub struct Look {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The composite curve, on all three channels at once: contrast.
    pub rgb: &'static [(f32, f32)],
    pub r: &'static [(f32, f32)],
    pub g: &'static [(f32, f32)],
    pub b: &'static [(f32, f32)],
}

impl Look {
    /// This look as a stack the pipeline can apply, faded by `strength`.
    pub fn stack(&self, strength: f32) -> Stack {
        Stack::new(Curve::new(self.rgb), Curve::new(self.r), Curve::new(self.g),
                   Curve::new(self.b), strength)
    }
}

/// Every look there is.
pub static LOOKS: [Look; 5] = [
    Look {
        id: "none",
        label: "None",
        description: "Your corrections, no grade on top",
        rgb: &[], r: &[], g: &[], b: &[],
    },
    Look {
        id: "comic",
        label: "Comic book",
        // Print-inspired: the ink-on-paper look, where the blacks are solid,
        // the highlights stop dead rather than rolling off, and the midtones
        // carry all the colour.
        description: "Solid cool blacks, hard highlights, colour held in the mids",
        rgb: &[(0.0, 0.0), (0.16, 0.08), (0.5, 0.5), (0.84, 0.94), (1.0, 1.0)],
        r: &[(0.0, 0.0), (0.5, 0.53), (1.0, 1.0)],
        g: &[],
        b: &[(0.0, 0.05), (0.3, 0.34), (0.75, 0.75), (1.0, 1.0)],
    },
    Look {
        id: "studio",
        label: "Studio portrait",
        // A lifted toe and a rolled shoulder is what a softbox does to a face.
        description: "Lifted blacks, rolled highlights, gentle through the mids",
        rgb: &[(0.0, 0.05), (0.25, 0.27), (0.75, 0.79), (1.0, 0.97)],
        r: &[(0.0, 0.0), (0.6, 0.63), (1.0, 1.0)],
        g: &[],
        b: &[(0.0, 0.02), (0.5, 0.485), (1.0, 0.98)],
    },
    Look {
        id: "teal_orange",
        label: "Teal and orange",
        // The one everybody recognises, and the clearest demonstration of what
        // four curves are for.
        description: "Cool shadows, warm highlights, mild contrast",
        rgb: &[(0.0, 0.0), (0.25, 0.22), (0.75, 0.78), (1.0, 1.0)],
        r: &[(0.0, 0.0), (0.35, 0.33), (0.75, 0.80), (1.0, 1.0)],
        g: &[],
        b: &[(0.0, 0.035), (0.25, 0.33), (0.7, 0.70), (1.0, 0.985)],
    },
    Look {
        id: "faded",
        label: "Faded",
        // Both ends pulled in and the slope kept under one throughout, which is
        // the only way a curve can take colour out.
        description: "Both ends pulled in, colour quietly drained",
        rgb: &[(0.0, 0.075), (0.3, 0.33), (0.7, 0.71), (1.0, 0.93)],
        r: &[(0.0, 0.01), (0.5, 0.51), (1.0, 0.985)],
        g: &[],
        b: &[(0.0, 0.02), (0.5, 0.49), (1.0, 0.96)],
    },
];

/// The look of that name, or nothing if there is no such look.
pub fn find(id: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|l| l.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nobody may add a look whose name claims somebody else's work.
    #[test]
    fn no_look_is_named_after_something_owned() {
        const CLAIMED: [&str; 12] = ["kodak", "fuji", "portra", "velvia", "ektar", "cinestill",
                                     "arri", "alexa", "sony", "canon", "marvel", "spider"];
        for look in LOOKS.iter() {
            let text = format!("{} {} {}", look.id, look.label, look.description).to_lowercase();
            for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
                assert!(!CLAIMED.contains(&word), "{} names {word}", look.id);
            }
        }
    }

    /// Every look has to be reachable by its id and no two may share one, or
    /// `find` silently returns the wrong grade.
    #[test]
    fn every_look_is_reachable_and_distinct() {
        for look in LOOKS.iter() {
            assert!(std::ptr::eq(find(look.id).expect(look.id), look));
        }
        for (i, a) in LOOKS.iter().enumerate() {
            for b in LOOKS.iter().skip(i + 1) {
                assert_ne!(a.id, b.id, "two looks share the id {}", a.id);
                assert_ne!(a.label, b.label, "two looks share the label {}", a.label);
            }
        }
        assert!(find("no such look").is_none());
    }

    /// `none` has to be nothing at all, not nearly nothing: it is what somebody
    /// picks to see their own corrections.
    #[test]
    fn none_is_the_identity() {
        assert!(find("none").unwrap().stack(1.0).is_identity());
        // And every other look has to actually do something, or it is a row in
        // a list that wastes a click.
        for look in LOOKS.iter().filter(|l| l.id != "none") {
            assert!(!look.stack(1.0).is_identity(), "{} does nothing", look.id);
        }
    }

    /// The ends are where a grade does damage.
    #[test]
    fn no_look_destroys_an_end() {
        for look in LOOKS.iter().filter(|l| l.id != "none") {
            let s = look.stack(1.0);
            for (c, name) in [(0, "red"), (1, "green"), (2, "blue")] {
                let curve = s.channel(c);
                let composite = s.composite();
                // The stack applies the composite first, so the end a pixel
                // actually reaches is the pair composed.
                let black = curve.at(composite.at(0.0));
                let white = curve.at(composite.at(1.0));
                assert!(white <= 1.0 + 1e-6, "{} clips {name} white to {white}", look.id);
                assert!(black < 0.35, "{} lifts {name} black to {black}", look.id);
            }
        }
    }

    /// The claim in each description has to be the shape on the curve, checked
    /// where the two could drift apart.
    #[test]
    fn the_descriptions_are_the_shapes() {
        let cool_shadows = |id: &str| {
            let s = find(id).unwrap().stack(1.0);
            let low = s.channel(2).at(0.2) - s.channel(0).at(0.2);
            let high = s.channel(2).at(0.85) - s.channel(0).at(0.85);
            assert!(low > 0.02, "{id} does not cool its shadows: {low}");
            assert!(low > high, "{id} cools the highlights as much as the shadows");
        };
        cool_shadows("comic");
        cool_shadows("teal_orange");

        // Warm highlights: red above the diagonal up top, for teal and orange.
        let s = find("teal_orange").unwrap().stack(1.0);
        assert!(s.channel(0).at(0.8) > 0.82, "the highlights are not warm");

        // Lifted blacks and a rolled shoulder, for studio and faded.
        for id in ["studio", "faded"] {
            let s = find(id).unwrap().stack(1.0);
            assert!(s.composite().at(0.0) > 0.03, "{id} does not lift its blacks");
            assert!(s.composite().at(1.0) < 0.99, "{id} does not roll its highlights");
        }

        // Faded takes colour out, which for a curve means a slope below one
        // through the middle. Measured across the mids rather than at a point.
        let faded = find("faded").unwrap().stack(1.0);
        let slope = (faded.composite().at(0.7) - faded.composite().at(0.3)) / 0.4;
        assert!(slope < 0.98, "faded does not flatten, so it cannot fade: slope {slope}");
    }
}
