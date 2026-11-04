//! The MCP server at `/mcp`: tools that send, ask, check, reply, withdraw,
//! and list, for agents. Each tool calls the same functions as the HTTP
//! routes, and an MCP request signs in with the same bearer token, so an
//! agent's MCP calls and its HTTP calls are one identity.

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ProgressNotificationParam, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::{App, Failure};
use crate::exchanges::fields::{Document, Fields};
use crate::exchanges::listing::{Filters, inbox};
use crate::exchanges::{legs, sending, view, visible};
use crate::identities::records::Identity;
use crate::identities::{Resolved, caller, resolve};
use crate::signin::Verified;

/// How often a tool that is waiting tells the client it is still at work,
/// so the client's own timeout does not end the call.
const PROGRESS_EVERY: Duration = Duration::from_secs(10);

pub(crate) fn router(app: Arc<App>) -> Router<Arc<App>> {
    router_with(app, PROGRESS_EVERY)
}

/// The router, with waiting tools reporting progress every `progress_every`.
pub(crate) fn router_with(app: Arc<App>, progress_every: Duration) -> Router<Arc<App>> {
    let tools_app = app.clone();
    // Every request to /mcp carries a bearer token, which a page rebinding
    // its DNS to transom cannot attach, so the Host allowlist that guards
    // a local server against that adds nothing here.
    let config = StreamableHttpServerConfig::default().disable_allowed_hosts();
    let service = StreamableHttpService::new(
        move || Ok(Tools::new(tools_app.clone(), progress_every)),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(app, sign_in))
}

/// Signs in the request's bearer token, as every HTTP route does, and
/// hands the caller to the tools.
async fn sign_in(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let identity = match Verified::from_request_parts(&mut parts, &app).await {
        Ok(verified) => caller(&app, &verified).await,
        Err(failure) => Err(failure),
    };
    match identity {
        Ok(identity) => {
            parts.extensions.insert(identity);
            next.run(Request::from_parts(parts, body)).await
        }
        Err(failure) => failure.into_response(),
    }
}

#[derive(Clone)]
pub(crate) struct Tools {
    app: Arc<App>,
    progress_every: Duration,
}

fn caller_of(parts: &Parts) -> Result<Identity, ErrorData> {
    parts
        .extensions
        .get::<Identity>()
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("the request was not signed in", None))
}

