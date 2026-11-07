//! The MCP server's resources, `gg://<translation>/<book>/<chapter>`: each
//! chapter of each translation, read-only, its text exactly what
//! `get_passage` gives for that chapter. And the one thing its prompt needs
//! read: a book's name.

use anyhow::{anyhow, bail};
use std::path::Path;

use super::{Answer, GetPassageArgs, Library};

/// What every resource's address starts with.
pub const RESOURCE_SCHEME: &str = "gg://";

/// The resources' address, as a template a client fills in.
pub const RESOURCE_TEMPLATE: &str = "gg://{translation}/{book}/{chapter}";

/// One chapter of one translation, as a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterResource {
    /// `gg://kjv/john/3`.
    pub uri: String,
    /// "John 3 (KJV)".
    pub name: String,
    /// What it is, in a sentence.
    pub description: String,
}

/// A book's name as a resource's address has it: lowercase, its spaces as
/// hyphens ("1-john", "song-of-solomon").
pub fn book_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Every chapter of every translation in the install at `database`, in the
/// order the app lists the translations, then in Bible order.
pub async fn chapter_resources(database: &Path) -> anyhow::Result<Vec<ChapterResource>> {
    let library = Library::open(database).await?;
    let resources = library.chapter_resources();
    library.close().await;
    Ok(resources)
}

/// The JSON the resource at `uri` holds in the install at `database`, the same
/// bytes `get_passage`'s tool answers with for its chapter.
pub async fn read_resource(database: &Path, uri: &str) -> anyhow::Result<String> {
    let library = Library::open(database).await?;
    let answer = library.read_resource(uri).await;
    library.close().await;
    super::to_json(&answer?)
}

/// The name of the book `name` names in the install at `database`, as the
/// app has it ("Phil" is Philippians), or why there's none.
pub async fn book_name(database: &Path, name: &str) -> anyhow::Result<String> {
    let library = Library::open(database).await?;
    let found = library.book_name(name);
    library.close().await;
    found
}

impl Library {
    fn chapter_resources(&self) -> Vec<ChapterResource> {
        self.translations
            .iter()
            .flat_map(|t| {
                self.books.iter().flat_map(move |b| {
                    (1..=b.chapter_count).map(move |chapter| ChapterResource {
                        uri: format!(
                            "{RESOURCE_SCHEME}{}/{}/{chapter}",
                            t.code,
                            book_slug(&b.name)
                        ),
                        name: format!("{} {chapter} ({})", b.name, t.code.to_uppercase()),
                        description: format!(
                            "The exact text of {} {chapter} in the {}, as get_passage gives it.",
                            b.name, t.name
                        ),
                    })
                })
            })
            .collect()
    }

    /// What the resource at `uri` holds: `get_passage`'s answer for its
    /// chapter in its translation, or why there's no such resource.
    pub(super) async fn read_resource(&self, uri: &str) -> anyhow::Result<Answer> {
        let not_one = || {
            anyhow!(
                "\u{201c}{uri}\u{201d} isn't a Gospel Getter resource. They look like \
                 gg://kjv/john/3: a translation, a book and a chapter."
            )
        };
        let path = uri.strip_prefix(RESOURCE_SCHEME).ok_or_else(not_one)?;
        let [code, book, chapter] = path.split('/').collect::<Vec<_>>()[..] else {
            return Err(not_one());
        };
        let translation = self.translation_named(code)?;
        let book = self.book_named(&book.replace('-', " "))?;
        let chapter: i64 = chapter.parse().map_err(|_| not_one())?;
        if !(1..=book.chapter_count).contains(&chapter) {
            bail!(
                "{} has {} chapters; there's no chapter {chapter}.",
                book.name,
                book.chapter_count
            );
        }
        self.get_passage(GetPassageArgs {
            reference: format!("{} {chapter}", book.name),
            translation: Some(translation.code.clone()),
        })
        .await
    }

    /// The name of the book `name` names, as the app has it, or why there's
    /// none.
    pub fn book_name(&self, name: &str) -> anyhow::Result<String> {
        Ok(self.book_named(name.trim())?.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_books_address_is_its_name_in_lowercase_with_hyphens() {
        assert_eq!(book_slug("John"), "john");
        assert_eq!(book_slug("1 John"), "1-john");
        assert_eq!(book_slug("Song of Solomon"), "song-of-solomon");
    }
}
