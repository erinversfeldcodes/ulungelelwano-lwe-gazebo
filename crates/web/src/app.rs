// #[component] already enforces call sites use the returned view; clippy's
// pedantic must_use_candidate doesn't see through the macro expansion.
#![allow(clippy::must_use_candidate)]

use crate::account::{
    ChangePassword, ConfirmPasswordReset, LogIn, RequestPasswordReset, SetDailyNewCardLimit,
    SignUp, UpdateEmail, UpdateName, current_profile, current_user_name, user_stats,
};
use crate::cards::{
    DeleteCard, DirectionInput, GradeCard, GradeInput, ImageSearchResult, ImportCardImage,
    ReviewOrderInput, SearchCardImages, get_card, list_tags, review_queue,
};
use crate::decks::{
    AddCardToDeck, CreateDeck, DeleteDeck, RemoveCardFromDeck, RenameDeck, list_decks,
};
use crate::dictionary::get_word_definition;
use crate::dictionary_ui::{ClickableWord, DictionaryDetailsEditor};
use crate::media_recorder::RecordButton;
use leptos::html;
use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{A, Redirect, Route, Router, Routes},
    hooks::{use_navigate, use_params_map, use_query_map},
    path,
};
use uuid::Uuid;

/// `card.youtube_url` is always already a normalized
/// `youtube.com/embed/{id}` URL by the time it reaches here (validated
/// and canonicalized server-side in main.rs, never the raw link a user
/// pasted), so this is just the embed markup — no parsing needed. Shared
/// across the card list, card detail, and review pages rather than
/// repeated, since it's the exact same three attributes everywhere.
fn youtube_embed(url: String) -> impl IntoView {
    view! { <iframe src=url width="560" height="315" allowfullscreen=true></iframe> }
}

/// Renders a `?error=` query param — left by main.rs's raw multipart POST
/// handlers (`cards_new`/`cards_bulk`/`cards_update`) on a failed
/// submission, see `error_redirect` there — with the same `"Error: "`
/// paragraph every `ActionForm`-based error in this file already uses. A
/// plain, one-time read rather than a reactive signal: these pages are
/// only ever reached with `?error=` set via a fresh full-page redirect
/// (never a client-side route change), so there's nothing for it to react
/// to after mount.
fn form_error_banner() -> impl IntoView {
    let query = use_query_map();
    query
        .get_untracked()
        .get("error")
        .map(|message| view! { <p>"Error: " {message}</p> })
}

/// A submit button for a destructive `ActionForm` that requires two clicks:
/// the first arms it (relabeling to "Confirm delete?" for a few seconds)
/// without submitting, the second lets the click proceed as a normal submit.
/// Avoids pulling in a modal/dialog for a single low-frequency action that's
/// already behind its own form; `armed` must be a signal local to this one
/// button (a fresh `RwSignal::new(false)` per call site, or per loop
/// iteration for a repeated button like one delete-per-row).
fn confirm_button(label: &'static str, armed: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            type="submit"
            class:armed=armed
            on:click=move |ev| {
                if !armed.get_untracked() {
                    ev.prevent_default();
                    armed.set(true);
                    set_timeout(move || armed.set(false), std::time::Duration::from_secs(4));
                }
            }
        >
            {move || if armed.get() { "Confirm delete?" } else { label }}
        </button>
    }
}

/// Search-and-pick an image for a card's `image_url`, as an alternative to
/// uploading a file directly. The English text here is deliberately never
/// part of the submitted card form — it only ever drives
/// `search_card_images`, never gets stored or shown on the card itself
/// (unlike target_text, which is the target-language word/phrase).
/// Picking a thumbnail re-fetches and re-hosts the full image server-side
/// (see `cards::import_card_image`) rather than linking the provider's own
/// URL, so a card's image is always something this app hosts, same as a
/// direct upload. The caller renders the resulting `chosen_image_url` as a
/// hidden form field — see `NewCardPage`/`EditCardPage`.
#[component]
fn ImageSearchPicker(
    chosen_image_url: RwSignal<Option<String>>,
    /// Pre-fills the search field — used by `BulkAddPage`, where the
    /// English word was already given as part of the pasted list, so the
    /// user shouldn't have to retype it per card just to search.
    #[prop(default = String::new())]
    initial_query: String,
) -> impl IntoView {
    let english_text = RwSignal::new(initial_query.clone());
    let search = ServerAction::<SearchCardImages>::new();
    let import = ServerAction::<ImportCardImage>::new();

    Effect::new(move |_| {
        if let Some(Ok(url)) = import.value().get() {
            chosen_image_url.set(Some(url));
        }
    });

    view! {
        <div class="image-search">
            <label>
                "English translation (used only to search for an image — never stored on the card)"
                <input
                    type="text"
                    value=initial_query
                    on:input=move |ev| english_text.set(event_target_value(&ev))
                />
            </label>
            <button
                type="button"
                on:click=move |_| {
                    search.dispatch(SearchCardImages { query: english_text.get_untracked() });
                }
            >
                "Search images"
            </button>
            {move || search.pending().get().then(|| view! { <p>"Searching…"</p> })}
            {move || {
                // Distinguishes "haven't searched yet" (None, show nothing)
                // from "searched and every provider came back empty" (an
                // Ok with no results) — without this, a search that
                // genuinely found nothing looks identical to never having
                // searched at all, with no signal that redoing it with
                // different text might help.
                search.value().get().and_then(|result| match result {
                    Ok(results) if results.is_empty() => {
                        Some(view! { <p>"No images found — try different search text."</p> }.into_any())
                    }
                    Ok(_) => None,
                    Err(e) => Some(view! { <p>"Error: " {e.to_string()}</p> }.into_any()),
                })
            }}
            <div class="image-search-results">
                {move || {
                    let results: Vec<ImageSearchResult> = search
                        .value()
                        .get()
                        .and_then(Result::ok)
                        .unwrap_or_default();
                    results
                        .into_iter()
                        .map(|r| {
                            let provider = r.provider.clone();
                            let id = r.id.clone();
                            view! {
                                <img
                                    src=r.thumbnail_url
                                    alt=r.description
                                    class="image-search-thumb"
                                    data-provider=r.provider.clone()
                                    on:click=move |_| {
                                        import.dispatch(ImportCardImage {
                                            provider: provider.clone(),
                                            id: id.clone(),
                                        });
                                    }
                                />
                            }
                        })
                        .collect_view()
                }}
            </div>
            {move || import.pending().get().then(|| view! { <p>"Adding image…"</p> })}
            {move || {
                import.value().get().and_then(Result::err).map(|e| {
                    view! { <p>"Error: " {e.to_string()}</p> }
                })
            }}
            {move || {
                chosen_image_url.get().map(|url| view! {
                    <div class="image-search-chosen">
                        <img src=url alt="chosen image" />
                        <button type="button" on:click=move |_| chosen_image_url.set(None)>
                            "Remove"
                        </button>
                    </div>
                })
            }}
        </div>
    }
}

