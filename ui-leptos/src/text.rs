//! The dialog's words and numbers, and the filter it builds from its
//! fields: the pure part of what `ui/index.html`'s export-notes script did,
//! kept free of the DOM so it is tested natively (`cargo test`).

use crate::dto::{DatePresets, Filter, NotedBook};

/// `n.toLocaleString('en')`: digits grouped in threes with commas.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1 note", "3 notes", "1,234 notes".
pub fn plural(n: u64, one: &str) -> String {
    let word = if n == 1 {
        one.to_string()
    } else {
        format!("{one}s")
    };
    format!("{} {word}", thousands(n))
}

/// The match line for a count: "No notes match", "1 note matches",
/// "3 notes match".
pub fn count_line(count: u64) -> String {
    match count {
        0 => "No notes match".to_string(),
        1 => "1 note matches".to_string(),
        n => format!("{} notes match", thousands(n)),
    }
}

/// A chapter field's number, held to the book's chapters; an empty or
/// unreadable one is `fallback` (the book's first or last chapter with
/// notes). As the page did: `Math.round(Number(value))`, clamped to
/// `1..=chapter_count`.
pub fn chapter_value(raw: &str, chapter_count: i64, fallback: i64) -> i64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback;
    }
    let Ok(n) = trimmed.parse::<f64>() else {
        return fallback;
    };
    if !n.is_finite() {
        return fallback;
    }
    // The clamp keeps the cast in range (a chapter count is small).
    (n.round() as i64).clamp(1, chapter_count.max(1))
}

/// The dialog's fields, as the reader left them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fields {
    pub chapter_from: String,
    pub chapter_to: String,
    /// The Written `<select>`'s value: `all`, a preset, or `custom`.
    pub when: String,
    pub date_from: String,
    pub date_to: String,
}

/// The filter the backend reads for the ticked books and the fields: every
/// book is no book list, and a chapter range around all of a book's notes
/// is none. `ticked` is in Bible order, as the checklist is; `total` is how
/// many books the checklist offers.
pub fn build_filter(
    ticked: &[&NotedBook],
    total: usize,
    fields: &Fields,
    dates: &DatePresets,
) -> Filter {
    let mut chapters = None;
    if let [book] = ticked {
        let range = (
            chapter_value(&fields.chapter_from, book.chapter_count, book.first_chapter),
            chapter_value(&fields.chapter_to, book.chapter_count, book.last_chapter),
        );
        if range != (book.first_chapter, book.last_chapter) {
            chapters = Some(range);
        }
    }
    let every = ticked.len() == total && chapters.is_none();
    let (written_from, written_to) = match fields.when.as_str() {
        "custom" => (
            Some(fields.date_from.clone()).filter(|d| !d.is_empty()),
            Some(fields.date_to.clone()).filter(|d| !d.is_empty()),
        ),
        "all" => (None, None),
        preset => match dates.preset(preset) {
            Some(span) => (Some(span.from.clone()), Some(span.to.clone())),
            None => (None, None),
        },
    };
    Filter {
        books: if every {
            Vec::new()
        } else {
            ticked.iter().map(|b| b.id).collect()
        },
        chapters,
        written_from,
        written_to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::DaySpan;

    fn book(id: i64, first: i64, last: i64, count: i64) -> NotedBook {
        NotedBook {
            id,
            name: format!("Book {id}"),
            notes: 2,
            first_chapter: first,
            last_chapter: last,
            noted_chapters: 2,
            chapter_count: count,
        }
    }

    fn dates() -> DatePresets {
        let span = |from: &str| DaySpan {
            from: from.to_string(),
            to: "2026-10-12".to_string(),
        };
        DatePresets {
            today: "2026-10-12".to_string(),
            last_7_days: span("2026-10-06"),
            last_30_days: span("2026-09-13"),
            this_year: span("2026-01-01"),
        }
    }

    #[test]
    fn numbers_read_as_the_page_showed_them() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1234), "1,234");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(plural(1, "note"), "1 note");
        assert_eq!(plural(3, "note"), "3 notes");
        assert_eq!(plural(1234, "note"), "1,234 notes");
        assert_eq!(count_line(0), "No notes match");
        assert_eq!(count_line(1), "1 note matches");
        assert_eq!(count_line(5), "5 notes match");
    }

    #[test]
    fn chapter_fields_are_held_to_the_book() {
        assert_eq!(chapter_value("3", 21, 1), 3);
        assert_eq!(chapter_value(" 3 ", 21, 1), 3);
        assert_eq!(chapter_value("", 21, 1), 1);
        assert_eq!(chapter_value("   ", 21, 3), 3);
        assert_eq!(chapter_value("abc", 21, 3), 3);
        assert_eq!(chapter_value("0", 21, 1), 1);
        assert_eq!(chapter_value("99", 21, 3), 21);
        assert_eq!(chapter_value("2.6", 21, 1), 3);
        assert_eq!(chapter_value("inf", 21, 4), 4);
        assert_eq!(chapter_value("NaN", 21, 4), 4);
    }

    #[test]
    fn every_book_is_no_book_list() {
        let john = book(43, 1, 3, 21);
        let psalms = book(19, 23, 23, 150);
        let fields = Fields {
            when: "all".to_string(),
            ..Fields::default()
        };
        let every = build_filter(&[&psalms, &john], 2, &fields, &dates());
        assert_eq!(every.books, Vec::<i64>::new());
        assert_eq!(every.chapters, None);
        let one = build_filter(&[&john], 2, &fields, &dates());
        assert_eq!(one.books, vec![43]);
        let none = build_filter(&[], 2, &fields, &dates());
        assert_eq!(none.books, Vec::<i64>::new());
    }

    #[test]
    fn a_range_around_all_of_a_books_notes_is_none() {
        let john = book(43, 1, 3, 21);
        let whole = Fields {
            chapter_from: "1".to_string(),
            chapter_to: "3".to_string(),
            when: "all".to_string(),
            ..Fields::default()
        };
        assert_eq!(build_filter(&[&john], 2, &whole, &dates()).chapters, None);
        let part = Fields {
            chapter_from: "3".to_string(),
            chapter_to: "".to_string(),
            when: "all".to_string(),
            ..Fields::default()
        };
        assert_eq!(
            build_filter(&[&john], 2, &part, &dates()).chapters,
            Some((3, 3))
        );
        // A range makes the one book a book list, even when it is the only
        // book there is.
        assert_eq!(build_filter(&[&john], 1, &part, &dates()).books, vec![43]);
        // Two books: the fields are ignored.
        let psalms = book(19, 23, 23, 150);
        assert_eq!(
            build_filter(&[&psalms, &john], 3, &part, &dates()).chapters,
            None
        );
    }

    #[test]
    fn written_presets_and_custom_dates() {
        let john = book(43, 1, 3, 21);
        let mut fields = Fields {
            when: "last7Days".to_string(),
            ..Fields::default()
        };
        let f = build_filter(&[&john], 2, &fields, &dates());
        assert_eq!(f.written_from.as_deref(), Some("2026-10-06"));
        assert_eq!(f.written_to.as_deref(), Some("2026-10-12"));
        fields.when = "custom".to_string();
        fields.date_from = "2026-01-01".to_string();
        let f = build_filter(&[&john], 2, &fields, &dates());
        assert_eq!(f.written_from.as_deref(), Some("2026-01-01"));
        assert_eq!(f.written_to, None);
        fields.when = "nonsense".to_string();
        let f = build_filter(&[&john], 2, &fields, &dates());
        assert_eq!((f.written_from, f.written_to), (None, None));
    }
}
