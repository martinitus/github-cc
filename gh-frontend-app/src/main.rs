use std::collections::HashMap;
use futures::stream::FuturesUnordered;
use web_sys::{Document, HtmlDivElement, Window, HtmlImageElement, HtmlProgressElement, HtmlLabelElement, HtmlInputElement, HtmlParagraphElement, HtmlUListElement, HtmlLiElement};
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::JsCast;
use futures::StreamExt;
use surf::Client;
use wasm_bindgen::closure::Closure;
use gh_client::{GHClient, GHRepository, GHUser};


/// Group repositories by language and return counts for every language.
fn language_count(repositories: Vec<GHRepository>) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for repo in repositories {
        if let Some(language) = &repo.language {
            if map.contains_key(language) {
                *map.get_mut(language).unwrap() += 1;
            } else {
                map.insert(language.clone(), 1);
            }
        }
    }
    map
    // let mut vec: Vec<(usize, String)> = map.into_iter().map(|(k, v)| (v, k)).collect();
    // vec.sort_by(|e1, e2| e2.0.cmp(&e1.0));
    // vec
}


fn render_avatars(window: &Window, user_repos: Vec<(GHUser, Vec<GHRepository>)>) {
    const USER_CONTAINER_ID: &str = "gh-frontend-app-users";
    const LANGUAGE_INPUT_ID: &str = "gh-frontend-app-language-input";

    let document: Document = window.document().expect("no document?");
    let root: HtmlDivElement = document.get_element_by_id("root").unwrap().unchecked_into();

    let user_languages: Vec<(GHUser, HashMap<String, usize>)> = user_repos.into_iter().map(
        |(user, repos)| {
            (user, language_count(repos))
        }
    ).collect();

    let header: HtmlDivElement = root.append_child(&document.create_element("div").unwrap()).unwrap().unchecked_into();
    header.set_text_content(Some("Search for language:"));

    let language: HtmlInputElement = header.append_child(&document.create_element("input").unwrap()).unwrap().unchecked_into();
    language.set_attribute("type", "text").unwrap();
    language.set_attribute("size", "20").unwrap();
    language.set_id(LANGUAGE_INPUT_ID);

    {
        let user_languages = user_languages.clone();
        let document = document.clone();
        let a = Closure::<dyn Fn(_)>::new(
            move |event: web_sys::InputEvent| {
                log::debug!("event: {:?}", event);
                let input: HtmlInputElement = document.get_element_by_id(LANGUAGE_INPUT_ID).unwrap().unchecked_into();
                let search = input.value();
                log::debug!("input-value: {search}");

                let mut tmp = user_languages.iter().filter_map(
                    |(user, languages)| {
                        for (language, count) in languages.iter() {
                            if language.to_ascii_lowercase().contains(&search.to_ascii_lowercase()) {
                                return Some((count.clone(), user.clone(), languages.clone()));
                            }
                        }
                        return None;
                    }).collect::<Vec<_>>();

                tmp.sort_by(|e1, e2| e2.0.cmp(&e1.0));

                // drop the count that was only needed for sorting and pass it to rendering
                let tmp = tmp.into_iter().map(|(count, user, languages)| (user, languages)).collect();
                re_render(tmp);
            }
        );
        language.add_event_listener_with_callback("input", a.as_ref().unchecked_ref()).unwrap();
        // Avoid dangling closure. Essentially, we tell rust not to clean up the
        // closure once this method returns (i.e. scope ends).
        // See also: https://rustwasm.github.io/wasm-bindgen/examples/closures.html
        a.forget();
    }

    re_render(user_languages);

    fn re_render(user_languages: Vec<(GHUser, HashMap<String, usize>)>) {
        log::debug!("rendering");
        let window: Window = web_sys::window().expect("no window?");
        let document: Document = window.document().expect("no document?");
        let root: HtmlDivElement = document.get_element_by_id("root").unwrap().unchecked_into();

        if let Some(user_containers) = document.get_element_by_id(USER_CONTAINER_ID) {
            root.remove_child(&user_containers).unwrap();
            log::debug!("Removed old content")
        };

        let user_containers: HtmlDivElement = root.append_child(&document.create_element("div").unwrap()).unwrap().unchecked_into();
        user_containers.set_id(USER_CONTAINER_ID);

        for (user, languages) in user_languages {
            let user_container: HtmlDivElement = user_containers.append_child(&document.create_element("div").unwrap()).unwrap().unchecked_into();
            let name_p: HtmlParagraphElement = user_container.append_child(&document.create_element("h1").unwrap()).unwrap().unchecked_into();
            name_p.set_text_content(Some(&user.login));

            let avatar_and_languages_container: HtmlParagraphElement = user_container.append_child(&document.create_element("div").unwrap()).unwrap().unchecked_into();
            avatar_and_languages_container.set_attribute("style", "display: flex; align-items: center;").unwrap();
            let img: HtmlImageElement = avatar_and_languages_container.append_child(&document.create_element("img").unwrap()).unwrap().unchecked_into();
            img.set_attribute("src", &user.avatar_url).unwrap();
            img.set_attribute("alt", &user.login).unwrap();
            img.set_attribute("width", "200").unwrap();
            img.set_attribute("height", "200").unwrap();
            let languages_p: HtmlParagraphElement = avatar_and_languages_container.append_child(&document.create_element("p").unwrap()).unwrap().unchecked_into();
            let list: HtmlUListElement = languages_p.append_child(&document.create_element("ul").unwrap()).unwrap().unchecked_into();
            for (lang, count) in languages {
                let i: HtmlLiElement = list.append_child(&document.create_element("li").unwrap()).unwrap().unchecked_into();
                i.set_text_content(Some(&format!("{lang} ({count})")));
            }
        }
    }
}