/// A tool's result: the JSON it found, or what went wrong, as text the
/// model reads either way.
fn result(outcome: Result<serde_json::Value, Failure>) -> CallToolResult {
    match outcome {
        Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
        Err(failure) => CallToolResult::error(vec![ContentBlock::text(failure.to_string())]),
    }
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct Ask {
    /// Who to ask: a name such as @chris or @household.
    to: String,
    /// How many seconds to wait for the reply before returning. Without it,
    /// the tool returns at once, and the reply reaches your webhook.
    wait: Option<u64>,
    #[serde(flatten)]
    message: Document,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct Check {
    /// The exchange's id.
    exchange: String,
    /// How many seconds to wait for it to end before returning.
    wait: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ReplyTo {
    /// The exchange's id.
    exchange: String,
    /// The id or label of one of its choices.
    choice: Option<String>,
    /// A typed reply.
    text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct Withdraw {
    /// The exchange's id.
    exchange: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct Inbox {
    /// Only exchanges in this state: pending, open, replied, expired,
    /// withdrawn, or declined.
    state: Option<String>,
    /// Only questions, or only notifications.
    question: Option<bool>,
    /// Only what you have, or have not, read.
    read: Option<bool>,
    /// How many to return, newest first: 50 unless you say.
    limit: Option<usize>,
}

impl Tools {
    async fn send(
        &self,
        caller: &Identity,
        to: &str,
        document: Document,
    ) -> Result<crate::exchanges::records::Exchange, Failure> {
        let recipient = match resolve(&self.app, to).await? {
            Resolved::Current(identity) => identity,
            Resolved::Moved(_) => {
                return Err(Failure::new(
                    axum::http::StatusCode::NOT_FOUND,
                    format!("{to} has a new name"),
                ));
            }
        };
        let message = Fields::try_from(document)
            .and_then(Fields::into_message)
            .map_err(|reason| Failure::new(axum::http::StatusCode::BAD_REQUEST, reason))?;
        sending::deliver(&self.app, caller, &recipient, message).await
    }

    /// Waits up to `wait` for the exchange to end, telling the client it is
    /// still waiting as it goes.
    async fn wait_for_end(
        &self,
        id: &str,
        viewer: &str,
        wait: Duration,
        context: &RequestContext<RoleServer>,
    ) -> Result<serde_json::Value, Failure> {
        let token = context.meta.get_progress_token();
        let mut waited = Duration::ZERO;
        while waited < wait {
            let step = self.progress_every.min(wait.saturating_sub(waited));
            if let Some(exchange) = sending::until_ended(&self.app, id, step).await? {
                return view(&self.app, &exchange, viewer).await;
            }
            waited += step;
            if let Some(token) = token.clone() {
                let progress = ProgressNotificationParam::new(token, waited.as_secs_f64())
                    .with_total(wait.as_secs_f64())
                    .with_message("waiting for a reply");
                let _ = context.peer.notify_progress(progress).await;
            }
        }
        view(&self.app, &visible(&self.app, id, viewer).await?, viewer).await
    }
}

#[tool_router]
impl Tools {
    pub(crate) fn new(app: Arc<App>, progress_every: Duration) -> Self {
        Self {
            app,
            progress_every,
        }
    }

    #[tool(
        description = "Ask someone a question and, if you like, wait for the reply. \
        Offer choices, a prompt for a typed answer, or both. Ask one thing at a time, and put \
        what the person needs to decide in the body. Use this to interrupt someone for a decision; \
        use notify to inform them."
    )]
    async fn ask(
        &self,
        Parameters(ask): Parameters<Ask>,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let outcome = async {
            let exchange = self.send(&caller, &ask.to, ask.message).await?;
            match ask.wait {
                Some(seconds) => {
                    self.wait_for_end(
                        &exchange.id,
                        &caller.id,
                        Duration::from_secs(seconds),
                        &context,
                    )
                    .await
                }
                None => view(&self.app, &exchange, &caller.id).await,
            }
        };
        Ok(result(outcome.await))
    }

    #[tool(
        description = "Tell someone something that needs no answer, such as a status \
        update or a finished task. They can still reply if they want to."
    )]
    async fn notify(
        &self,
        Parameters(ask): Parameters<Ask>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let outcome = async {
            let exchange = self.send(&caller, &ask.to, ask.message).await?;
            view(&self.app, &exchange, &caller.id).await
        };
        Ok(result(outcome.await))
    }

    #[tool(
        description = "Check an exchange's state and reply, and, if you like, wait for it to end."
    )]
    async fn check(
        &self,
        Parameters(check): Parameters<Check>,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let outcome = async {
            let exchange = visible(&self.app, &check.exchange, &caller.id).await?;
            match check.wait {
                Some(seconds) if !exchange.state.is_final() => {
                    self.wait_for_end(
                        &exchange.id,
                        &caller.id,
                        Duration::from_secs(seconds),
                        &context,
                    )
                    .await
                }
                _ => view(&self.app, &exchange, &caller.id).await,
            }
        };
        Ok(result(outcome.await))
    }

    #[tool(
        description = "Reply to an exchange in your inbox, with one of its choices, text, or both."
    )]
    async fn reply(
        &self,
        Parameters(reply): Parameters<ReplyTo>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let outcome = async {
            let exchange = legs::reply_as(
                &self.app,
                &caller.id,
                &reply.exchange,
                reply.choice,
                reply.text,
                false,
            )
            .await?;
            view(&self.app, &exchange, &caller.id).await
        };
        Ok(result(outcome.await))
    }

    #[tool(description = "Take back a question you asked that no longer needs an answer.")]
    async fn withdraw(
        &self,
        Parameters(withdraw): Parameters<Withdraw>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let outcome = async {
            let exchange = legs::withdraw_as(&self.app, &caller.id, &withdraw.exchange).await?;
            view(&self.app, &exchange, &caller.id).await
        };
        Ok(result(outcome.await))
    }

    #[tool(description = "List your exchanges, sent and received, newest first.")]
    async fn inbox(
        &self,
        Parameters(filters): Parameters<Inbox>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let filters = Filters {
            state: filters.state,
            read: filters.read,
            question: filters.question,
            limit: filters.limit,
        };
        let outcome = async {
            Ok(serde_json::json!({ "exchanges": inbox(&self.app, &caller, &filters).await? }))
        };
        Ok(result(outcome.await))
    }
}

// rmcp's macro writes async trait methods that need no await.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler]
impl ServerHandler for Tools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "transom lets you ask a person, or another agent, a question and get the answer \
             back, and tell them things that need no answer. Ask when you need a decision you \
             cannot make yourself; put what they need to decide in the body.",
        )
    }
}
