//! The book and chapter picker's numbers and rules (`picker`), free of the
//! DOM so they are tested natively (`cargo test`): the books as boot hands
//! them over, which books get the compact grid, Random chapter's pick, the
//! keyboard's moves through Random chapter, the books and a book's
//! chapters, and how far the picker scrolls to show something. They are the
//! page's (`ui/index.html` at 13911c4: `pickRandomBookAndChapter`,
//! `showPickerChapters`, `revealInPicker`, `revealCurrentInPicker` and the
//! picker's keydown), rule for rule.

use serde::Deserialize;

/// A book, as the backend lists it (`get_home`'s `otBooks` and `ntBooks`,
/// `commands::BookDto`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: i64,
    pub name: String,
    pub chapter_count: i64,
}

/// A book with more chapters than this (Psalms, Isaiah, Jeremiah) gets the
/// compact grid, ten to a row; the rest get pills, six to a row (see
/// `.chapter-grid` in the page's stylesheet).
pub const COMPACT_OVER: i64 = 50;
pub const COMPACT_COLUMNS: usize = 10;
pub const PILL_COLUMNS: usize = 6;

/// Whether a book of `count` chapters gets the compact grid.
pub fn compact(count: i64) -> bool {
    count > COMPACT_OVER
}

/// How many chapters a row of its grid has.
pub fn columns(count: i64) -> usize {
    if compact(count) {
        COMPACT_COLUMNS
    } else {
        PILL_COLUMNS
    }
}

/// Random chapter: a book chosen uniformly from `books`, then a chapter
/// uniformly from 1 to its chapter count, each from one call of `random`
/// (a number in [0, 1), as `Math.random` gives). Nothing for no books, or
/// for a book whose chapter count isn't positive (rather than ask for a
/// chapter that doesn't exist). The book's id and the chapter.
pub fn random_pick(books: &[Book], mut random: impl FnMut() -> f64) -> Option<(i64, i64)> {
    if books.is_empty() {
        return None;
    }
    let book = books.get(floor_times(random(), books.len()))?;
    if book.chapter_count < 1 {
        return None;
    }
    let count = usize::try_from(book.chapter_count).ok()?;
    let chapter = i64::try_from(floor_times(random(), count)).ok()? + 1;
    Some((book.id, chapter))
}

/// `Math.floor(r * n)` for an `r` in [0, 1): a place among `n`.
fn floor_times(r: f64, n: usize) -> usize {
    // A place among a few hundred at most: exact in an f64, and the cast
    // can't overflow.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let place = (r * n as f64).floor() as usize;
    place
}

/// A key the picker moves with (it lets every other key through).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
}

impl Key {
    /// The key `key` (a `KeyboardEvent.key`) names, if the picker moves
    /// with it.
    pub fn named(key: &str) -> Option<Key> {
        match key {
            "ArrowUp" => Some(Key::Up),
            "ArrowDown" => Some(Key::Down),
            "ArrowLeft" => Some(Key::Left),
            "ArrowRight" => Some(Key::Right),
            "Home" => Some(Key::Home),
            "End" => Some(Key::End),
            _ => None,
        }
    }
}

/// Where a key goes from one of a book's chapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromChapter {
    /// The chapter at this place in the grid.
    Chapter(usize),
    /// The book itself (↑ from the first row).
    Book,
    /// What follows the book: Random chapter or the next book (↓ from the
    /// last row).
    AfterBook,
    Stay,
}

/// From chapter `i` of a grid of `len`, `columns` to a row: ← and → go
/// along, Home and End to the first and last, ↑ and ↓ a row up or down
/// (↑ from the first row is the book; ↓ into a short last row is its last
/// chapter, and from the last row, what follows the book).
pub fn from_chapter(key: Key, i: usize, len: usize, columns: usize) -> FromChapter {
    if len == 0 || columns == 0 {
        return FromChapter::Stay;
    }
    let last = len - 1;
    match key {
        Key::Left if i > 0 => FromChapter::Chapter(i - 1),
        Key::Right if i < last => FromChapter::Chapter(i + 1),
        Key::Left | Key::Right => FromChapter::Stay,
        Key::Home => FromChapter::Chapter(0),
        Key::End => FromChapter::Chapter(last),
        Key::Up if i < columns => FromChapter::Book,
        Key::Up => FromChapter::Chapter(i - columns),
        Key::Down if i + columns < len => FromChapter::Chapter(i + columns),
        Key::Down if i / columns < last / columns => FromChapter::Chapter(last),
        Key::Down => FromChapter::AfterBook,
    }
}

/// Where a key goes from Random chapter or a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromItem {
    /// Random chapter (0) or a book, at this place.
    Item(usize),
    /// The first chapter of the book at this place (its chapters show).
    FirstChapter(usize),
    /// The last chapter of the book at this place (its chapters show).
    LastChapter(usize),
    /// Show this book's chapters, focus staying on it.
    ShowChapters,
    /// Hide this book's chapters, focus staying on it.
    HideChapters,
    Stay,
}

