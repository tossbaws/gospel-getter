//! Aa's rules, free of the DOM so they are tested natively (`cargo test`):
//! which keys change the text size, and stepping through the sizes. They
//! are the page's (`ui/index.html` at 4b2d15e: `sizeKey` and
//! `stepTextSize`), rule for rule.

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
}
