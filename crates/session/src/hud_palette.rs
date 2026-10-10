//! Colours of the timer HUD (docs/DESIGN_SYSTEM.md): a plum pill, light or
//! dark with the app, with orange for attention. Shared by the Direct2D
//! renderer (`mpo.rs`) and the GDI fallback (`hud.rs`) so the two cannot
//! drift apart. Pure: no OS calls.

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HudColors {
    pub bg: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub dot: Rgb,
}

/// At or under this many seconds the whole pill turns orange.
pub(crate) const WARN_SECS: i64 = 60;

const PLUM_DEEP: Rgb = Rgb(0x1C, 0x12, 0x29);
const PLUM_BORDER: Rgb = Rgb(0x3A, 0x27, 0x52);
const ON_PLUM: Rgb = Rgb(0xF4, 0xEE, 0xFB);
const LILAC: Rgb = Rgb(0xC9, 0xB6, 0xF2);
const WHITE: Rgb = Rgb(0xFF, 0xFF, 0xFF);
const LIGHT_BORDER: Rgb = Rgb(0xE7, 0xE0, 0xF0);
const PLUM_INK: Rgb = Rgb(0x23, 0x16, 0x3A);
const VIOLET: Rgb = Rgb(0x8F, 0x6C, 0xE6);
const ORANGE: Rgb = Rgb(0xF0, 0x8A, 0x3C);

/// The pill's colours for the current state. `is_timer` marks a running
/// "+15 min" extension, shown with an orange dot.
pub(crate) fn hud_colors(is_light: bool, remaining_secs: i64, is_timer: bool) -> HudColors {
    if remaining_secs <= WARN_SECS {
        return HudColors {
            bg: ORANGE,
            border: ORANGE,
            text: PLUM_DEEP,
            dot: PLUM_DEEP,
        };
    }
    if is_light {
        HudColors {
            bg: WHITE,
            border: LIGHT_BORDER,
            text: PLUM_INK,
            dot: if is_timer { ORANGE } else { VIOLET },
        }
    } else {
        HudColors {
            bg: PLUM_DEEP,
            border: PLUM_BORDER,
            text: ON_PLUM,
            dot: if is_timer { ORANGE } else { LILAC },
        }
    }
}

/// Whether `theme.txt` (written by the app: `light`, `dark` or `system`,
/// or an older theme name) means a light HUD. `system` follows Windows'
/// app mode, which the caller reads (`system_light`).
pub(crate) fn theme_is_light(content: &str, system_light: impl FnOnce() -> bool) -> bool {
    match content.trim() {
        "system" | "" => system_light(),
        other => other.contains("light") || other.contains("titanium") || other.contains("frost"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_minute_turns_the_pill_orange_in_both_themes() {
        for light in [false, true] {
            let c = hud_colors(light, 60, false);
            assert_eq!(c.bg, ORANGE);
            assert_eq!(c.text, PLUM_DEEP);
        }
        assert_ne!(hud_colors(false, 61, false).bg, ORANGE);
    }

    #[test]
    fn dark_and_light_pills_use_the_plum_palette() {
        let dark = hud_colors(false, 600, false);
        assert_eq!((dark.bg, dark.text, dark.dot), (PLUM_DEEP, ON_PLUM, LILAC));
        let light = hud_colors(true, 600, false);
        assert_eq!((light.bg, light.text, light.dot), (WHITE, PLUM_INK, VIOLET));
    }

    #[test]
    fn a_running_extension_shows_an_orange_dot() {
        assert_eq!(hud_colors(false, 600, true).dot, ORANGE);
        assert_eq!(hud_colors(true, 600, true).dot, ORANGE);
    }

    #[test]
    fn theme_file_values_map_to_light_or_dark() {
        assert!(theme_is_light("light", || false));
        assert!(!theme_is_light("dark", || true));
        assert!(theme_is_light("system", || true));
        assert!(!theme_is_light("system\n", || false));
        assert!(theme_is_light("", || true));
        // Older names written before the redesign.
        assert!(theme_is_light("clean-titanium", || false));
        assert!(theme_is_light("nordic-frost", || false));
        assert!(!theme_is_light("midnight-cobalt", || true));
    }
}