/// The image/audio/video/YouTube attachment fields shared by `NewCardPage`
/// and `EditCardPage` — previously hand-duplicated between the two with
/// only label wording and the presence of "remove" checkboxes differing.
/// `editing` switches between "Image file" (create) and "Replace image
/// (optional)" (edit) wording for all four fields; the `has_*` flags gate
/// each "Remove X" checkbox, since there's nothing to remove on a brand
/// new card (NewCardPage never passes `true` for any of them).
#[component]
fn MediaAttachmentFields(
    chosen_image_url: RwSignal<Option<String>>,
    audio_input: NodeRef<html::Input>,
    video_input: NodeRef<html::Input>,
    #[prop(default = false)] editing: bool,
    #[prop(default = false)] has_image: bool,
    #[prop(default = false)] has_audio: bool,
    #[prop(default = false)] has_video: bool,
    #[prop(default = false)] has_youtube: bool,
) -> impl IntoView {
    let image_label = if editing {
        "Replace image (optional)"
    } else {
        "Image file"
    };
    let audio_label = if editing {
        "Replace audio (optional)"
    } else {
        "Audio file"
    };
    let video_label = if editing {
        "Replace video (optional)"
    } else {
        "Video file"
    };
    let youtube_label = if editing {
        "Replace YouTube link (optional)"
    } else {
        "Or link a YouTube video"
    };

    view! {
        <label>{image_label} <input type="file" name="image" accept="image/*" /></label>
        <ImageSearchPicker chosen_image_url=chosen_image_url />
        {move || {
            chosen_image_url.get().map(|url| view! {
                <input type="hidden" name="image_url" value=url />
            })
        }}
        {has_image.then(|| view! {
            <label><input type="checkbox" name="remove_image" />"Remove image"</label>
        })}
        <label>
            {audio_label}
            <input type="file" name="audio" accept="audio/*" node_ref=audio_input />
        </label>
        <RecordButton kind="audio" target_input=audio_input />
        {has_audio.then(|| view! {
            <label><input type="checkbox" name="remove_audio" />"Remove audio"</label>
        })}
        <label>
            {video_label}
            <input type="file" name="video" accept="video/*" node_ref=video_input />
        </label>
        <RecordButton kind="video" target_input=video_input />
        {has_video.then(|| view! {
            <label><input type="checkbox" name="remove_video" />"Remove video"</label>
        })}
        <label>
            {youtube_label}
            <input type="url" name="youtube_url" placeholder="https://www.youtube.com/watch?v=..." />
        </label>
        {has_youtube.then(|| view! {
            <label><input type="checkbox" name="remove_youtube" />"Remove YouTube link"</label>
        })}
    }
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let auth = Resource::new(|| (), |()| current_user_name());

    view! {
        <Stylesheet id="leptos" href="/pkg/web.css"/>
        <Title text="Gazebo"/>

        <Router>
            <nav>
                <A href="/" exact=true>"Cards"</A>
                " · "
                <A href="/decks">"Decks"</A>
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        match auth.await.ok().flatten() {
                            Some(_) => view! {
                                " · "<A href="/new">"Add a card"</A>
                                " · "<A href="/bulk-add">"Bulk add"</A>
                                " · "<A href="/review">"Review"</A>
                                " · "<A href="/stats">"Stats"</A>
                                " · "<A href="/settings">"Settings"</A>
                                " · "<A href="/account">"Account"</A>
                                " · "<a href="/auth/logout">"Sign out"</a>
                            }.into_any(),
                            None => view! {
                                " · "<A href="/login">"Sign in"</A>
                                " · "<A href="/signup">"Sign up"</A>
                            }.into_any(),
                        }
                    })}
                </Transition>
            </nav>
            <main>
                // Two deliberately different auth-gating shapes below, not
                // an accident of implementation order: a page with no
                // meaningful signed-out use (NewCardPage, BulkAddPage,
                // ReviewPage, StatsPage, SettingsPage, AccountPage) hard-
                // redirects to /login via a `<Redirect>` inside its own
                // Suspend block. A page with a genuine public read-only
                // view (CardListPage, EditCardPage, DecksPage — browsing
                // cards/decks is intentionally open to anyone, per the
                // shared-card-pool model) instead degrades gracefully: it
                // renders for everyone, and conditions only the
                // mutating bits on `signed_in.then(...)`.
                <Routes fallback=|| "Page not found.">
                    <Route path=StaticSegment("") view=CardListPage/>
                    <Route path=StaticSegment("new") view=NewCardPage/>
                    <Route path=StaticSegment("bulk-add") view=BulkAddPage/>
                    <Route path=path!("/cards/:id") view=EditCardPage/>
                    <Route path=StaticSegment("decks") view=DecksPage/>
                    <Route path=StaticSegment("review") view=ReviewPage/>
                    <Route path=StaticSegment("stats") view=StatsPage/>
                    <Route path=StaticSegment("settings") view=SettingsPage/>
                    <Route path=StaticSegment("account") view=AccountPage/>
                    <Route path=StaticSegment("login") view=LoginPage/>
                    <Route path=StaticSegment("signup") view=SignupPage/>
                    <Route path=StaticSegment("forgot-password") view=ForgotPasswordPage/>
                    <Route path=StaticSegment("reset-password") view=ResetPasswordPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// Signup/login both end by setting a session cookie server-side and then
