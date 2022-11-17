use serde::Deserialize;
// use gloo_net::http::{Method, Request};
use regex::Regex;
use surf::{Client, Request, Response};
use surf::http::Method;

// use web_sys::RequestMode;

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

impl GHClient {
    pub fn new(client: Client, token: Option<String>) -> Self { Self { client, token } }

    /// Build a request including the token (if available) and CORS Mode.
    fn request(&self, method: Method, url: &str) -> Request {
        let mut request = Request::new(method, url.try_into().unwrap());
        request.set_header("Accept", "application/vnd.github.v3+json");

        // any nice functional way to incorporate this in the above builder pattern?
        if let Some(token) = &self.token {
            request.set_header("Authorization", &format!("Bearer {}", token));
        }
        request
    }

    /// extract the last page number from the response headers.
    /// Note: Pagination is "one based" - I.e. a range 1..last_page + 1 in the GH API.
    fn last_page(response: Response) -> Result<usize, String> {

        // Note: in the API query parameters aren't zero based!
        match response.header("link") {
            Some(pagination_header) => {
                log::debug!("pagination header: {pagination_header}");
                let re = Regex::new(r".*?&page=([0-9]+).*").unwrap();
                let last_page: usize = re
                    .captures_iter(&pagination_header.to_string())
                    .last()
                    .ok_or("Could not parse pagination header".to_string())?[1]
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?;
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
    async fn get_org_members_page(&self, org: &str, page: usize) -> Result<Vec<GHUser>, String> {
        log::debug!("fetching {org}-org member page {page}");
        let request = self.request(
            Method::Get,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30&page={page}"),
        );

        let mut response = self.client.send(request).await.map_err(|e| format!("Failed to send request: {e}"))?;
        let members: Vec<GHUser> = response.body_json().await.map_err(|e| format!("Failed to parse Users from body: {e}"))?;
        Ok(members)
    }

    /// Get all members of an organization.
    pub async fn get_org_members(&self, org: &str) -> Result<Vec<GHUser>, String> {
        log::info!("fetching organization members of {org}");

        let request = self.request(
            surf::http::Method::Head,
            &format!("https://api.github.com/orgs/{org}/members?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let response = self.client.send(request)
            .await
            .map_err(|e| format!("failed to fetch page count: {e}"))?;

        // Note: in the API query parameters aren't zero based!
        let last_page: usize = GHClient::last_page(response).map_err(|e| format!("Failed to get page count: {e}"))?;

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
    async fn get_user_repositories_page(&self, user: &str, page: usize) -> Result<Vec<GHRepository>, String> {
        log::debug!("fetching {user} repository page {page}");
        let request = self.request(
            Method::Get,
            &format!("https://api.github.com/users/{user}/repos?per_page=30&page={page}"),
        );
        let mut response = self.client.send(request).await.map_err(|e| format!("Failed to send request: {e}"))?;

        let repos: Vec<GHRepository> = response.body_json().await.map_err(|e| e.to_string())?;
        Ok(repos)
    }

    pub async fn get_user_repositories(&self, user: &str) -> Result<Vec<GHRepository>, String> {
        log::info!("fetching user repositories for {user}");

        let request = self.request(
            Method::Head,
            &format!("https://api.github.com/users/{user}/repos?per_page=30"),
        );

        // get the link header
        // link: <.../{org}/members?page=2>; rel="next", <...{org}/members?page=123>; rel="last"
        let response = self.client.send(request).await.map_err(|e| e.to_string())?;
        let last_page: usize = GHClient::last_page(response).map_err(|e| format!("Failed to get page count: {e}"))?;

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
    use crate::gh_api::GHClient;

    #[test]
    async fn test_get_repos_single_page() {
        let client = GHClient::new(Client::new(), None);
        let repos = client.get_user_repositories("maiksensi").await;
        assert!(repos.is_ok());
        // make sure below number matches with https://github.com/maiksensi
        assert_eq!(29, repos.unwrap().len());
    }

    #[test]
    async fn test_get_repos_multi_page() {
        let client = GHClient::new(Client::new(), None);
        let repos = client.get_user_repositories("jonashackt").await;
        assert!(repos.is_ok());
        // make sure below number matches with https://github.com/jonashackt
        assert_eq!(145, repos.unwrap().len());
    }
}