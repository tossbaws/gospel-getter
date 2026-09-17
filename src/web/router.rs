use axum::{Router, routing::get};
use std::sync::Arc;

use crate::db::{Book, Translation};
use crate::domain::Store;
use crate::web::handlers::serve_favicon;

/// Application state shared across all handlers.
pub struct AppState {
    pub store: Store,
    /// All 66 books, loaded once at startup. Small and immutable, so it's
    /// kept in memory rather than re-queried on every request — both for
    /// rendering the book list and for computing chapter navigation.
    pub books: Vec<Book>,
    /// All bundled translations, loaded once at startup. `translations[0]`
    /// is the default used when a request doesn't specify one.
    pub translations: Vec<Translation>,
}

/// Build the axum Router.
pub fn build_router(store: Store, books: Vec<Book>, translations: Vec<Translation>) -> Router {
    let state = Arc::new(AppState {
        store,
        books,
        translations,
    });

    Router::new()
        .route("/favicon.ico", get(serve_favicon))
        .route("/", get(crate::web::handlers::home))
        .route(
            "/books/{book_id}/chapters",
            get(crate::web::handlers::chapters_fragment),
        )
        .route(
            "/read/{book_id}/{chapter}",
            get(crate::web::handlers::read_fragment),
        )
        .route(
            "/xref-text/{book_id}/{chapter}/{verse}",
            get(crate::web::handlers::xref_text_fragment),
        )
        .with_state(state)
}
