use serde::Deserialize;
use gloo_net::http::{Method, Request};
use regex::Regex;

use web_sys::RequestMode;

#[derive(Debug, Deserialize, Clone)]
pub struct GHUser {
    pub login: String,
    pub id: usize,
    pub repos_url: String,
    pub avatar_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GHRepository {
    pub name: String,
    pub language: Option<String>,
}

pub struct GHClient {
    token: Option<String>,
}

impl GHClient {
    pub fn new(token: Option<String>) -> Self { Self { token } }

    /// Build a request including the token (if available) and CORS Mode.
    fn request(&self, method: Method, url: &str) -> Request {
        let mut request = Request::new(url)
            .method(method)
            .mode(RequestMode::Cors)
            .header("Accept", "application/vnd.github.v3+json");

        // any nice functional way to incorporate this in the above builder pattern?
        if let Some(token) = &self.token {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }
        request
    }

    /// Get a single page of organization members.
    async fn get_org_members_page(&self, org: &str, page: usize) -> Vec<GHUser> {
        log::debug!("fetching {org}-org member page {page}");
        let request = self.request(
            Method::GET,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30&page={page}"),
        );

        request.send().await.unwrap().json().await.unwrap()
    }

    /// Get all members of an organization.
    pub async fn get_org_members(&self, org: &str) -> Vec<GHUser> {
        log::info!("fetching organization members of {org}");

        let request = self.request(
            Method::HEAD,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let pagination_header = request
            .send()
            .await
            .unwrap()
            .headers()
            .get("link")// extract the "link" header from the response
            .unwrap();
        log::debug!("pagination header: {pagination_header}");
        let re = Regex::new(r".*?&page=([0-9]+).*").unwrap();

        // Note: in the API query parameters aren't zero based!
        let last_page: usize = re.captures_iter(&pagination_header).last().unwrap()[1].parse::<usize>().unwrap();
        log::debug!("last page: {last_page}");

        let mut users: Vec<GHUser> = Vec::with_capacity((last_page - 1) * 30);

        // sequential fetching of pages.
        for page in 1..last_page + 1 {
            users.append(&mut (self.get_org_members_page(org, page).await))
        }
        log::debug!("Loaded {0} users for {org}", users.len());

        log::debug!("{}", users[0].repos_url);
        return users;
    }

    /// Get a single page of the repositories of a user.
    async fn get_user_repositories_page(&self, user: &str, page: usize) -> Result<Vec<GHRepository>, String> {
        log::debug!("fetching {user} repository page {page}");
        let request = self.request(
            Method::GET,
            &format!("https://api.github.com/users/{user}/repos?per_page=30&page={page}"),
        );
        let response = request.send().await.map_err(|e| format!("Failed to send request: {e}"))?;

        let repos: Vec<GHRepository> = response.json().await.map_err(|e| e.to_string())?;
        Ok(repos)
    }

    pub async fn get_user_repositories(&self, user: &str) -> Result<Vec<GHRepository>, String> {
        log::info!("fetching user repositories for {user}");

        let request = self.request(
            Method::HEAD,
            &format!("https://api.github.com/users/{user}/repos?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let response = request.send().await.map_err(|e| e.to_string())?;
        log::debug!("Heaeders: {:?}", response.headers());


        let re = Regex::new(r".*?&page=([0-9]+).*").unwrap();

        // Note: in the API query parameters aren't zero based!
        let last_page: usize = match response.headers().get("link") {
            Some(pagination_header) => {
                log::debug!("pagination header: {pagination_header}");
                re
                    .captures_iter(&pagination_header)
                    .last()
                    .ok_or("Could not parse pagination header".to_string())?[1]
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?
            }
            None => 1,
        };
        log::debug!("last page: {last_page}");

        let mut repos: Vec<GHRepository> = Vec::with_capacity(last_page * 30);
        // concurrent fetching of pages
        let pages = ::futures::future::join_all((1..last_page + 1).map(|page: usize| async move { self.get_user_repositories_page(user, page).await }));
        for page in pages.await {
            repos.append(&mut page?);
        }
        log::debug!("Loaded {0} repos for {user}", repos.len());
        return Ok(repos);
    }
}

