//! Aa's rules, free of the DOM so they are tested natively (`cargo test`):
//! which keys change the text size, and stepping through the sizes, what
//! Compare shows beside the text, and what M does to "Notes beside the
//! text".

use crate::preferences::NotesBeside;

/// What Compare shows beside the text being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beside {
    /// Nothing: the text alone.
    Nothing,
    /// The translations, side by side.
    Translations,
}

/// What Compare shows, with Compare on or off (`compare`): it compares
/// translations only. The reader's notes are the margin's
/// (`margin_text::shown`).
pub fn beside(compare: bool) -> Beside {
    if compare {
        Beside::Translations
    } else {
        Beside::Nothing
    }
}

/// What M makes "Notes beside the text", from `now`: it shows or hides the
/// notes right where the reader is. Outside reading mode, Always goes back
/// to In reading mode and anything else becomes Always; in reading mode,
/// Off becomes In reading mode and anything else Off. So each press flips
/// whether the notes show there.
pub fn notes_toggled(now: NotesBeside, reading_mode: bool) -> NotesBeside {
    match (reading_mode, now) {
        (false, NotesBeside::Always) => NotesBeside::Reading,
        (false, _) => NotesBeside::Always,
        (true, NotesBeside::Off) => NotesBeside::Reading,
        (true, _) => NotesBeside::Off,
    }
}

/// A key press, as far as the text size keys care.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Press<'a> {
    pub key: &'a str,
    /// `KeyboardEvent.code` (the numpad's keys), or empty.
    pub code: &'a str,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What a key does to the text size: a step larger (`1`) or smaller
/// (`-1`), or back to Medium (`0`); `None` if it isn't one of the size
/// keys. Ctrl (or Cmd) with `=` or `+` (Shift and the numpad's too), with
/// `-` or `_` (the numpad's too), or with `0` (not Shift+0, which is `)`
/// on some layouts; the numpad's 0 either way). Never with Alt.
pub fn size_key(press: &Press) -> Option<i8> {
    if !(press.ctrl || press.meta) || press.alt {
        return None;
    }
    let (key, code) = (press.key, press.code);
    if key == "=" || key == "+" || code == "NumpadAdd" {
        Some(1)
    } else if key == "-" || key == "_" || code == "NumpadSubtract" {
        Some(-1)
    } else if (key == "0" && !press.shift) || code == "Numpad0" {
        Some(0)
    } else {
        None
    }
}

/// The size `by` steps from the one at `index` among `count` sizes, if
/// there is one (the ends don't wrap).
pub fn stepped(index: i32, count: u32, by: i32) -> Option<u32> {
    let next = index.checked_add(by)?;
    u32::try_from(next).ok().filter(|&n| n < count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(key: &str) -> Press<'_> {
        Press {
            key,
            ctrl: true,
            ..Press::default()
        }
    }

    #[test]
    fn the_size_keys() {
        assert_eq!(size_key(&ctrl("=")), Some(1));
        assert_eq!(size_key(&ctrl("+")), Some(1));
        assert_eq!(
            size_key(&Press {
                shift: true,
                ..ctrl("+")
            }),
            Some(1)
        );
        assert_eq!(size_key(&ctrl("-")), Some(-1));
        assert_eq!(size_key(&ctrl("_")), Some(-1));
        assert_eq!(size_key(&ctrl("0")), Some(0));
        // Cmd on a Mac.
        assert_eq!(
            size_key(&Press {
                key: "=",
                meta: true,
                ..Press::default()
            }),
            Some(1)
        );
        // The numpad, by its code, whatever the key says.
        for (code, step) in [("NumpadAdd", 1), ("NumpadSubtract", -1), ("Numpad0", 0)] {
            assert_eq!(
                size_key(&Press {
                    code,
                    ..ctrl("Unidentified")
                }),
                Some(step),
                "{code}"
            );
        }
        assert_eq!(
            size_key(&Press {
                code: "Numpad0",
                shift: true,
                ..ctrl("Insert")
            }),
            Some(0)
        );
    }

    #[test]
    fn not_the_size_keys() {
        // Without Ctrl or Cmd, or with Alt.
        assert_eq!(
            size_key(&Press {
                key: "=",
                ..Press::default()
            }),
            None
        );
        assert_eq!(
            size_key(&Press {
                alt: true,
                ..ctrl("-")
            }),
            None
        );
        // Shift+0 isn't 0.
        assert_eq!(
            size_key(&Press {
                shift: true,
                ..ctrl("0")
            }),
            None
        );
        assert_eq!(size_key(&ctrl("b")), None);
        assert_eq!(size_key(&ctrl("9")), None);
    }

    #[test]
    fn stepping_stops_at_the_ends() {
        assert_eq!(stepped(1, 4, 1), Some(2));
        assert_eq!(stepped(3, 4, 1), None);
        assert_eq!(stepped(0, 4, -1), None);
        assert_eq!(stepped(2, 4, -1), Some(1));
        // Nothing chosen (`selectedIndex` -1): a step up is the first.
        assert_eq!(stepped(-1, 4, 1), Some(0));
        assert_eq!(stepped(-1, 0, 1), None);
    }

    #[test]
    fn compare_compares_translations_only() {
        assert_eq!(beside(false), Beside::Nothing);
        assert_eq!(beside(true), Beside::Translations);
    }

    /// M2: M shows or hides the notes where the reader is, in or out of
    /// reading mode, from each of the three settings.
    #[test]
    fn m_shows_or_hides_the_notes_where_the_reader_is() {
        use NotesBeside::{Always, Off, Reading};
        for (now, reading_mode, then) in [
            (Off, false, Always),
            (Reading, false, Always),
            (Always, false, Reading),
            (Off, true, Reading),
            (Reading, true, Off),
            (Always, true, Off),
        ] {
            assert_eq!(
                notes_toggled(now, reading_mode),
                then,
                "{now:?}, {reading_mode}"
            );
        }
        // Each press flips whether they show where the reader is.
        let shows = |s: NotesBeside, reading_mode: bool| match s {
            Off => false,
            Reading => reading_mode,
            Always => true,
        };
        for now in [Off, Reading, Always] {
            for reading_mode in [false, true] {
                let then = notes_toggled(now, reading_mode);
                assert_ne!(shows(now, reading_mode), shows(then, reading_mode));
            }
        }
    }
}