/// From item `i` of `len` (Random chapter, then the books), `open` being
/// the place of the book whose chapters show, if any, and `is_book` saying
/// whether item `i` is a book: ↑ and ↓ go through the items and into and
/// out of the chapters that show, Home and End to the first and last item;
/// on a book, → shows its chapters (or, when they show, goes into them)
/// and ← hides them.
pub fn from_item(key: Key, i: usize, len: usize, open: Option<usize>, is_book: bool) -> FromItem {
    if len == 0 {
        return FromItem::Stay;
    }
    let shows = open == Some(i);
    match key {
        Key::Home => FromItem::Item(0),
        Key::End => FromItem::Item(len - 1),
        Key::Down if shows => FromItem::FirstChapter(i),
        Key::Down if i + 1 < len => FromItem::Item(i + 1),
        Key::Up if i > 0 && open == Some(i - 1) => FromItem::LastChapter(i - 1),
        Key::Up if i > 0 => FromItem::Item(i - 1),
        Key::Up | Key::Down => FromItem::Stay,
        _ if !is_book => FromItem::Stay,
        Key::Right if shows => FromItem::FirstChapter(i),
        Key::Right => FromItem::ShowChapters,
        Key::Left if shows => FromItem::HideChapters,
        Key::Left => FromItem::Stay,
    }
}

/// How the picker scrolls (`scrollTop`) to show something `top` down its
/// contents and `height` tall, when it shows `view` of them from
/// `scroll_top`: just enough, with a little room; nothing when it's in
/// view.
pub fn reveal(top: f64, height: f64, scroll_top: f64, view: f64) -> Option<f64> {
    let bottom = top + height;
    if top < scroll_top {
        Some(top - 8.0)
    } else if bottom > scroll_top + view {
        Some(bottom - view + 8.0)
    } else {
        None
    }
}

