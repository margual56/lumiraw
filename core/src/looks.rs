//! The named looks.

/// A named look.
pub struct Look {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The table's file in the Film Simulation collection.
    pub from: &'static str,
}

/// Every look there is, "none" first.
pub static LOOKS: [Look; 8] = [
    Look { id: "none", label: "None", description: "Your corrections, no look on top", from: "" },
    Look { id: "warm-portrait", label: "Warm portrait",
           description: "Warm, soft contrast, kind to skin",
           from: "Color/Kodak/Kodak Portra 400 2.png" },
    Look { id: "soft-pastel", label: "Soft pastel",
           description: "Airy and light, cool greens",
           from: "Color/Fuji/Fuji 400H 2.png" },
    Look { id: "slide-film", label: "Slide film",
           description: "Richer colour and deeper blacks, still natural",
           from: "Color/Fuji/Fuji Astia 100F.png" },
    Look { id: "everyday-colour", label: "Everyday colour",
           description: "Punchy colour from a consumer print film",
           from: "Color/Fuji/Fuji Superia 400 2.png" },
    Look { id: "faded-vintage", label: "Faded vintage",
           description: "Warm and nostalgic, like an old print",
           from: "Color/Agfa/Agfa Vista 200.png" },
    Look { id: "teal-orange", label: "Teal and orange",
           description: "Cool shadows, warm skin and highlights",
           from: "Color/CreativePack-1/TealOrange.png" },
    Look { id: "black-and-white", label: "Black and white",
           description: "Film black and white, with body in the mids",
           from: "Black-and-White/Ilford/Ilford HP5 Plus 400.png" },
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
        const CLAIMED: [&str; 18] = ["kodak", "fuji", "portra", "velvia", "ektar", "cinestill",
                                     "arri", "alexa", "sony", "canon", "marvel", "spider",
                                     "astia", "superia", "agfa", "vista", "ilford", "hp5"];
        for look in LOOKS.iter() {
            let text = format!("{} {} {}", look.id, look.label, look.description).to_lowercase();
            // `from` is the one place a stock is named, and it is never shown.
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

    /// Every look but "none" says where its table came from, and "none" has
    /// no table: it is the absence of one.
    #[test]
    fn every_look_has_its_table_but_none() {
        for look in LOOKS.iter() {
            assert_eq!(look.from.is_empty(), look.id == "none", "{}", look.id);
        }
    }
}