async fn fetch_user_repos(window: &Window, token: &str, organization: &str) -> Vec<(GHUser, Vec<GHRepository>)> {
    let document: Document = window.document().expect("no document?");
    let root: HtmlDivElement = document.get_element_by_id("root").unwrap().unchecked_into();

    let client = GHClient::new(Client::new(), Some(token.to_string()));

    let label: HtmlLabelElement = root.append_child(&document.create_element("label").unwrap()).unwrap().unchecked_into();
    label.set_attribute("for", "progress").unwrap();
    label.set_text_content(Some("Fetching Users"));
    let progress: HtmlProgressElement = root.append_child(&document.create_element("progress").unwrap()).unwrap().unchecked_into();
    progress.set_attribute("id", "progress").unwrap();

    let users = client.get_org_members(organization).await.unwrap();
    let total = users.len();
    label.set_text_content(Some("Fetching repositories for users:"));
    progress.set_attribute("value", "0").unwrap();
    progress.set_attribute("max", &total.to_string()).unwrap();

    // create a stream of (username, repositories) pairs. An item in the stream will become available
    // once its underlying fetch request is finished.
    let user_repo_stream: FuturesUnordered<_> = users
        .into_iter()
        .map(|user| async {
            let repos = client.get_user_repositories(&user.login).await;
            (user, repos)
        })
        .collect();

    // await all the items in the stream and store them in the return hashmap.
    let user_repos = user_repo_stream
        .filter_map(
            |(user, repos)| async {
                match repos {
                    Ok(x) => {
                        let new = progress.value() + 1.;
                        progress.set_value(new);
                        label.set_text_content(Some(&format!("Fetching repositories for users ({new:3.0}/{total:3.0}):")));
                        Some((user, x))
                    }
                    Err(msg) => {
                        log::warn!("Failed to fetch repos for {user:?}: {msg}");
                        None
                    }
                }
            }
        ).collect()
        .await;
    root.remove_child(&label).unwrap();
    root.remove_child(&progress).unwrap();
    user_repos
}

/// Try to load the GH-API token from the browsers local storage.
///
/// If the token is not yet in local storage, then an input dialog asking the
/// user to provide the it will be displayed, and the token will be saved in local storage.
fn get_api_token(window: &Window) -> String {
    const TOKEN_STORAGE_KEY: &str = "gh-frontend-app-api-token";
    let local_storage = window.local_storage().unwrap().unwrap();
    let token = local_storage.get(TOKEN_STORAGE_KEY).unwrap();
    match token {
        None => {
            // fixme: handle case where used clicks cancel.
            let token = window.prompt_with_message("Please provide GH-API token:").unwrap().unwrap();
            local_storage.set(TOKEN_STORAGE_KEY, &token).expect("Failed to store token in local storage.");
            token
        }
        Some(token) => { token }
    }
}

/// Try to load the User/Repositories from browser local storage.
///
/// If local storage does not contain the data, it will be fetched from GH and stored in local storage.
async fn get_user_repos(window: &Window, token: &str, organization: &str) -> Vec<(GHUser, Vec<GHRepository>)> {
    const USER_REPOSITORIES_STORAGE_KEY: &str = "gh-frontend-app-user-repositories";
    let local_storage = window.local_storage().unwrap().unwrap();
    let data = local_storage.get(USER_REPOSITORIES_STORAGE_KEY).unwrap();
    match data {
        None => {
            let data = fetch_user_repos(window, token, organization).await;
            let json = serde_json::to_string(&data).unwrap();
            log::info!("Storing data in local storage");
            local_storage.set(USER_REPOSITORIES_STORAGE_KEY, &json).unwrap();
            data
        }
        Some(data) => {
            log::debug!("Deserializing data from local storage");
            serde_json::from_str(&data).unwrap()
        }
    }
}

// #[wasm_bindgen] done by trunk :)
fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    log::info!("Hello, world!");
    let window: Window = web_sys::window().expect("no window?");
    log::info!("got window {:?}", &window);
    let document: Document = window.document().expect("no document?");
    log::info!("got document {:?}", &document);
    let body = document.body().expect("no body?");
    log::info!("got body {:?}", &body);
    let root: HtmlDivElement = document.get_element_by_id("root").unwrap().unchecked_into();
    log::info!("root div {:?}", &root);

    spawn_local(
        async {
            let window: Window = web_sys::window().expect("no window?");

            let token = get_api_token(&window);
            let user_repos = get_user_repos(&window, &token, "codecentric").await;


            // log::debug!("repos: {user_repos:?}");
            // let repos = get_user_repositories(&token, &users[0].login).await;
            render_avatars(&window, user_repos);
        }
    );
}