/// How the picker scrolls to show the book being read, `book_top` down its
/// contents, and as much of its chapters, ending `bottom` down, as fit:
/// unless they're already in view, its name goes a little below the top.
pub fn reveal_current(book_top: f64, bottom: f64, scroll_top: f64, view: f64) -> Option<f64> {
    let in_view = book_top >= scroll_top && bottom <= scroll_top + view;
    (!in_view).then_some(book_top - 48.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: i64, chapter_count: i64) -> Book {
        Book {
            id,
            name: format!("Book {id}"),
            chapter_count,
        }
    }

    /// A `random` that gives these numbers in turn.
    fn numbers(list: &[f64]) -> impl FnMut() -> f64 + '_ {
        let mut next = list.iter();
        move || *next.next().expect("asked for one more random number")
    }

    #[test]
    fn books_over_fifty_chapters_get_the_compact_grid() {
        assert!(!compact(50));
        assert!(compact(51));
        assert_eq!(columns(150), 10, "Psalms");
        assert_eq!(columns(66), 10, "Isaiah");
        assert_eq!(columns(52), 10, "Jeremiah");
        assert_eq!(columns(50), 6, "Genesis");
        assert_eq!(columns(1), 6, "Obadiah");
    }

    #[test]
    fn random_chapter_is_uniform_over_books_then_chapters() {
        let books = [book(1, 50), book(2, 40), book(66, 22)];
        assert_eq!(random_pick(&books, numbers(&[0.0, 0.0])), Some((1, 1)));
        assert_eq!(
            random_pick(&books, numbers(&[0.999_999, 0.999_999])),
            Some((66, 22)),
            "the last book, its last chapter"
        );
        assert_eq!(random_pick(&books, numbers(&[0.5, 0.5])), Some((2, 21)));
        assert_eq!(random_pick(&books, numbers(&[0.34, 0.0])), Some((2, 1)));
    }

    #[test]
    fn random_chapter_picks_nothing_rather_than_a_chapter_that_isnt_there() {
        assert_eq!(
            random_pick(&[], numbers(&[])),
            None,
            "no books, no number asked"
        );
        assert_eq!(random_pick(&[book(1, 0)], numbers(&[0.2])), None);
        assert_eq!(random_pick(&[book(1, -3)], numbers(&[0.2])), None);
    }

    #[test]
    fn keys_are_the_arrows_home_and_end() {
        assert_eq!(Key::named("ArrowUp"), Some(Key::Up));
        assert_eq!(Key::named("ArrowDown"), Some(Key::Down));
        assert_eq!(Key::named("ArrowLeft"), Some(Key::Left));
        assert_eq!(Key::named("ArrowRight"), Some(Key::Right));
        assert_eq!(Key::named("Home"), Some(Key::Home));
        assert_eq!(Key::named("End"), Some(Key::End));
        for other in ["Enter", " ", "Tab", "PageDown", "g", "Escape"] {
            assert_eq!(Key::named(other), None, "{other}");
        }
    }

    #[test]
    fn the_arrows_move_around_a_grid_of_pills() {
        // John: 21 chapters, six to a row (places 0 to 20; the last row
        // holds 18 to 20).
        let from = |key, i| from_chapter(key, i, 21, 6);
        assert_eq!(from(Key::Right, 2), FromChapter::Chapter(3));
        assert_eq!(from(Key::Down, 3), FromChapter::Chapter(9), "six to a row");
        assert_eq!(from(Key::Down, 9), FromChapter::Chapter(15));
        assert_eq!(
            from(Key::Down, 15),
            FromChapter::Chapter(20),
            "into the short last row"
        );
        assert_eq!(
            from(Key::Down, 20),
            FromChapter::AfterBook,
            "then the next book"
        );
        assert_eq!(from(Key::Down, 18), FromChapter::AfterBook);
        assert_eq!(from(Key::Home, 20), FromChapter::Chapter(0));
        assert_eq!(
            from(Key::Left, 0),
            FromChapter::Stay,
            "nothing before chapter 1"
        );
        assert_eq!(
            from(Key::Right, 20),
            FromChapter::Stay,
            "nothing after the last"
        );
        assert_eq!(from(Key::End, 0), FromChapter::Chapter(20));
        assert_eq!(
            from(Key::Up, 1),
            FromChapter::Book,
            "up from the first row is the book"
        );
        assert_eq!(from(Key::Up, 7), FromChapter::Chapter(1));
    }

    #[test]
    fn the_arrows_move_around_a_compact_grid() {
        // Psalms: 150, ten to a row.
        let from = |key, i| from_chapter(key, i, 150, 10);
        assert_eq!(from(Key::Down, 0), FromChapter::Chapter(10));
        assert_eq!(from(Key::Up, 9), FromChapter::Book);
        assert_eq!(
            from(Key::Down, 145),
            FromChapter::AfterBook,
            "a full last row"
        );
        // Obadiah: one chapter.
        assert_eq!(from_chapter(Key::Down, 0, 1, 6), FromChapter::AfterBook);
        assert_eq!(from_chapter(Key::Up, 0, 1, 6), FromChapter::Book);
        assert_eq!(from_chapter(Key::Right, 0, 1, 6), FromChapter::Stay);
    }

    #[test]
    fn the_arrows_move_through_random_chapter_and_the_books() {
        // Random chapter (0), then three books; the second (2) shows its
        // chapters.
        let from = |key, i, is_book| from_item(key, i, 4, Some(2), is_book);
        assert_eq!(from(Key::Down, 0, false), FromItem::Item(1));
        assert_eq!(from(Key::Down, 1, true), FromItem::Item(2));
        assert_eq!(
            from(Key::Down, 2, true),
            FromItem::FirstChapter(2),
            "into its chapters"
        );
        assert_eq!(
            from(Key::Up, 3, true),
            FromItem::LastChapter(2),
            "back into them"
        );
        assert_eq!(from(Key::Up, 2, true), FromItem::Item(1));
        assert_eq!(from(Key::Up, 0, false), FromItem::Stay);
        assert_eq!(from(Key::Down, 3, true), FromItem::Stay);
        assert_eq!(from(Key::Home, 3, true), FromItem::Item(0));
        assert_eq!(from(Key::End, 0, false), FromItem::Item(3));
        assert_eq!(
            from(Key::Right, 2, true),
            FromItem::FirstChapter(2),
            "→ again goes into them"
        );
        assert_eq!(from(Key::Left, 2, true), FromItem::HideChapters);
        assert_eq!(from(Key::Right, 1, true), FromItem::ShowChapters);
        assert_eq!(from(Key::Left, 1, true), FromItem::Stay);
        assert_eq!(
            from(Key::Right, 0, false),
            FromItem::Stay,
            "Random chapter has none"
        );
        assert_eq!(from(Key::Left, 0, false), FromItem::Stay);
        // No book's chapters showing.
        assert_eq!(from_item(Key::Down, 2, 4, None, true), FromItem::Item(3));
        assert_eq!(from_item(Key::Up, 3, 4, None, true), FromItem::Item(2));
    }

    #[test]
    fn the_picker_scrolls_just_enough_to_show_something() {
        // A 400px view, scrolled 100px down.
        assert_eq!(reveal(150.0, 30.0, 100.0, 400.0), None, "in view");
        assert_eq!(
            reveal(90.0, 30.0, 100.0, 400.0),
            Some(82.0),
            "above: to its top"
        );
        assert_eq!(
            reveal(480.0, 30.0, 100.0, 400.0),
            Some(118.0),
            "below: to its bottom"
        );
        assert_eq!(reveal(0.0, 0.0, 0.0, 0.0), None, "nothing laid out");
    }

    #[test]
    fn the_book_being_read_goes_a_little_below_the_top_unless_in_view() {
        assert_eq!(reveal_current(150.0, 400.0, 100.0, 400.0), None);
        assert_eq!(
            reveal_current(150.0, 600.0, 100.0, 400.0),
            Some(102.0),
            "its chapters don't fit"
        );
        assert_eq!(reveal_current(50.0, 80.0, 100.0, 400.0), Some(2.0), "above");
        assert_eq!(reveal_current(0.0, 0.0, 0.0, 0.0), None, "nothing laid out");
    }
}
