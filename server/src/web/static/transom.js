// The one script of transom's own: what htmx attributes cannot say.
// Inline scripts are not allowed by the page's Content Security Policy,
// so this file is all there is.

// After an action that changes the page, such as removing a member, the
// page shows the result by loading itself again.
document.addEventListener("htmx:afterRequest", (event) => {
  if (event.detail.successful && event.detail.elt.hasAttribute("data-reload")) {
    window.location.reload();
  }
});

// A deploy changes what the server serves while pages stay open, so a
// page compares the version it was rendered with to the server's, and
// loads itself again for a newer one. It checks when it loads, when its
// event stream opens, which it does again after the server behind it
// stops, and when the person comes back to it. During a rolling deploy a
// page hears from servers of both versions, so it reloads only for a
// server that started after the one that rendered it, which keeps it
// from going back to the version it left. It reloads at most three times
// for one version, so a page that keeps landing on an older server stops
// trying rather than reloading for as long as both versions run.
const rendered = document.querySelector('meta[name="transom-version"]');
let checking = null;
let reloading = false;

// A server that cannot be reached is asked again at the next check.
function checkVersion() {
  checking ??= compareVersions().catch(() => {}).finally(() => { checking = null; });
}

async function compareVersions() {
  if (!rendered || reloading) return;
  const response = await fetch("/version", { cache: "no-store" });
  if (!response.ok) return;
  const server = await response.json();
  if (server.version === rendered.content || server.started <= Number(rendered.dataset.started)) {
    return;
  }
  const reloads = JSON.parse(sessionStorage.getItem("transom-reloads") || "{}");
  const count = reloads[server.version] || 0;
  if (count >= 3) return;
  reloads[server.version] = count + 1;
  sessionStorage.setItem("transom-reloads", JSON.stringify(reloads));
  reloading = true;
  // The newer server may serve a newer service worker, which takes over
  // the push notifications as soon as it installs.
  navigator.serviceWorker?.getRegistration().then((registration) => registration?.update());
  reloadWhenIdle();
}

// A reload would lose what the person is typing, so it waits while a
// text box has focus or holds text, unless the page is hidden. It would
// also lose a reply that is not sent yet, so it waits for that reply to
// be sent, whether the page is hidden or not.
function reloadWhenIdle() {
  const typing = [...document.querySelectorAll("textarea, input:not([type=hidden])")]
    .some((field) => field === document.activeElement || field.value.trim());
  if (reloading && unsent.size === 0 && (document.visibilityState === "hidden" || !typing)) {
    window.location.reload();
  }
}

document.addEventListener("htmx:sseOpen", checkVersion);
window.addEventListener("focus", checkVersion);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") checkVersion();
});
// Focus has moved on only after focusout ends, so that retry waits a turn.
for (const name of ["visibilitychange", "focusout", "input", "htmx:afterSettle"]) {
  document.addEventListener(name, () => setTimeout(reloadWhenIdle));
}
checkVersion();

// The server writes each time in UTC, since it cannot know the reader's
// time zone, so the page says it again in the browser's own: a row's
// time as how long ago, and a page's as the day and the time, with the
// full local time as its tooltip. The rules match the server's own.
function ago(at, now) {
  const seconds = (now - at) / 1000;
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  const day = (date) => new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const days = Math.round((day(now) - day(at)) / 86400000);
  if (days === 0) return `${Math.floor(seconds / 3600)}h`;
  if (days === 1) return "Yesterday";
  if (days < 7) return at.toLocaleDateString(undefined, { weekday: "short" });
  const year = at.getFullYear() === now.getFullYear() ? undefined : "numeric";
  return at.toLocaleDateString(undefined, { month: "short", day: "numeric", year });
}

function localTimes() {
  for (const time of document.querySelectorAll("time[datetime]")) {
    const at = new Date(time.dateTime);
    if (Number.isNaN(at.getTime())) continue;
    time.title = at.toLocaleString(undefined, { dateStyle: "full", timeStyle: "short" });
    time.textContent = time.classList.contains("long")
      ? `${at.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })} at ${at.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`
      : ago(at, new Date());
  }
}
localTimes();
document.addEventListener("htmx:afterSettle", localTimes);

