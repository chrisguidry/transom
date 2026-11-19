//! A browser's part of a sign-in, over plain HTTP, so no test opens a
//! browser.

use super::{ISSUER, TestServer};

impl TestServer {
    /// The trip a browser makes from `login`, the page the app sends it
    /// to, when `name`, without its `@`, signs in: the sign-in page, its
    /// link to the test issuer, the issuer's authorization endpoint,
    /// which signs `name` in at once, and transom's callback, which
    /// answers with a redirect to the app's callback carrying the
    /// one-time code. The redirects are followed by hand, carrying the
    /// cookie that binds the sign-in to its browser, so the last one is
    /// returned instead of followed.
    pub async fn browse(&self, login: &str, name: &str) -> String {
        let page = self.http.get(login).header("Accept", "text/html");
        let page = page.send().await.unwrap().text().await.unwrap();
        let link = format!("href=\"/login/{ISSUER}?");
        let (_, query) = page
            .split_once(&link)
            .expect("the page links to the issuer");
        let query = query.split_once('"').unwrap().0.replace("&amp;", "&");
        let begin = self.at(&format!("/login/{ISSUER}?{query}"));
        let to_issuer = self.http.get(begin).send().await.unwrap();
        let cookie = to_issuer.headers()["set-cookie"].to_str().unwrap();
        let cookie = cookie.split_once(';').unwrap().0.to_owned();
        // The issuer signs in whoever the authorization request names.
        let authorize = format!("{}&subject={name}&name={name}", location(&to_issuer));
        let back = location(&self.http.get(authorize).send().await.unwrap());
        let finish = self.http.get(back).header("Cookie", cookie);
        location(&finish.send().await.unwrap())
    }
}

/// Where `response` redirects to.
fn location(response: &reqwest::Response) -> String {
    response.headers()["location"].to_str().unwrap().to_owned()
}