/// need the browser to actually pick it up. A client-side route change
/// (`use_navigate`, as `EditCardPage` uses after a delete) wouldn't do
/// that: `App`'s top-level nav `Resource` was already fetched once at
/// mount and a SPA-style transition never re-runs it. A full top-level
/// navigation does, so this deliberately uses a real browser navigation
/// rather than the router.
fn reload_to(path: &str) {
    _ = window().location().set_href(path);
}

#[component]
fn LoginPage() -> impl IntoView {
    let log_in = ServerAction::<LogIn>::new();
    let query = use_query_map();
    let just_reset = move || query.read().get("reset").as_deref() == Some("success");

    Effect::new(move |_| {
        if matches!(log_in.value().get(), Some(Ok(()))) {
            reload_to("/");
        }
    });

    view! {
        <h1>"Sign in"</h1>
        {move || just_reset().then(|| view! {
            <p>"Password updated — sign in with your new password."</p>
        })}
        <ActionForm action=log_in>
            <label>"Email" <input type="email" name="email" /></label>
            <label>"Password" <input type="password" name="password" /></label>
            <button type="submit">"Sign in"</button>
        </ActionForm>
        {move || {
            log_in.value().get().and_then(Result::err).map(|e| {
                view! { <p>"Error: " {e.to_string()}</p> }
            })
        }}
        <p><A href="/signup">"Need an account? Sign up"</A></p>
        <p><A href="/forgot-password">"Forgot your password?"</A></p>
    }
}

#[component]
fn SignupPage() -> impl IntoView {
    let sign_up = ServerAction::<SignUp>::new();

    Effect::new(move |_| {
        if matches!(sign_up.value().get(), Some(Ok(()))) {
            reload_to("/");
        }
    });

    view! {
        <h1>"Sign up"</h1>
        <ActionForm action=sign_up>
            <label>"Name" <input type="text" name="name" /></label>
            <label>"Email" <input type="email" name="email" /></label>
            <label>"Password" <input type="password" name="password" /></label>
            <label>"Confirm password" <input type="password" name="confirm_password" /></label>
            <button type="submit">"Sign up"</button>
        </ActionForm>
        {move || {
            sign_up.value().get().and_then(Result::err).map(|e| {
                view! { <p>"Error: " {e.to_string()}</p> }
            })
        }}
        <p><A href="/login">"Already have an account? Sign in"</A></p>
    }
}

#[component]
fn ForgotPasswordPage() -> impl IntoView {
    let request_reset = ServerAction::<RequestPasswordReset>::new();

    view! {
        <h1>"Forgot your password?"</h1>
        <ActionForm action=request_reset>
            <label>"Email" <input type="email" name="email" /></label>
            <button type="submit">"Send reset link"</button>
        </ActionForm>
        {move || {
            request_reset
                .value()
                .get()
                .map(|result| match result {
                    Ok(()) => view! {
                        <p>"If an account with that email exists, we've sent a password reset link."</p>
                    }.into_any(),
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                })
        }}
    }
}

#[component]
fn ResetPasswordPage() -> impl IntoView {
    let confirm_reset = ServerAction::<ConfirmPasswordReset>::new();
    let query = use_query_map();
    let token = move || query.read().get("token").unwrap_or_default();

    Effect::new(move |_| {
        if matches!(confirm_reset.value().get(), Some(Ok(()))) {
            reload_to("/login?reset=success");
        }
    });

    view! {
        <h1>"Reset your password"</h1>
        {move || {
            token().is_empty().then(|| view! { <p>"This password reset link is invalid."</p> })
        }}
        <ActionForm action=confirm_reset>
            <input type="hidden" name="token" value=token() />
            <label>"New password" <input type="password" name="password" /></label>
            <label>"Confirm new password" <input type="password" name="confirm_password" /></label>
            <button type="submit">"Reset password"</button>
        </ActionForm>
        {move || {
            confirm_reset.value().get().and_then(Result::err).map(|e| {
                view! { <p>"Error: " {e.to_string()}</p> }
            })
        }}
    }
}

