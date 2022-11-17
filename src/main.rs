use std::collections::HashMap;
use futures::stream::FuturesUnordered;
use web_sys::{Document, HtmlDivElement, Window, HtmlImageElement};
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::JsCast;
use futures::StreamExt;
use surf::Client;
use polars::prelude::*;
use gh_wasm::gh_api::{GHClient, GHRepository, GHUser};


struct Repository {
    languages: Vec<String>,
}

struct GithubRepository {}

impl GithubRepository {
    /// Return a collection of users and the number of repositories where given language is used.
    /// Note: Will not return users that do not have any repository with given language. I.e.
    /// the returned count will always be larger than zero.
    fn get_users_by_repository_language(&self, language: &str) -> Vec<(String, usize)> {
        return vec![("Max".to_string(), 3)];
    }
}


fn render_avatars(users: Vec<GHUser>, users_df: PolarsResult<DataFrame>, repos_df: PolarsResult<DataFrame>) {
    let window = web_sys::window().unwrap();
    let document: Document = window.document().expect("no document?");
    log::info!("names {:?}", repos_df.as_ref().unwrap().dtypes());
    log::info!("names {:?}", repos_df.as_ref().unwrap().fields());
    let root: HtmlDivElement = document.get_element_by_id("root").unwrap().unchecked_into();
    let r1 = repos_df.as_ref().unwrap().groupby(["UserName"]);
    log::info!("r1: {:?}",r1);
    // if r1.is_ok() {
    //     // let r2 = r1.unwrap().select(["UserName", "Language"]).count();
    //     log::info!("ok");
    //     // log::info!("r2: {:?}",r2);
    // }else{
    //     log::info!("err");
    //     // log::info!("erro: {:?}", r1.as_ref().unwrap_err());
    // }
    //

    for user in users {
        let img: HtmlImageElement = root.append_child(&document.create_element("img").unwrap()).unwrap().unchecked_into();
        img.set_attribute("src", &user.avatar_url).unwrap();
        img.set_attribute("alt", &user.login).unwrap();
        img.set_attribute("width", "200").unwrap();
        img.set_attribute("height", "200").unwrap();
    }
}

async fn fetch_and_render_users() {
    let client = GHClient::new(Client::new(), Some("XYZ".to_string()));
    let users = client.get_org_members("codecentric").await.unwrap();

    // create a stream of (username, repositories) pairs. An item in the stream will become available
    // once its underlying fetch request is finished.
    let user_repo_stream: FuturesUnordered<_> = users
        .iter()
        .map(|user| async { (user.login.clone(), client.get_user_repositories(&user.login).await) })
        .collect();


    let user_repos: HashMap<String, Vec<GHRepository>> = user_repo_stream
        .filter_map(
            |(user, repos)| async {
                match repos {
                    Ok(x) => Some((user, x)),
                    Err(msg) => {
                        log::warn!("Failed to fetch repos for {user}: {msg}");
                        None
                    }
                }
            }
        ).collect()
        .await;

    let users_df: PolarsResult<DataFrame> = {
        let names = Series::new("UserName", users.iter().map(|u| u.login.clone()).collect::<Vec<String>>());
        let avatars = Series::new("Avatar", users.iter().map(|u| u.avatar_url.clone()).collect::<Vec<String>>());
        DataFrame::new(vec![names, avatars])
    };
    if users_df.is_ok() {
        log::info!("The users df: {:?}", users_df.as_ref().unwrap());
    } else {
        log::error!("Failed to build users df: {:?}", users_df.as_ref().unwrap_err());
    }

    let repos_df: PolarsResult<DataFrame> = {
        let names = Series::new(
            "UserName",
            ["A", "B"], // user_repos
            //     .iter()
            //     .map(
            //         |(login, repos)|
            //             std::iter::repeat(login.clone()).take(repos.len())
            //     ).flatten().collect::<Vec<String>>(),
        );
        let repos = Series::new(
            "Repository",
            ["R1", "R2"],// user_repos
            //     .iter()
            //     .map(
            //         |(login, repos)|
            //             repos.iter().map(|r| r.name.clone())
            //     ).flatten().collect::<Vec<String>>(),
        );
        let languages = Series::new(
            "Language",
            ["l1", "l2"],// user_repos
            //     .iter()
            //     .map(
            //         |(login, repos)|
            //             repos.iter().map(|r| r.language.clone())
            //     ).flatten().collect::<Vec<Option<String>>>(),
        );
        DataFrame::new(vec![names, repos, languages])
    };

    if repos_df.is_ok() {
        log::info!("The repos df: {:?}", repos_df.as_ref().unwrap());
    } else {
        log::error!("Failed to build repos df: {:?}", repos_df.as_ref().unwrap_err());
    }


    // log::debug!("repos: {user_repos:?}");
    // let repos = get_user_repositories(&token, &users[0].login).await;
    render_avatars(users, users_df, repos_df);
}

// #[wasm_bindgen]
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
    let val = root.append_child(&document.create_element("p").unwrap());
    log::info!("created element {:?}", &val);

    spawn_local(
        fetch_and_render_users()
    );

    let local_storage = window.local_storage().unwrap().unwrap();

    log::info!("local storage {:?}", &local_storage);
    let data = local_storage.get("cc-gh").unwrap();
    if data.is_none() {
        let download: bool = window.confirm_with_message("Could not find local CC Github data. Should it be fetched now?").unwrap();
        if download {
            log::info!("Fetching data from GH API.")
        } else {
            log::info!("well.. no data to display.")
        }
    }
    log::info!("local data {:?}", &data);
}
