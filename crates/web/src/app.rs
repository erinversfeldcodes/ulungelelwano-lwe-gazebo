// #[component] already enforces call sites use the returned view; clippy's
// pedantic must_use_candidate doesn't see through the macro expansion.
#![allow(clippy::must_use_candidate)]

use crate::account::{
    ChangePassword, ConfirmPasswordReset, LogIn, RequestPasswordReset, SetDailyNewCardLimit,
    SignUp, UpdateEmail, UpdateName, current_profile, current_user_name, user_stats,
};
use crate::cards::{
    DeleteCard, DirectionInput, GradeCard, GradeInput, ReviewOrderInput, get_card, list_tags,
    review_queue,
};
use crate::decks::{
    AddCardToDeck, CreateDeck, DeleteDeck, RemoveCardFromDeck, RenameDeck, list_decks,
};
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
                <A href="/">"Cards"</A>
                " · "
                <A href="/decks">"Decks"</A>
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        match auth.await.ok().flatten() {
                            Some(_) => view! {
                                " · "<A href="/new">"Add a card"</A>
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
                <Routes fallback=|| "Page not found.">
                    <Route path=StaticSegment("") view=CardListPage/>
                    <Route path=StaticSegment("new") view=NewCardPage/>
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
    let tag_filter = RwSignal::new(None::<String>);
    let deck_filter = RwSignal::new(None::<Uuid>);

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
                <select on:change=move |ev| {
                    let value = event_target_value(&ev);
                    tag_filter.set(if value.is_empty() { None } else { Some(value) });
                }>
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
        </div>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                match cards.await {
                    Ok(cards) if cards.is_empty() => view! { <p>"No cards match."</p> }.into_any(),
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

    view! {
        <h1>"Add a card"</h1>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if auth.await.ok().flatten().is_none() {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                view! {
                    <form method="post" action="/cards/new" enctype="multipart/form-data">
                        <label>"Image file" <input type="file" name="image" accept="image/*" /></label>
                        <label>
                            "Audio file"
                            <input type="file" name="audio" accept="audio/*" node_ref=audio_input />
                        </label>
                        <RecordButton kind="audio" target_input=audio_input />
                        <label>
                            "Video file"
                            <input type="file" name="video" accept="video/*" node_ref=video_input />
                        </label>
                        <RecordButton kind="video" target_input=video_input />
                        <label>
                            "Or link a YouTube video"
                            <input
                                type="url"
                                name="youtube_url"
                                placeholder="https://www.youtube.com/watch?v=..."
                            />
                        </label>
                        <label>"Target-language word/phrase" <input type="text" name="target_text" /></label>
                        <label>"Tags (comma separated)" <input type="text" name="tags" /></label>
                        <label>
                            <input type="checkbox" name="two_way" />
                            "Two-way: also review back → front, scheduled independently"
                        </label>
                        <button type="submit">"Add"</button>
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

    let navigate = use_navigate();
    Effect::new(move |_| {
        if delete.value().get().is_some() {
            navigate("/", leptos_router::NavigateOptions::default());
        }
    });

    view! {
        <h1>"Card"</h1>
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

                        // Anyone can view a card; only a signed-in user sees
                        // the mutable form, deck checkboxes, and delete
                        // button in place of their read-only equivalents.
                        let read_only_info = (!signed_in).then(|| {
                            let target_text = detail.target_text.clone();
                            let tags_display = detail.tags.join(", ");
                            view! {
                                <p>{target_text}</p>
                                {(!detail.tags.is_empty())
                                    .then(|| view! { <p>"Tags: " {tags_display}</p> })}
                            }
                        });
                        let edit_form = signed_in.then(|| {
                            let tags_value = detail.tags.join(", ");
                            // Only offered when there's actually something
                            // to remove — a brand-new card (NewCardPage)
                            // has no "remove" checkboxes at all, since
                            // nothing's attached yet to remove.
                            let remove_image = detail.image_url.is_some().then(|| view! {
                                <label>
                                    <input type="checkbox" name="remove_image" />
                                    "Remove image"
                                </label>
                            });
                            let remove_audio = detail.audio_url.is_some().then(|| view! {
                                <label>
                                    <input type="checkbox" name="remove_audio" />
                                    "Remove audio"
                                </label>
                            });
                            let remove_video = detail.video_url.is_some().then(|| view! {
                                <label>
                                    <input type="checkbox" name="remove_video" />
                                    "Remove video"
                                </label>
                            });
                            let remove_youtube = detail.youtube_url.is_some().then(|| view! {
                                <label>
                                    <input type="checkbox" name="remove_youtube" />
                                    "Remove YouTube link"
                                </label>
                            });
                            view! {
                                <form method="post" action=action enctype="multipart/form-data">
                                    <label>
                                        "Replace image (optional)"
                                        <input type="file" name="image" accept="image/*" />
                                    </label>
                                    {remove_image}
                                    <label>
                                        "Replace audio (optional)"
                                        <input type="file" name="audio" accept="audio/*" node_ref=audio_input />
                                    </label>
                                    <RecordButton kind="audio" target_input=audio_input />
                                    {remove_audio}
                                    <label>
                                        "Replace video (optional)"
                                        <input type="file" name="video" accept="video/*" node_ref=video_input />
                                    </label>
                                    <RecordButton kind="video" target_input=video_input />
                                    {remove_video}
                                    <label>
                                        "Replace YouTube link (optional)"
                                        <input
                                            type="url"
                                            name="youtube_url"
                                            placeholder="https://www.youtube.com/watch?v=..."
                                        />
                                    </label>
                                    {remove_youtube}
                                    <label>
                                        "Target-language word/phrase"
                                        <input type="text" name="target_text" value=detail.target_text.clone() />
                                    </label>
                                    <label>"Tags (comma separated)" <input type="text" name="tags" value=tags_value /></label>
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
                                <h2>"Decks"</h2>
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
                            }
                        };
                        let delete_form = signed_in.then(|| {
                            view! {
                                <ActionForm action=delete>
                                    <input type="hidden" name="card_id" value=id.to_string() />
                                    <button type="submit">"Delete card"</button>
                                </ActionForm>
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
                                {read_only_info}
                            </div>
                            {edit_form}
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
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                let signed_in = auth.await.ok().flatten().is_some();
                let create_form = signed_in.then(|| {
                    view! {
                        <ActionForm action=create>
                            <label>"New deck name" <input type="text" name="name" /></label>
                            <button type="submit">"Create"</button>
                        </ActionForm>
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
                                        view! {
                                            <div class="manage">
                                                <ActionForm action=rename>
                                                    <input type="hidden" name="deck_id" value=deck_id.to_string() />
                                                    <input type="text" name="name" value=name_value />
                                                    <button type="submit">"Rename"</button>
                                                </ActionForm>
                                                <ActionForm action=delete>
                                                    <input type="hidden" name="deck_id" value=deck_id.to_string() />
                                                    <button type="submit">"Delete"</button>
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
                "Deck "
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
                                            <p>{card.target_text}</p>
                                        </details>
                                    }
                                    .into_any(),
                                    DirectionInput::Reverse => view! {
                                        <p>{card.target_text}</p>
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
                                            view! {
                                                <li>
                                                    {c.target_text} " — " {c.lapses} " lapses, difficulty "
                                                    {format!("{:.2}", c.difficulty)}
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
                        update_name.value().get().and_then(Result::err).map(|e| {
                            view! { <p>"Error: " {e.to_string()}</p> }
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
                        update_email.value().get().and_then(Result::err).map(|e| {
                            view! { <p>"Error: " {e.to_string()}</p> }
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