// After Mark read or Mark all read, a toast says what was marked and
// offers Undo for six seconds, which marks each of them unread again.
// The server answers which exchanges it marked, so Undo leaves alone any
// that were read already.
const toast = document.querySelector(".toast");
let undoing = [];
let hiding;

// Shows the toast for six seconds, with Undo only when there is something
// to undo.
function say(text, undoable) {
  toast.querySelector(".toast-text").textContent = text;
  toast.querySelector(".undo").hidden = !undoable;
  toast.hidden = false;
  clearTimeout(hiding);
  hiding = setTimeout(() => { toast.hidden = true; }, 6000);
}

document.addEventListener("htmx:afterRequest", (event) => {
  if (!toast || !event.detail.successful || !event.detail.elt.hasAttribute("data-undo")) {
    return;
  }
  undoing = JSON.parse(event.detail.xhr.responseText).read;
  if (undoing.length === 0) {
    return;
  }
  say(undoing.length === 1 ? "Marked read" : `Marked ${undoing.length} read`, true);
});
toast?.querySelector(".undo").addEventListener("click", (event) => {
  const reader = event.target.dataset.reader;
  for (const id of undoing) {
    htmx.ajax("DELETE", `/exchanges/${id}/@${reader}`, { swap: "none" });
  }
  undoing = [];
  toast.hidden = true;
});

// An invitation's URL names the invitee, which a form's fields cannot
// put in a path, so the form builds the URL here and sends a PUT.
document.addEventListener("submit", (event) => {
  const form = event.target.closest("form[data-invite]");
  if (!form) {
    return;
  }
  event.preventDefault();
  const name = form.elements.name.value.trim().replace(/^@/, "");
  if (name) {
    htmx.ajax("PUT", form.dataset.invite + encodeURIComponent(name), { swap: "none" })
      .then(() => window.location.reload());
  }
});

// An answer's Reply sends only the text, so it is enabled only while
// there is text to send. The choice buttons need none.
document.addEventListener("input", (event) => {
  const form = event.target.closest("form.answer");
  if (form) {
    form.querySelector("[data-needs-text]").disabled = !event.target.value.trim();
  }
});

// Two kinds of reply are this person's and not the exchange's: an unsent
// one, whose request did not reach the server, and a lost one, which came
// after someone else's and which the server refused with 409 and the
// reply that won. An exchange's page shows each where the reply would
// be, and a list shows each on its exchange's row. They are kept here, by
// exchange id, and drawn again after each of the page's live redraws,
// which replace the elements that showed them. Loading the page again
// forgets them, and nothing sends a reply after the page is closed.
const unsent = new Map();
const lost = new Map();
const REPLY = /^\/exchanges\/([^/]+)\/reply$/;

// The reply that a request sends, if it sends one: its exchange's id,
// the choice and the text as the form sent them, and what it said, which
// is the choice's label, or the text when there is no choice.
function replyIn(request) {
  const id = request.verb.toLowerCase() === "put" && REPLY.exec(request.path)?.[1];
  if (!id) return null;
  if (unsent.has(id)) return unsent.get(id);
  const choice = request.formData.get("choice");
  const text = request.formData.get("text");
  const button = [...request.elt.querySelectorAll('button[name="choice"]')]
    .find((offered) => offered.value === choice);
  const said = button ? button.textContent : choice ?? (text ?? "").trim();
  return { id, choice, text, said, sending: false };
}

// A server that accepts the connection and then sends nothing would keep
// a reply waiting for as long as the browser allows, so a reply's request
// ends after 30 seconds, as the desktop app's does.
document.addEventListener("htmx:configRequest", (event) => {
  if (replyIn(event.detail)) event.detail.timeout = 30000;
});

