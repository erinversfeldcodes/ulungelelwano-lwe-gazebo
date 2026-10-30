use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tree node with no content — what the sidebar renders. `parent_id` is
/// `None` for a root-level page (the tree can have more than one root).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTreeNode {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub slug: String,
    pub title: String,
    pub position: i32,
}

/// A page's current content — `content_json` is the canonical `BlockNote`
/// document (what the editor loads), `content_markdown` is a derived export
/// (what a read-only, signed-out view renders, and what search/diffing use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContent {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub slug: String,
    pub title: String,
    pub content_json: String,
    pub content_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSearchResult {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    /// Already has the matched terms wrapped in `<b>...</b>` (from
    /// Postgres's `ts_headline`) — render with `inner_html`, not as plain
    /// text, or the highlighting is just literal angle brackets on screen.
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRevisionSummary {
    pub id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    /// RFC3339, formatted server-side — `OffsetDateTime` itself has no
    /// precedent for crossing the wire anywhere else in this codebase, so
    /// this keeps the client-facing type unambiguous rather than leaning
    /// on `time`'s serde impl doing the right thing by default.
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionDiff {
    pub current_title: String,
    pub current_markdown: String,
    /// `None` when the diffed revision is the page's first — there's
    /// nothing before it to compare against.
    pub previous_markdown: Option<String>,
}

#[cfg(feature = "ssr")]
async fn require_user() -> Result<crate::auth::UserRow, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let cookies = leptos_axum::extract::<tower_cookies::Cookies>()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    crate::auth::require_user(&state.db_pool, &cookies)
        .await
        .map_err(ServerFnError::new)
}

/// Lowercase letters/digits only, runs separated by a single hyphen — a
/// cosmetic URL suffix, not the lookup key (routing is always by page id),
/// so this never needs to be unique: two siblings can end up with the same
/// slug and nothing breaks, it's just a less distinctive link to look at.
#[cfg(feature = "ssr")]
fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut last_was_hyphen = true; // avoids a leading hyphen
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("page");
    }
    slug
}

// Browsing the tree and reading a page is public, same as cards/decks —
// only create/edit/move/delete require a signed-in user.

#[server]
pub async fn list_page_tree() -> Result<Vec<PageTreeNode>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::list_page_tree(&state.db_pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| PageTreeNode {
            id: r.id,
            parent_id: r.parent_id,
            slug: r.slug,
            title: r.title,
            position: r.position,
        })
        .collect())
}

#[server]
pub async fn search_pages(query: String) -> Result<Vec<PageSearchResult>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::search_pages(&state.db_pool, &query)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| PageSearchResult {
            id: r.id,
            slug: r.slug,
            title: r.title,
            snippet: r.snippet,
        })
        .collect())
}

#[server]
pub async fn get_page(page_id: Uuid) -> Result<Option<PageContent>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let row = crate::db::get_page(&state.db_pool, page_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(row.map(|r| PageContent {
        id: r.id,
        parent_id: r.parent_id,
        slug: r.slug,
        title: r.title,
        content_json: r.content_json,
        content_markdown: r.content_markdown,
    }))
}

#[server]
pub async fn create_page(parent_id: Option<Uuid>, title: String) -> Result<Uuid, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let slug = slugify(&title);
    crate::db::create_page(&state.db_pool, user.id, parent_id, &title, &slug)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn save_page_revision(
    page_id: Uuid,
    title: String,
    content_json: String,
    content_markdown: String,
) -> Result<Uuid, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::save_page_revision(
        &state.db_pool,
        page_id,
        user.id,
        &title,
        &content_json,
        &content_markdown,
    )
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn rename_page(page_id: Uuid, title: String) -> Result<(), ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let slug = slugify(&title);
    crate::db::rename_page(&state.db_pool, page_id, &title, &slug)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn move_page(
    page_id: Uuid,
    new_parent_id: Option<Uuid>,
    new_position: i32,
) -> Result<(), ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::move_page(&state.db_pool, page_id, new_parent_id, new_position)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn delete_page(page_id: Uuid) -> Result<(), ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let has_children = crate::db::page_has_children(&state.db_pool, page_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if has_children {
        return Err(ServerFnError::new(
            "move or delete this page's children first",
        ));
    }
    crate::db::delete_page(&state.db_pool, page_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn list_page_revisions(page_id: Uuid) -> Result<Vec<PageRevisionSummary>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::list_page_revisions(&state.db_pool, page_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    rows.into_iter()
        .map(|r| {
            let created_at = r
                .created_at
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(PageRevisionSummary {
                id: r.id,
                author_id: r.author_id,
                title: r.title,
                created_at,
            })
        })
        .collect()
}

/// Diffs `revision_id` against the revision immediately before it (a
/// GitHub-commit-style "what changed here", not an arbitrary A/B compare —
/// see the wiki planning notes for why that's the deliberate starting scope).
#[server]
pub async fn get_revision_diff(
    page_id: Uuid,
    revision_id: Uuid,
) -> Result<RevisionDiff, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let revision = crate::db::get_revision(&state.db_pool, revision_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("revision not found"))?;
    let previous_markdown =
        crate::db::previous_revision_markdown(&state.db_pool, page_id, revision_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(RevisionDiff {
        current_title: revision.title,
        current_markdown: revision.content_markdown,
        previous_markdown,
    })
}

/// Never rewrites history — restoring an old revision creates a *new* one
/// with that old content, same append-only shape as everything else here.
#[server]
pub async fn restore_revision(page_id: Uuid, revision_id: Uuid) -> Result<Uuid, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let revision = crate::db::get_revision(&state.db_pool, revision_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("revision not found"))?;
    crate::db::save_page_revision(
        &state.db_pool,
        page_id,
        user.id,
        &revision.title,
        &revision.content_json,
        &revision.content_markdown,
    )
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))
}

/// Server-renders `content_markdown` to HTML for a read-only view — no
/// editor JS shipped to someone who's just reading. `default-features =
/// false` on the comrak dependency drops its CLI binary and syntax
/// highlighting (syntect/onig, a C library) in favor of just the GFM
/// parser; add a highlighting feature back explicitly if code blocks in
/// articles ever need it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn render_markdown(markdown: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    comrak::markdown_to_html(markdown, &options)
}
