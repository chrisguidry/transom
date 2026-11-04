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
