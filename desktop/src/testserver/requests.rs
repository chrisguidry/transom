//! The people around the app: each request here is one a person other
//! than the app's makes, as curl or another client would, so the app's
//! own client is tested only by what it does itself.

use reqwest::Method;
use serde_json::{Value, json};

use super::{Person, TestServer};

impl TestServer {
    /// `name`, without its `@`, as a person the server has an account
    /// for: transom makes the account at their first request.
    pub async fn person(&self, name: &str) -> Person {
        let person = Person {
            name: format!("@{name}"),
            token: self.issuer.token_named(name, name),
        };
        let path = format!("/{}", person.name);
        self.call(Method::GET, &path, &person, Value::Null).await;
        person
    }

    /// Sends `method` `path` as `by`, with `body` as JSON unless it is
    /// null, and returns the answer's JSON, or null when it has none.
    /// The server must answer that the request succeeded.
    pub async fn call(&self, method: Method, path: &str, by: &Person, body: Value) -> Value {
        let request = self
            .http
            .request(method.clone(), self.at(path))
            .bearer_auth(&by.token);
        let request = if body.is_null() {
            request
        } else {
            request.json(&body)
        };
        let response = request.send().await.unwrap();
        let (status, answer) = (response.status(), response.text().await.unwrap());
        assert!(status.is_success(), "{method} {path}: {status} {answer}");
        serde_json::from_str(&answer).unwrap_or_default()
    }

    /// Ends the session whose token is `token` on the server, with no
    /// word to the app that holds it.
    pub async fn end_session(&self, token: &str) {
        let logout = self.http.post(self.at("/logout")).bearer_auth(token);
        assert!(logout.send().await.unwrap().status().is_success());
    }

    /// Sends `message` to `to`, a handle with its `@`, as `from`, and
    /// returns the new exchange's id.
    pub async fn send(&self, from: &Person, to: &str, message: Value) -> String {
        let sent = self
            .call(Method::POST, &format!("/{to}"), from, message)
            .await;
        sent["id"].as_str().unwrap().to_owned()
    }

    /// The exchange `id` as `by` sees it.
    pub async fn exchange(&self, by: &Person, id: &str) -> Value {
        let path = format!("/exchanges/{id}");
        self.call(Method::GET, &path, by, Value::Null).await
    }

    /// `by`'s exchanges, sent and received, newest first.
    pub async fn exchanges(&self, by: &Person) -> Vec<Value> {
        let path = format!("/{}/exchanges", by.name);
        let listed = self.call(Method::GET, &path, by, Value::Null).await;
        listed["exchanges"].as_array().unwrap().clone()
    }

    /// Replies to exchange `id` as `by` with the choice `choice`.
    pub async fn reply(&self, by: &Person, id: &str, choice: &str) {
        let path = format!("/exchanges/{id}/reply");
        let reply = json!({ "choice": choice });
        self.call(Method::PUT, &path, by, reply).await;
    }

    /// Answers the one open request `@transom` has made of `by` with
    /// `choice`: `allow` for a sender who asks to send to them, and
    /// `join` for an invitation to a group.
    pub async fn answer_transom(&self, by: &Person, choice: &str) {
        let listed = self.exchanges(by).await;
        let asks =
            |exchange: &&Value| exchange["from"] == "@transom" && exchange["state"] == "open";
        let request = listed.iter().find(asks).unwrap()["id"].as_str().unwrap();
        self.reply(by, request, choice).await;
    }

    /// Gives `person` the picture `picture`, the bytes of a square image.
    pub async fn portray(&self, person: &Person, picture: &'static [u8]) {
        let path = self.at(&format!("/{}/picture", person.name));
        let put = self.http.put(path).bearer_auth(&person.token).body(picture);
        assert!(put.send().await.unwrap().status().is_success());
    }

    /// Has `recipient` allow `sender` to send to them, so what the sender
    /// sends next opens at once. The sender's first message is held for
    /// consent, and `@transom` asks the recipient about it, so the
    /// recipient's list keeps both, read: the first message, a
    /// notification that says Hello, and the request, replied to.
    pub async fn acquaint(&self, sender: &Person, recipient: &Person) {
        let hello = json!({ "summary": "Hello" });
        let hello = self.send(sender, &recipient.name, hello).await;
        self.answer_transom(recipient, "allow").await;
        let path = format!("/{}/read", recipient.name);
        let read = json!({ "exchanges": [hello] });
        self.call(Method::POST, &path, recipient, read).await;
    }
}
