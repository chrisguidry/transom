// transom's service worker. It shows a notification for each exchange
// that arrives, clears it when the exchange ends anywhere or is read on
// any device, and sends the reply when a notification's button is a
// choice.

self.addEventListener("push", (event) => {
  const push = event.data ? event.data.json() : null;
  if (push && push.event === "received") {
    event.waitUntil(show(push));
  } else if (push && (push.event === "ended" || push.event === "read")) {
    event.waitUntil(clear(push.id));
  }
});

// The urgency decides how a notification shows: urgent and high stay up
// until the person acts on them, low makes no sound, and very-low shows
// nothing, since the inbox holds it. A notification that takes the place
// of one with the same tag, as a repeat or a message with the same key
// does, alerts again unless it is silent.
function show(push) {
  if (push.urgency === "very-low") {
    return Promise.resolve();
  }
  const silent = push.urgency === "low";
  return self.registration.showNotification(push.title, {
    body: push.body,
    tag: push.tag || push.id,
    data: push,
    requireInteraction: push.urgency === "urgent" || push.urgency === "high",
    silent,
    renotify: !silent,
    actions: push.choices.map((choice, index) => ({ action: String(index), title: choice.label })),
  });
}

// A notification's tag can be a key that several exchanges share, so the
// exchange it shows is found by its data instead.
async function clear(id) {
  for (const notification of await self.registration.getNotifications()) {
    if (notification.data && notification.data.id === id) {
      notification.close();
    }
  }
}

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const push = event.notification.data;
  const choice = event.action ? push.choices[Number(event.action)] : null;
  event.waitUntil(choice ? answer(push, choice) : open(push));
});

// A choice is sent as the page sends it, with the session's cookie. If it
// is refused, as when the session has ended, the exchange opens instead.
async function answer(push, choice) {
  const response = await fetch(`/exchanges/${push.id}/reply`, {
    method: "PUT",
    headers: { "content-type": "application/json", "hx-request": "true" },
    body: JSON.stringify({ choice: choice.key }),
  });
  if (!response.ok) {
    await open(push);
  }
}

async function open(push) {
  const path = `/exchanges/${push.id}`;
  for (const client of await self.clients.matchAll({ type: "window" })) {
    if (new URL(client.url).pathname === path) {
      return client.focus();
    }
  }
  return self.clients.openWindow(path);
}