// A request that got no response has a status of 0, whether it failed
// to connect, ran out of time, or the connection closed with nothing on
// it, which a browser may report as a response and not as an error.
//
// A reply that the server answered is no longer unsent, whether it was
// used or not. The page's event stream may have closed while the server
// could not be reached, and a stream that opens again does not repeat
// what it missed, so the page then loads its exchange or its list again,
// as an event from the stream would have made it do.
document.addEventListener("htmx:afterRequest", (event) => {
  const reply = replyIn(event.detail.requestConfig);
  if (!reply) return;
  const xhr = event.detail.xhr;
  if (xhr.status === 0) {
    unsent.set(reply.id, { ...reply, sending: false });
    showReplies();
    return;
  }
  const winner = xhr.status === 409 && JSON.parse(xhr.responseText).reply;
  if (!event.detail.successful && !winner) return;
  const wasUnsent = unsent.delete(reply.id);
  if (winner) {
    lost.set(reply.id, { said: reply.said, winner: winner.by_name ?? winner.by });
    // A list has the toast, since the row may be out of sight.
    if (toast) say(notUsed(lost.get(reply.id)), false);
  }
  showReplies();
  if (wasUnsent || winner) htmx.trigger("#exchange, #inbox", "sse:exchange");
});

function notUsed(reply) {
  return `Your answer, ${reply.said}, was not used: ${reply.winner} answered first`;
}

// Send again sends the same reply. Its request comes from the body, which
// no redraw replaces, so the request's events reach this script even when
// the page redraws while the reply is on its way.
document.addEventListener("click", (event) => {
  const reply = unsent.get(event.target.closest("[data-send-again]")?.dataset.sendAgain);
  if (!reply || reply.sending) return;
  unsent.set(reply.id, { ...reply, sending: true });
  showReplies();
  const values = {};
  for (const name of ["choice", "text"]) {
    if (reply[name] !== null) values[name] = reply[name];
  }
  // A request that fails rejects, and the listener above shows that.
  htmx.ajax("PUT", `/exchanges/${reply.id}/reply`, { values, swap: "none" }).catch(() => {});
});

function element(tag, className, text) {
  const made = document.createElement(tag);
  made.className = className;
  made.textContent = text;
  return made;
}

// A reply of this person's as its exchange's page shows it: its state,
// which the stylesheet puts in small capitals, what it said, and a note
// on why it is not the exchange's reply.
function section(kind, state, said, note) {
  const made = element("section", `reply ${kind}`, "");
  made.append(
    element("p", `state ${kind}`, state),
    element("p", "chosen", said),
    element("p", "meta", note),
  );
  return made;
}

// Puts `shown` above `anchor`, or at the end of `container` when there
// is no anchor, until the next `showReplies`.
function place(shown, anchor, container) {
  shown.dataset.mine = "";
  if (anchor) anchor.before(shown);
  else container.append(shown);
}

// Puts `shown` where `covered` is and hides `covered`, until the next
// `showReplies`.
function cover(covered, shown) {
  covered.hidden = true;
  covered.dataset.covered = "";
  place(shown, covered);
}

// Draws every unsent and lost reply on the page as it is now, in place of
// what an earlier call drew. An unsent reply takes the place of its form,
// so the person cannot answer again over an answer that is waiting, and a
// lost one goes above the reply that won. A row says where the reply
// stands in place of where its exchange stands.
function showReplies() {
  for (const drawn of document.querySelectorAll("[data-mine]")) drawn.remove();
  for (const covered of document.querySelectorAll("[data-covered]")) {
    covered.hidden = false;
    delete covered.dataset.covered;
  }
  const page = (id) => document.querySelector(`#exchange[hx-get="/exchanges/${CSS.escape(id)}"]`);
  const row = (id) => document.querySelector(`.card > a[href="/exchanges/${CSS.escape(id)}"]`);
  for (const [id, reply] of unsent) {
    const again = element("button", "", "Send again");
    again.type = "button";
    again.dataset.sendAgain = id;
    again.disabled = reply.sending;
    const exchange = page(id);
    if (exchange) {
      const note = "Your answer is sent when the server can be reached.";
      const shown = section("unsent", "Not sent yet", reply.said, note);
      shown.append(again);
      const form = exchange.querySelector("form.answer");
      if (form) cover(form, shown);
      else place(shown, exchange.querySelector("section.reply"), exchange);
    }
    const link = row(id);
    if (link) {
      const says = `Your answer, ${reply.said}, is not sent yet`;
      cover(link.querySelector(".meta"), element("span", "meta unsent", says));
      const buttons = element("div", "row-choices", "");
      buttons.append(again);
      const choices = link.parentElement.querySelector("form.row-choices");
      if (choices) cover(choices, buttons);
      else place(buttons, link.nextElementSibling, link.parentElement);
    }
  }
  for (const [id, reply] of lost) {
    const exchange = page(id);
    if (exchange) {
      const shown = section("lost", "Not used", reply.said, notUsed(reply));
      place(shown, exchange.querySelector("section.reply, form.answer"), exchange);
    }
    const link = row(id);
    if (link) cover(link.querySelector(".meta"), element("span", "meta lost", notUsed(reply)));
  }
}
document.addEventListener("htmx:afterSettle", showReplies);