#[component]
fn CardListPage() -> impl IntoView {
    // Seeded from `?deck=<id>` so DecksPage's "go see this deck's cards"
    // links actually land pre-filtered, not just on an unfiltered list.
    let query = use_query_map();
    let tag_filter = RwSignal::new(None::<String>);
    let deck_filter = RwSignal::new(
        query
            .get_untracked()
            .get("deck")
            .and_then(|id| Uuid::parse_str(&id).ok()),
    );

    let tags = Resource::new(|| (), |()| list_tags());
    let decks = Resource::new(|| (), |()| list_decks());
    let cards = Resource::new(
        move || (tag_filter.get(), deck_filter.get()),
        |(tag, deck_id)| crate::cards::list_cards(tag, deck_id),
    );

    view! {
        <h1>"Cards"</h1>
        <div>
            <label>
                "Filter by tag "
                <select
                    prop:value=move || tag_filter.get().unwrap_or_default()
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        tag_filter.set(if value.is_empty() { None } else { Some(value) });
                    }
                >
                    <option value="">"All tags"</option>
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            tags.await.unwrap_or_default().into_iter().map(|tag| {
                                view! { <option value=tag.clone()>{tag.clone()}</option> }
                            }).collect_view()
                        })}
                    </Transition>
                </select>
            </label>
            " "
            <label>
                "Filter by deck "
                <select
                    prop:value=move || {
                        deck_filter.get().map(|id| id.to_string()).unwrap_or_default()
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        deck_filter.set(Uuid::parse_str(&value).ok());
                    }
                >
                    <option value="">"All decks"</option>
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            decks.await.unwrap_or_default().into_iter().map(|deck| {
                                view! { <option value=deck.id.to_string()>{deck.name}</option> }
                            }).collect_view()
                        })}
                    </Transition>
                </select>
            </label>
        </div>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                match cards.await {
                    Ok(cards) if cards.is_empty() => {
                        // Distinguish "nothing matches this filter" from
                        // "you have zero cards at all" — only the latter
                        // gets a call to action, since a narrow filter was
                        // the user's own deliberate choice.
                        let unfiltered = tag_filter.get_untracked().is_none()
                            && deck_filter.get_untracked().is_none();
                        if unfiltered {
                            view! {
                                <p>"No cards yet. " <A href="/new">"Add the first one"</A>"."</p>
                            }
                            .into_any()
                        } else {
                            view! { <p>"No cards match these filters."</p> }.into_any()
                        }
                    }
                    Ok(cards) => view! {
                        <ul class="card-grid">
                            {cards
                                .into_iter()
                                .map(|card| {
                                    let edit_href = format!("/cards/{}", card.id);
                                    view! {
                                        <li>
                                            {card
                                                .image_url
                                                .map(|url| view! { <img src=url alt="" /> })}
                                            {card
                                                .audio_url
                                                .map(|url| view! { <audio controls src=url></audio> })}
                                            {card
                                                .video_url
                                                .map(|url| view! { <video controls src=url></video> })}
                                            {card.youtube_url.map(youtube_embed)}
                                            <span>{card.target_text}</span>
                                            " "
                                            <A href=edit_href>"Edit"</A>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                    .into_any(),
                    Err(e) => view! { <p>"Error loading cards: " {e.to_string()}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn NewCardPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let audio_input: NodeRef<html::Input> = NodeRef::new();
    let video_input: NodeRef<html::Input> = NodeRef::new();
    let chosen_image_url: RwSignal<Option<String>> = RwSignal::new(None);
    let target_text_signal: RwSignal<String> = RwSignal::new(String::new());

    view! {
        <h1>"Add a card"</h1>
        {form_error_banner()}
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                view! {
                    <form method="post" action="/cards/new" enctype="multipart/form-data">
                        <label>
                            "Target-language word/phrase"
                            <input
                                type="text"
                                name="target_text"
                                on:input=move |ev| target_text_signal.set(event_target_value(&ev))
                            />
                        </label>
                        <label>"Tags (comma separated)" <input type="text" name="tags" /></label>
                        <MediaAttachmentFields
                            chosen_image_url=chosen_image_url
                            audio_input=audio_input
                            video_input=video_input
                        />
                        <label>
                            <input type="checkbox" name="two_way" />
                            "Two-way: also review back → front, scheduled independently"
                        </label>
                        <button type="submit">"Add"</button>
                    </form>
                    <DictionaryDetailsEditor target_text=target_text_signal.into() />
                }
                .into_any()
            })}
        </Transition>
    }
}

/// One parsed line from the pasted list, waiting on the page for an image
/// before the batch is submitted. `key` is stable across this page's
/// lifetime (assigned once at parse time, never reused) so each draft's
/// own `ImageSearchPicker` keeps its chosen image if anything elsewhere on
/// the page re-renders.
#[derive(Debug, Clone)]
struct BulkDraft {
    key: usize,
    english: String,
    target_text: String,
}

/// Parses one `english,target word` line. The first comma splits english
/// from the target phrase; anything after a *second* comma stays part of
/// the target phrase rather than being treated as a parse error, so a
/// target phrase that itself contains a comma still works. `None` means
/// the line didn't look like "english,target" at all (missing comma, or
/// either side empty after trimming).
fn parse_bulk_line(line: &str) -> Option<(String, String)> {
    let (english, target) = line.trim().split_once(',')?;
    let english = english.trim();
    let target = target.trim();
    (!english.is_empty() && !target.is_empty()).then(|| (english.to_string(), target.to_string()))
}

#[component]
fn BulkAddPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let raw_input = RwSignal::new(String::new());
    let drafts: RwSignal<Vec<BulkDraft>> = RwSignal::new(Vec::new());
    let skipped_lines: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let next_key = RwSignal::new(0usize);

    // Parsing replaces `drafts` wholesale, which would also wipe out any
    // images already chosen on a re-parse — deliberately only ever
    // triggered by this explicit click, never reactively on every
    // keystroke, so starting to pick images is a safe point to stop
    // touching the textarea.
    let parse = move |_| {
        let mut parsed = Vec::new();
        let mut skipped = Vec::new();
        for line in raw_input.get_untracked().lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match parse_bulk_line(line) {
                Some((english, target_text)) => {
                    let key = next_key.get_untracked();
                    next_key.set(key + 1);
                    parsed.push(BulkDraft {
                        key,
                        english,
                        target_text,
                    });
                }
                None => skipped.push(line.to_string()),
            }
        }
        drafts.set(parsed);
        skipped_lines.set(skipped);
    };

    view! {
        <h1>"Bulk add cards"</h1>
        <p class="page-intro">
            "Faster than adding one at a time — paste a whole list, check the parsed results, then create them together."
        </p>
        {form_error_banner()}
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                view! {
                    <label>
                        "Paste one card per line: english,target word"
                        <textarea
                            rows="8"
                            on:input=move |ev| raw_input.set(event_target_value(&ev))
                        ></textarea>
                    </label>
                    <button type="button" on:click=parse>"Parse"</button>
                    {move || {
                        let skipped = skipped_lines.get();
                        (!skipped.is_empty()).then(|| {
                            let count = skipped.len();
                            let lines = skipped.join("; ");
                            view! {
                                <p>
                                    "Skipped " {count}
                                    " line(s) that weren't \"english,word\": " {lines}
                                </p>
                            }
                        })
                    }}
                    <form method="post" action="/cards/bulk" enctype="multipart/form-data">
                        {move || {
                            drafts
                                .get()
                                .into_iter()
                                .map(|draft| {
                                    let chosen_image_url: RwSignal<Option<String>> = RwSignal::new(None);
                                    let idx = draft.key;
                                    view! {
                                        <div class="bulk-draft">
                                            <input
                                                type="hidden"
                                                name=format!("target_text_{idx}")
                                                value=draft.target_text.clone()
                                            />
                                            <p>{draft.target_text.clone()} " (" {draft.english.clone()} ")"</p>
                                            <label>
                                                "Image file"
                                                <input
                                                    type="file"
                                                    name=format!("image_{idx}")
                                                    accept="image/*"
                                                />
                                            </label>
                                            <ImageSearchPicker
                                                chosen_image_url=chosen_image_url
                                                initial_query=draft.english
                                            />
                                            {move || {
                                                chosen_image_url.get().map(|url| view! {
                                                    <input
                                                        type="hidden"
                                                        name=format!("image_url_{idx}")
                                                        value=url
                                                    />
                                                })
                                            }}
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                        {move || {
                            (!drafts.get().is_empty())
                                .then(|| view! { <button type="submit">"Create all cards"</button> })
                        }}
                    </form>
                }
                .into_any()
            })}
        </Transition>
    }
}

#[component]
fn EditCardPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let params = use_params_map();
    let card_id = move || {
        params
            .read()
            .get("id")
            .and_then(|id| Uuid::parse_str(&id).ok())
    };

    let card = Resource::new(card_id, |id| async move {
        match id {
            Some(id) => get_card(id).await,
            None => Ok(None),
        }
    });
    let decks = Resource::new(|| (), |()| list_decks());

    let delete = ServerAction::<DeleteCard>::new();
    let toggle_deck = ServerAction::<AddCardToDeck>::new();
    let untoggle_deck = ServerAction::<RemoveCardFromDeck>::new();
    let audio_input: NodeRef<html::Input> = NodeRef::new();
    let video_input: NodeRef<html::Input> = NodeRef::new();
    let chosen_image_url: RwSignal<Option<String>> = RwSignal::new(None);

    let navigate = use_navigate();
    Effect::new(move |_| {
        // Was `.is_some()` — true on a *failed* delete too, so the page
        // navigated away and silently hid the error. Only a real success
        // should leave this page.
        if matches!(delete.value().get(), Some(Ok(()))) {
            navigate("/", leptos_router::NavigateOptions::default());
        }
    });

    view! {
        <h1>"Card"</h1>
        {form_error_banner()}
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                let Some(id) = card_id() else {
                    return view! { <p>"Invalid card."</p> }.into_any();
                };
                let signed_in = auth.await.ok().flatten().is_some();
                match card.await {
                    Ok(None) => view! { <p>"Card not found."</p> }.into_any(),
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                    Ok(Some(detail)) => {
                        let action = format!("/cards/{id}");
                        let image_view = detail
                            .image_url
                            .clone()
                            .map(|url| view! { <img src=url alt="" /> });
                        let audio_view = detail
                            .audio_url
                            .clone()
                            .map(|url| view! { <audio controls src=url></audio> });
                        let video_view = detail
                            .video_url
                            .clone()
                            .map(|url| view! { <video controls src=url></video> });
                        let youtube_view = detail.youtube_url.clone().map(youtube_embed);
                        let deck_ids = detail.deck_ids.clone();

                        // A signed-in editor sees the word immediately — visual
                        // confirmation of which card this is before the
                        // (possibly media-heavy) form below it. A signed-out
                        // visitor instead gets the same reveal-then-show shape
                        // as ReviewPage's forward direction: media is the
                        // prompt, the word is hidden behind "Reveal". Browsing
                        // a card to self-test is open to anyone; it's only
                        // *grading* (ReviewPage's Again/Hard/Good/Easy, which
                        // writes to the Anki-derived SRS state) that requires
                        // an account — there's no grading UI here at all.
                        let word_preview = {
                            let target_text = detail.target_text.clone();
                            let tags_display = detail.tags.join(", ");
                            let info = view! {
                                <div class="word-display"><ClickableWord text=target_text /></div>
                                {(!detail.tags.is_empty())
                                    .then(|| view! { <p>"Tags: " {tags_display}</p> })}
                            };
                            if signed_in {
                                info.into_any()
                            } else {
                                view! {
                                    <details>
                                        <summary>"Reveal"</summary>
                                        {info}
                                    </details>
                                }
                                .into_any()
                            }
                        };
                        // Tracks the input's *current* value reactively, so
                        // the "Dictionary details" editor below always saves
                        // against whatever's actually typed — including a
                        // just-edited rename not yet submitted — rather than
                        // the stale value this page loaded with.
                        let target_text_signal = RwSignal::new(detail.target_text.clone());
                        let existing_definition = get_word_definition(detail.target_text.clone())
                            .await
                            .ok()
                            .flatten();
                        let dictionary_editor = signed_in.then(|| {
                            let first_example = existing_definition
                                .as_ref()
                                .and_then(|d| d.examples.first());
                            view! {
                                <DictionaryDetailsEditor
                                    target_text=target_text_signal.into()
                                    part_of_speech_initial=existing_definition
                                        .as_ref()
                                        .and_then(|d| d.part_of_speech.clone())
                                        .unwrap_or_default()
                                    notes_initial=existing_definition
                                        .as_ref()
                                        .and_then(|d| d.notes.clone())
                                        .unwrap_or_default()
                                    example_target_initial=first_example
                                        .map(|e| e.target_sentence.clone())
                                        .unwrap_or_default()
                                    example_gloss_initial=first_example
                                        .and_then(|e| e.gloss_sentence.clone())
                                        .unwrap_or_default()
                                    linked_words_initial=existing_definition
                                        .as_ref()
                                        .map(|d| {
                                            d.linked_words
                                                .iter()
                                                .map(|l| l.target_text.clone())
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        })
                                        .unwrap_or_default()
                                />
                            }
                        });
                        let edit_form = signed_in.then(|| {
                            let tags_value = detail.tags.join(", ");
                            view! {
                                <form method="post" action=action enctype="multipart/form-data">
                                    <label>
                                        "Target-language word/phrase"
                                        <input
                                            type="text"
                                            name="target_text"
                                            value=detail.target_text.clone()
                                            on:input=move |ev| target_text_signal.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <label>"Tags (comma separated)" <input type="text" name="tags" value=tags_value /></label>
                                    <MediaAttachmentFields
                                        editing=true
                                        chosen_image_url=chosen_image_url
                                        audio_input=audio_input
                                        video_input=video_input
                                        has_image=detail.image_url.is_some()
                                        has_audio=detail.audio_url.is_some()
                                        has_video=detail.video_url.is_some()
                                        has_youtube=detail.youtube_url.is_some()
                                    />
                                    <label>
                                        <input type="checkbox" name="two_way" checked=detail.two_way />
                                        "Two-way: also review back → front, scheduled independently"
                                    </label>
                                    <button type="submit">"Save"</button>
                                </form>
                            }
                        });
                        let deck_section = {
                            let deck_ids = deck_ids.clone();
                            view! {
                                <div class="card-section">
                                    <p class="card-section-heading">"Decks"</p>
                                    <Transition fallback=|| view! { <p>"Loading decks…"</p> }>
                                    {move || {
                                        let deck_ids = deck_ids.clone();
                                        Suspend::new(async move {
                                            match decks.await {
                                                Ok(decks) if signed_in => decks
                                                    .into_iter()
                                                    .map(|deck| {
                                                        let in_deck = deck_ids.contains(&deck.id);
                                                        let deck_id = deck.id;
                                                        view! {
                                                            <label>
                                                                <input
                                                                    type="checkbox"
                                                                    checked=in_deck
                                                                    on:change=move |ev| {
                                                                        if event_target_checked(&ev) {
                                                                            toggle_deck.dispatch(AddCardToDeck { deck_id, card_id: id });
                                                                        } else {
                                                                            untoggle_deck.dispatch(RemoveCardFromDeck { deck_id, card_id: id });
                                                                        }
                                                                    }
                                                                />
                                                                {deck.name}
                                                            </label>
                                                        }
                                                    })
                                                    .collect_view()
                                                    .into_any(),
                                                Ok(decks) => {
                                                    let names: Vec<_> = decks
                                                        .into_iter()
                                                        .filter(|d| deck_ids.contains(&d.id))
                                                        .map(|d| d.name)
                                                        .collect();
                                                    if names.is_empty() {
                                                        view! { <p>"Not in any deck."</p> }.into_any()
                                                    } else {
                                                        view! { <p>{names.join(", ")}</p> }.into_any()
                                                    }
                                                }
                                                Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                                            }
                                        })
                                    }}
                                    </Transition>
                                </div>
                            }
                        };
                        let delete_form = signed_in.then(|| {
                            let armed = RwSignal::new(false);
                            view! {
                                <ActionForm action=delete>
                                    <input type="hidden" name="card_id" value=id.to_string() />
                                    {confirm_button("Delete card", armed)}
                                </ActionForm>
                                {move || {
                                    delete.value().get().and_then(Result::err).map(|e| {
                                        view! { <p>"Error: " {e.to_string()}</p> }
                                    })
                                }}
                            }
                        });
                        let sign_in_prompt = (!signed_in).then(|| {
                            view! { <p><A href="/login">"Sign in"</A>" to edit this card."</p> }
                        });

                        view! {
                            <div class="review-card">
                                {image_view}
                                {audio_view}
                                {video_view}
                                {youtube_view}
                                {word_preview}
                            </div>
                            {edit_form}
                            {dictionary_editor}
                            {deck_section}
                            {delete_form}
                            {sign_in_prompt}
                        }
                        .into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn DecksPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let create = ServerAction::<CreateDeck>::new();
    let rename = ServerAction::<RenameDeck>::new();
    let delete = ServerAction::<DeleteDeck>::new();
    let decks = Resource::new(
        move || {
            (
                create.version().get(),
                rename.version().get(),
                delete.version().get(),
            )
        },
        |_| list_decks(),
    );

    view! {
        <h1>"Decks"</h1>
        <p class="page-intro">
            "Group cards for focused review — a deck's cards show up in Review and in the card list's deck filter."
        </p>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                let signed_in = auth.await.ok().flatten().is_some();
                let create_form = signed_in.then(|| {
                    view! {
                        <ActionForm action=create>
                            <label>"New deck name" <input type="text" name="name" /></label>
                            <button type="submit">"Create"</button>
                        </ActionForm>
                        {move || {
                            create.value().get().and_then(Result::err).map(|e| {
                                view! { <p>"Error: " {e.to_string()}</p> }
                            })
                        }}
                    }
                });
                match decks.await {
                    Ok(decks) if decks.is_empty() => {
                        view! { {create_form} <p>"No decks yet."</p> }.into_any()
                    }
                    Ok(decks) => view! {
                        {create_form}
                        <ul class="deck-cabinet">
                            {decks
                                .into_iter()
                                .map(|deck| {
                                    let href = format!("/?deck={}", deck.id);
                                    let deck_id = deck.id;
                                    let name_value = deck.name.clone();
                                    let manage = signed_in.then(|| {
                                        let delete_armed = RwSignal::new(false);
                                        view! {
                                            <div class="manage">
                                                <ActionForm action=rename>
                                                    <input type="hidden" name="deck_id" value=deck_id.to_string() />
                                                    <input type="text" name="name" value=name_value />
                                                    <button type="submit">"Rename"</button>
                                                </ActionForm>
                                                <ActionForm action=delete>
                                                    <input type="hidden" name="deck_id" value=deck_id.to_string() />
                                                    {confirm_button("Delete", delete_armed)}
                                                </ActionForm>
                                            </div>
                                        }
                                    });
                                    view! {
                                        <li>
                                            <div class="label-holder"><a href=href>{deck.name}</a></div>
                                            <div class="pull"></div>
                                            {manage}
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                        // rename/delete are one ServerAction shared across every
                        // row's form, so their feedback is page-level, not
                        // per-row — correct given today's layout, worth
                        // revisiting if Phase B gives each row its own action.
                        {move || {
                            rename.value().get().and_then(Result::err).map(|e| {
                                view! { <p>"Error renaming deck: " {e.to_string()}</p> }
                            })
                        }}
                        {move || {
                            delete.value().get().and_then(Result::err).map(|e| {
                                view! { <p>"Error deleting deck: " {e.to_string()}</p> }
                            })
                        }}
                    }
                    .into_any(),
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn ReviewPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let order = RwSignal::new(ReviewOrderInput::DueDate);
    let deck_filter = RwSignal::new(None::<Uuid>);
    let decks = Resource::new(|| (), |()| list_decks());
    let grade = ServerAction::<GradeCard>::new();

    let queue = Resource::new(
        move || (deck_filter.get(), order.get(), grade.version().get()),
        |(deck_id, order, _)| review_queue(deck_id, order),
    );

    view! {
        <h1>"Review"</h1>
        <div>
            <label>
                "Filter by deck "
                <select on:change=move |ev| {
                    let value = event_target_value(&ev);
                    deck_filter.set(Uuid::parse_str(&value).ok());
                }>
                    <option value="">"All decks"</option>
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            decks.await.unwrap_or_default().into_iter().map(|deck| {
                                view! { <option value=deck.id.to_string()>{deck.name}</option> }
                            }).collect_view()
                        })}
                    </Transition>
                </select>
            </label>
            " "
            <label>
                "Order "
                <select on:change=move |ev| {
                    order.set(match event_target_value(&ev).as_str() {
                        "random" => ReviewOrderInput::Random,
                        "hardest" => ReviewOrderInput::HardestFirst,
                        _ => ReviewOrderInput::DueDate,
                    });
                }>
                    <option value="due">"Due date"</option>
                    <option value="random">"Random"</option>
                    <option value="hardest">"Hardest first"</option>
                </select>
            </label>
        </div>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                match queue.await {
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                    Ok(q) => {
                        let counts = view! {
                            <p>
                                {q.due_review_count} " due · " {q.new_available_today} " new available today"
                            </p>
                        };
                        match q.cards.into_iter().next() {
                            None => view! { {counts} <p>"Nothing due. Nice."</p> }.into_any(),
                            Some(card) => {
                                let card_id = card.id;
                                let direction = card.direction;
                                let media_view = view! {
                                    {card.image_url.map(|url| view! { <img src=url alt="" /> })}
                                    {card.audio_url.map(|url| view! { <audio controls src=url></audio> })}
                                    {card.video_url.map(|url| view! { <video controls src=url></video> })}
                                    {card.youtube_url.map(youtube_embed)}
                                }
                                .into_any();
                                // Forward: media is the prompt, target_text the
                                // answer. Reverse (two-way cards only): the same
                                // card, but target_text is the prompt and media
                                // is the answer — this slot is independently
                                // scheduled from the forward one.
                                let prompt_and_answer = match direction {
                                    DirectionInput::Forward => view! {
                                        {media_view}
                                        <details>
                                            <summary>"Reveal"</summary>
                                            <div class="word-display"><ClickableWord text=card.target_text /></div>
                                        </details>
                                    }
                                    .into_any(),
                                    DirectionInput::Reverse => view! {
                                        <div class="word-display"><ClickableWord text=card.target_text /></div>
                                        <details>
                                            <summary>"Reveal"</summary>
                                            {media_view}
                                        </details>
                                    }
                                    .into_any(),
                                };
                                view! {
                                    {counts}
                                    <div class="review-card">
                                        {prompt_and_answer}
                                        <div class="review-grades">
                                            <button on:click=move |_| {
                                                grade.dispatch(GradeCard { card_id, grade: GradeInput::Again, direction });
                                            }>"Again"</button>
                                            <button on:click=move |_| {
                                                grade.dispatch(GradeCard { card_id, grade: GradeInput::Hard, direction });
                                            }>"Hard"</button>
                                            <button on:click=move |_| {
                                                grade.dispatch(GradeCard { card_id, grade: GradeInput::Good, direction });
                                            }>"Good"</button>
                                            <button on:click=move |_| {
                                                grade.dispatch(GradeCard { card_id, grade: GradeInput::Easy, direction });
                                            }>"Easy"</button>
                                        </div>
                                    </div>
                                }
                                .into_any()
                            }
                        }
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn StatsPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let stats = Resource::new(|| (), |()| user_stats());

    view! {
        <h1>"Stats"</h1>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                match stats.await {
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                    Ok(s) => view! {
                        <ul class="ledger">
                            <li>{s.total_cards} " cards in the shared pool"</li>
                            <li>{s.due_now} " due right now"</li>
                            <li>{s.reviews_today} " reviews logged today"</li>
                            <li>{s.current_streak_days} " day streak"</li>
                        </ul>
                        <h2>"Cards you find hard"</h2>
                        {if s.hardest_cards.is_empty() {
                            view! { <p>"None yet — nice."</p> }.into_any()
                        } else {
                            view! {
                                <ul class="ledger">
                                    {s.hardest_cards
                                        .into_iter()
                                        .map(|c| {
                                            let href = format!("/cards/{}", c.id);
                                            view! {
                                                <li>
                                                    <A href=href>
                                                        {c.target_text} " — " {c.lapses} " lapses, difficulty "
                                                        {format!("{:.2}", c.difficulty)}
                                                    </A>
                                                </li>
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            }
                            .into_any()
                        }}
                    }
                    .into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn SettingsPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let set_limit = ServerAction::<SetDailyNewCardLimit>::new();
    let limit = Resource::new(
        move || set_limit.version().get(),
        |_| crate::account::get_daily_new_card_limit(),
    );

    view! {
        <h1>"Settings"</h1>
        <p class="page-intro">"Tune how your own review sessions pace themselves."</p>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                let current = limit.await.ok().flatten();
                view! {
                    <ActionForm action=set_limit>
                        <label>
                            "New cards per day (blank = unlimited)"
                            <input
                                type="number"
                                name="limit"
                                min="0"
                                value=current.map(|n| n.to_string()).unwrap_or_default()
                            />
                        </label>
                        <button type="submit">"Save"</button>
                    </ActionForm>
                    {move || {
                        set_limit.value().get().map(|result| match result {
                            Ok(()) => view! { <p>"Saved."</p> }.into_any(),
                            Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                        })
                    }}
                }
                .into_any()
            })}
        </Transition>
    }
}

#[component]
fn AccountPage() -> impl IntoView {
    let auth = Resource::new(|| (), |()| current_user_name());
    let update_name = ServerAction::<UpdateName>::new();
    let update_email = ServerAction::<UpdateEmail>::new();
    let change_password = ServerAction::<ChangePassword>::new();
    // Re-fetched after either name or email actually saves, so the forms
    // below reflect the new value rather than the stale one they were
    // first loaded with.
    let profile = Resource::new(
        move || (update_name.version().get(), update_email.version().get()),
        |_| current_profile(),
    );

    // Changing the password revokes every session server-side (see
    // account::change_password), so the only sane place left to land is
    // the login page — same message LoginPage already shows after a
    // password-reset redemption.
    Effect::new(move |_| {
        if matches!(change_password.value().get(), Some(Ok(()))) {
            reload_to("/login?reset=success");
        }
    });

    view! {
        <h1>"Account"</h1>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                let current = profile.await.ok();
                let name_value = current.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                let email_value = current.as_ref().map(|p| p.email.clone()).unwrap_or_default();

                view! {
                    <h2>"Name"</h2>
                    <ActionForm action=update_name>
                        <label>"Name" <input type="text" name="name" value=name_value /></label>
                        <button type="submit">"Save"</button>
                    </ActionForm>
                    {move || {
                        update_name.value().get().map(|result| match result {
                            Ok(()) => view! { <p>"Saved."</p> }.into_any(),
                            Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                        })
                    }}

                    <h2>"Email"</h2>
                    <ActionForm action=update_email>
                        <label>
                            "New email"
                            <input type="email" name="new_email" value=email_value />
                        </label>
                        <label>
                            "Current password"
                            <input type="password" name="current_password" />
                        </label>
                        <button type="submit">"Save"</button>
                    </ActionForm>
                    {move || {
                        update_email.value().get().map(|result| match result {
                            Ok(()) => view! { <p>"Saved. Sign in with your new email next time."</p> }.into_any(),
                            Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                        })
                    }}

                    <h2>"Password"</h2>
                    <ActionForm action=change_password>
                        <label>
                            "Current password"
                            <input type="password" name="current_password" />
                        </label>
                        <label>"New password" <input type="password" name="new_password" /></label>
                        <label>
                            "Confirm new password"
                            <input type="password" name="confirm_password" />
                        </label>
                        <button type="submit">"Change password"</button>
                    </ActionForm>
                    {move || {
                        change_password.value().get().and_then(Result::err).map(|e| {
                            view! { <p>"Error: " {e.to_string()}</p> }
                        })
                    }}
                }
                .into_any()
            })}
        </Transition>
    }
}

#[cfg(test)]
mod tests {
    use super::parse_bulk_line;

    #[test]
    fn parses_a_plain_line() {
        assert_eq!(
            parse_bulk_line("dog,perro"),
            Some(("dog".to_string(), "perro".to_string()))
        );
    }

    #[test]
    fn trims_whitespace_on_both_sides() {
        assert_eq!(
            parse_bulk_line("  dog , perro  "),
            Some(("dog".to_string(), "perro".to_string()))
        );
    }

    #[test]
    fn keeps_a_second_comma_as_part_of_the_target_phrase() {
        assert_eq!(
            parse_bulk_line("hello, how are you,hola, ¿cómo estás?"),
            Some((
                "hello".to_string(),
                "how are you,hola, ¿cómo estás?".to_string()
            ))
        );
    }

    #[test]
    fn rejects_a_line_with_no_comma() {
        assert_eq!(parse_bulk_line("just one word"), None);
    }

    #[test]
    fn rejects_a_line_with_an_empty_side() {
        assert_eq!(parse_bulk_line(",perro"), None);
        assert_eq!(parse_bulk_line("dog,"), None);
        assert_eq!(parse_bulk_line(" , "), None);
    }

    #[test]
    fn rejects_an_empty_line() {
        assert_eq!(parse_bulk_line(""), None);
    }
}
