use crate::account::{RequestMagicLink, current_user_name};
use crate::cards::{CreateCard, GradeCard, GradeInput, due_cards, list_cards};
use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{A, Redirect, Route, Router, Routes},
};

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

    view! {
        <Stylesheet id="leptos" href="/pkg/web.css"/>
        <Title text="Gazebo"/>

        <Router>
            <nav>
                <A href="/">"Cards"</A>
                " · "
                <A href="/new">"Add a card"</A>
                " · "
                <A href="/review">"Review"</A>
                " · "
                <a href="/auth/logout">"Sign out"</a>
            </nav>
            <main>
                <Routes fallback=|| "Page not found.">
                    <Route path=StaticSegment("") view=CardListPage/>
                    <Route path=StaticSegment("new") view=NewCardPage/>
                    <Route path=StaticSegment("review") view=ReviewPage/>
                    <Route path=StaticSegment("login") view=LoginPage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let send_link = ServerAction::<RequestMagicLink>::new();

    view! {
        <h1>"Sign in"</h1>
        <ActionForm action=send_link>
            <label>"Email" <input type="email" name="email" /></label>
            <button type="submit">"Send magic link"</button>
        </ActionForm>
        {move || {
            send_link
                .value()
                .get()
                .map(|result| match result {
                    Ok(_) => view! { <p>"Check your email for a sign-in link."</p> }.into_any(),
                    Err(e) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                })
        }}
    }
}

#[component]
fn CardListPage() -> impl IntoView {
    let data = Resource::new(
        || (),
        |_| async move {
            match current_user_name().await {
                Ok(Some(_)) => Some(list_cards().await),
                _ => None,
            }
        },
    );

    view! {
        <h1>"Cards"</h1>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                match data.await {
                    None => view! { <Redirect path="/login"/> }.into_any(),
                    Some(Ok(cards)) if cards.is_empty() => {
                        view! { <p>"No cards yet."</p> }.into_any()
                    }
                    Some(Ok(cards)) => view! {
                        <ul>
                            {cards
                                .into_iter()
                                .map(|card| {
                                    view! {
                                        <li>
                                            {card
                                                .image_url
                                                .map(|url| view! { <img src=url width="80" alt="" /> })}
                                            <span>{card.target_text}</span>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                    .into_any(),
                    Some(Err(e)) => view! { <p>"Error loading cards: " {e.to_string()}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
fn NewCardPage() -> impl IntoView {
    let signed_in = Resource::new(
        || (),
        |_| async move { matches!(current_user_name().await, Ok(Some(_))) },
    );
    let create = ServerAction::<CreateCard>::new();

    view! {
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                if !signed_in.await {
                    return view! { <Redirect path="/login"/> }.into_any();
                }
                view! {
                    <h1>"Add a card"</h1>
                    <ActionForm action=create>
                        <label>"Image URL" <input type="text" name="image_url" /></label>
                        <label>"Target-language word/phrase" <input type="text" name="target_text" /></label>
                        <label>"Tags (comma separated)" <input type="text" name="tags" /></label>
                        <button type="submit">"Add"</button>
                    </ActionForm>
                    {move || {
                        create
                            .value()
                            .get()
                            .map(|result| match result {
                                Ok(_) => view! { <p>"Added."</p> }.into_any(),
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
fn ReviewPage() -> impl IntoView {
    let grade = ServerAction::<GradeCard>::new();
    let data = Resource::new(
        move || grade.version().get(),
        |_| async move {
            match current_user_name().await {
                Ok(Some(_)) => Some(due_cards().await),
                _ => None,
            }
        },
    );

    view! {
        <h1>"Review"</h1>
        <Transition fallback=|| view! { <p>"Loading…"</p> }>
            {move || Suspend::new(async move {
                match data.await {
                    None => view! { <Redirect path="/login"/> }.into_any(),
                    Some(Err(e)) => view! { <p>"Error: " {e.to_string()}</p> }.into_any(),
                    Some(Ok(cards)) => match cards.into_iter().next() {
                        None => view! { <p>"Nothing due. Nice."</p> }.into_any(),
                        Some(card) => {
                            let card_id = card.id;
                            view! {
                                <div>
                                    {card
                                        .image_url
                                        .map(|url| view! { <img src=url width="200" alt="" /> })}
                                    <details>
                                        <summary>"Reveal"</summary>
                                        <p>{card.target_text}</p>
                                    </details>
                                    <div>
                                        <button on:click=move |_| {
                                            grade.dispatch(GradeCard { card_id, grade: GradeInput::Again });
                                        }>"Again"</button>
                                        <button on:click=move |_| {
                                            grade.dispatch(GradeCard { card_id, grade: GradeInput::Hard });
                                        }>"Hard"</button>
                                        <button on:click=move |_| {
                                            grade.dispatch(GradeCard { card_id, grade: GradeInput::Good });
                                        }>"Good"</button>
                                        <button on:click=move |_| {
                                            grade.dispatch(GradeCard { card_id, grade: GradeInput::Easy });
                                        }>"Easy"</button>
                                    </div>
                                </div>
                            }
                            .into_any()
                        }
                    },
                }
            })}
        </Transition>
    }
}