// The devices page marks this browser's own device, by the id the browser
// keeps for itself, which only the browser knows.
const own = localStorage.getItem("transom-device");
const mine = own && document.querySelector(`[data-device="${CSS.escape(own)}"] [data-this-browser]`);
if (mine) {
  mine.hidden = false;
}

// Push notifications. A browser that can receive them shows a toggle in
// the bar that subscribes it as one of the person's devices, or
// unsubscribes it. The service worker shows the notifications.
const toggle = document.querySelector("[data-devices]");
if (toggle && "serviceWorker" in navigator && "PushManager" in window) {
  offerPush(toggle).catch((error) => unavailable(toggle, error));
}

// A browser can refuse push for reasons a page cannot detect first, such
// as a private window or push turned off in its settings, so a failure
// says so in the toggle's label rather than leaving it as it was.
function unavailable(button, error) {
  console.error("transom: notifications are unavailable", error);
  label(button, `Notifications unavailable: ${error.message}`);
  button.setAttribute("aria-pressed", "false");
  button.hidden = false;
}

// The toggle is an icon, so its words are its label and its tooltip.
function label(button, text) {
  button.setAttribute("aria-label", text);
  button.title = text;
}

async function offerPush(button) {
  const devices = button.dataset.devices;
  const registration = await navigator.serviceWorker.register("/sw.js");
  const show = (subscription) => {
    label(button, subscription ? "Notifications on" : "Notifications off");
    button.setAttribute("aria-pressed", subscription ? "true" : "false");
    button.hidden = false;
  };
  show(await registration.pushManager.getSubscription());
  button.addEventListener("click", async () => {
    button.disabled = true;
    try {
      const current = await registration.pushManager.getSubscription();
      show(current ? await unsubscribe(devices, current) : await subscribe(devices, registration));
    } catch (error) {
      unavailable(button, error);
    } finally {
      button.disabled = false;
    }
  });
}

// Unsafe requests carry HX-Request, as htmx's do, which the session's
// cookie needs.
const sending = { "content-type": "application/json", "hx-request": "true" };

// A browser makes up its device id once, and keeps it.
function deviceId() {
  let id = localStorage.getItem("transom-device");
  if (!id) {
    id = crypto.randomUUID();
    localStorage.setItem("transom-device", id);
  }
  return id;
}

// The key as bytes: every browser takes those, and not all take base64.
function bytes(base64url) {
  const base64 = base64url.replace(/-/g, "+").replace(/_/g, "/");
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

async function subscribe(devices, registration) {
  const { key } = await (await fetch(devices)).json();
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: bytes(key),
  });
  const platform = navigator.userAgentData ? navigator.userAgentData.platform : "";
  const response = await fetch(`${devices}/${deviceId()}`, {
    method: "PUT",
    headers: sending,
    body: JSON.stringify({
      name: platform ? `A browser on ${platform}` : "A browser",
      wake: { web_push: subscription.toJSON() },
    }),
  });
  if (!response.ok) {
    await subscription.unsubscribe();
    throw new Error(`transom refused the subscription: ${response.status}`);
  }
  return subscription;
}

async function unsubscribe(devices, subscription) {
  await fetch(`${devices}/${deviceId()}`, { method: "DELETE", headers: sending });
  await subscription.unsubscribe();
  return null;
}
