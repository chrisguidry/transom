// Push notifications in the browser: the bar's toggle that subscribes this
// browser as one of the person's devices, and the devices page's mark on
// this browser's own. The service worker, sw.js, shows the notifications.

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

// The browser's subscription alone does not say notifications are on: the
// server forgets the device when the session that registered it ends,
// and the push that says so may not have arrived. So the toggle is on
// only while the server still lists this browser's device.
//
// The server attaches a device to the session that registered it, and
// tells the device when that session ends, so a browser that is on
// registers its device again on each load, under the session it has now.
async function offerPush(button) {
  const devices = button.dataset.devices;
  const registration = await navigator.serviceWorker.register("/sw.js");
  const show = (on) => {
    label(button, on ? "Notifications on" : "Notifications off");
    button.setAttribute("aria-pressed", on ? "true" : "false");
    button.hidden = false;
  };
  const isOn = async (subscription) => subscription !== null && (await listed(devices));
  button.addEventListener("click", async () => {
    button.disabled = true;
    try {
      const current = await registration.pushManager.getSubscription();
      show((await isOn(current))
        ? await unsubscribe(devices, current)
        : await subscribe(devices, registration));
    } catch (error) {
      unavailable(button, error);
    } finally {
      button.disabled = false;
    }
  });
  // The click above works even when this first look fails, as when the
  // server cannot be reached as the page loads.
  try {
    const current = await registration.pushManager.getSubscription();
    const on = await isOn(current);
    if (on) await register(devices, current);
    show(on);
  } catch (error) {
    unavailable(button, error);
  }
}

// Signing out takes this browser's device off first. Otherwise the end of
// the session would push this browser a notification that it was signed
// out, to the person who just chose to. Removing a browser's device
// leaves its session alone, so the sign-out still has a session to end.
document.addEventListener("htmx:confirm", (event) => {
  if (event.detail.path !== "/logout" || !toggle || !("serviceWorker" in navigator)) return;
  event.preventDefault();
  forgetThisBrowser(toggle.dataset.devices)
    .catch(() => {})
    .finally(() => event.detail.issueRequest(true));
});

async function forgetThisBrowser(devices) {
  const registration = await navigator.serviceWorker.getRegistration();
  const subscription = await registration?.pushManager.getSubscription();
  if (subscription) await unsubscribe(devices, subscription);
}

// Unsafe requests carry HX-Request, as htmx's do, which the session's
// cookie needs.
const sending = { "content-type": "application/json", "hx-request": "true" };

// Whether the server lists this browser's device among the person's.
async function listed(devices) {
  const own = localStorage.getItem("transom-device");
  const response = await fetch(devices, { headers: { accept: "application/json" } });
  if (!response.ok) return false;
  return (await response.json()).devices.some((device) => device.id === own);
}

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
  try {
    await register(devices, subscription);
  } catch (error) {
    await subscription.unsubscribe();
    throw error;
  }
  return true;
}

// Puts this browser's device, with `subscription`, as one of the person's.
async function register(devices, subscription) {
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
    throw new Error(`transom refused the subscription: ${response.status}`);
  }
}

async function unsubscribe(devices, subscription) {
  await fetch(`${devices}/${deviceId()}`, { method: "DELETE", headers: sending });
  await subscription.unsubscribe();
  return false;
}
