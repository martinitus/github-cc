use serde::Deserialize;
use regex::Regex;
use surf::{Client, Request, Response, StatusCode};
use surf::http::Method;
use anyhow::{anyhow, Result};

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
    client: Client,
}

// FIXME: turn this into a trait and provide blanket implementations for tower::Service<HTTPRequest>
//        see: https://docs.rs/tower-http/latest/tower_http/index.html#example-client
impl GHClient {
    pub fn new(client: Client, token: Option<String>) -> Self { Self { client, token } }

    /// Build a request including the token (if available) and CORS Mode.
    fn request(&self, method: Method, url: &str) -> Request {
        let mut request = Request::new(method, url.try_into().unwrap());
        request.set_header("Accept", "application/vnd.github.v3+json");
        // fixme: rework the header injection via the tower_http service layers.
        request.set_header("User-Agent", "Awesome-Octocat-App");

        // any nice functional way to incorporate this in the above builder pattern?
        if let Some(token) = &self.token {
            request.set_header("Authorization", &format!("Bearer {}", token));
        }
        request
    }

    /// extract the last page number from the response headers.
    /// Note: Pagination is "one based" - I.e. a range 1..last_page + 1 in the GH API.
    fn last_page(response: Response) -> Result<usize> {
        // Note: in the API query parameters aren't zero based!
        match response.header("link") {
            Some(pagination_header) => {
                log::debug!("pagination header: {pagination_header}");
                let re = Regex::new(r".*&page=([0-9]+).*").expect("Failed to construct regex");
                let last_page: usize = re
                    .captures_iter(&pagination_header.to_string())
                    // .map(|c| {
                    //     println!("match: {c:?}");
                    //     c
                    // })
                    .last()
                    .ok_or(anyhow!("Could not parse pagination header"))?[1]
                    .parse::<usize>()?;
                log::debug!("last page from header: {last_page}");
                Ok(last_page)
            }
            None => {
                log::debug!("no page header found. assuming single page.");
                Ok(1)
            }
        }
    }

    /// Get a single page of organization members.
    async fn get_org_members_page(&self, org: &str, page: usize) -> Result<Vec<GHUser>> {
        log::debug!("fetching {org}-org member page {page}");
        let request = self.request(
            Method::Get,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30&page={page}"),
        );

        let mut response = self.client.send(request).await.map_err(|e| anyhow!("Failed sending request: {e:?}"))?;
        let members: Vec<GHUser> = response.body_json().await.map_err(|e| anyhow!("Failed reading body: {e:?}"))?;
        Ok(members)
    }

    /// Get all members of an organization.
    pub async fn get_org_members(&self, org: &str) -> Result<Vec<GHUser>> {
        log::info!("fetching organization members of {org}");

        let request = self.request(
            surf::http::Method::Head,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let response = self.client.send(request).await.map_err(|e| anyhow!("Failed sending request: {e:?}"))?;

        // Note: in the API query parameters aren't zero based!
        let last_page: usize = GHClient::last_page(response)?;

        let mut users: Vec<GHUser> = Vec::with_capacity((last_page - 1) * 30);

        // sequential fetching of pages.
        for page in 1..last_page + 1 {
            let mut pages = self.get_org_members_page(org, page).await?;
            users.append(&mut pages)
        }
        log::debug!("Loaded {0} users for {org}", users.len());

        return Ok(users);
    }

    /// Get a single page of the repositories of a user.
    async fn get_user_repositories_page(&self, user: &str, page: usize) -> Result<Vec<GHRepository>> {
        log::debug!("fetching {user} repository page {page}");
        let request = self.request(
            Method::Get,
            &format!("https://api.github.com/users/{user}/repos?per_page=30&page={page}"),
        );
        let mut response = self.client.send(request).await.map_err(|e| anyhow!("Failed sending request: {e:?}"))?;
        if response.status() != StatusCode::Ok {
            return Err(anyhow!("Request failed with {}", response.status()));
        }

        let repos: Vec<GHRepository> = response.body_json().await.map_err(|e| anyhow!("Failed reading body: {e:?}"))?;
        Ok(repos)
    }

    pub async fn get_user_repositories(&self, user: &str) -> Result<Vec<GHRepository>> {
        log::info!("fetching user repositories for {user}");

        let request = self.request(
            Method::Head,
            &format!("https://api.github.com/users/{user}/repos?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let response = self.client.send(request).await.map_err(|e| anyhow!("Failed sending request: {e:?}"))?;
        let last_page: usize = GHClient::last_page(response)?;

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


#[cfg(test)]
mod tests {
    use surf::Client;
    use crate::GHClient;
    use rstest::*;
    use anyhow::Result;

    #[fixture]
    fn token() -> String {
        dotenv::dotenv().ok();
        std::env::var("GH_API_TOKEN").expect("Failed to load github token from environment variables.")
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_repos_page(token: String) -> Result<()> {
        // let http_client: Client = surf::Config::new()
        //     // fixme: this here seems to get ignored in the GHClient, because it builds
        //     //        its own request and does not merge the header. Better use tower_http as
        //     //        client abstraction in the future, given there is an wasm compatible
        //     //        service implementation.
        //     // .add_header("User-Agent", "Awesome-Octocat-App")
        //     .unwrap()
        //     .try_into()
        //     .unwrap();
        let client = GHClient::new(Client::new(), Some(token));
        let repos = client.get_user_repositories("maiksensi").await?;
        println!("repos: {repos:?}");
        // make sure below number matches with https://github.com/maiksensi
        assert_eq!(29, repos.len());
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_repos_multi_page(token: String) -> Result<()> {
        let client = GHClient::new(Client::new(), Some(token));
        let repos = client.get_user_repositories("jonashackt").await?;
        println!("repos: {repos:?}");
        // make sure below number matches with https://github.com/jonashackt
        assert_eq!(145, repos.len());
        Ok(())
    }
}