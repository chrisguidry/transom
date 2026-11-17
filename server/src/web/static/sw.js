// transom's service worker. It shows a notification for each exchange
// that arrives, clears it when the exchange ends anywhere, is read on any
// device, or is deleted, marks the exchange read when the person dismisses
// it, and sends the reply when a notification's button is a choice.

// A new worker takes over as soon as it installs, rather than once every
// page has closed, so the push handlers change with a deploy. It keeps no
// caches and answers no fetches, so an open page loses nothing when it
// does. Claiming the pages lets a click on a notification find a page
// that opened before the worker first installed.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("push", (event) => {
  const push = event.data ? event.data.json() : null;
  if (push && push.event === "received") {
    event.waitUntil(show(push));
  } else if (push && ["ended", "read", "gone"].includes(push.event)) {
    // A read of many exchanges at once names them all in `ids`.
    event.waitUntil(clear(push.ids || [push.id]));
  }
});

// The urgency decides how a notification shows: urgent and high stay up
// until the person acts on them, and low makes no sound. The server sends
// no push for very-low, which only the inbox shows. A notification that
// takes the place of one with the same tag, as a repeat or a message with
// the same key does, alerts again unless it is silent.
async function show(push) {
  const silent = push.urgency === "low";
  return self.registration.showNotification(push.title, {
    body: push.body,
    icon: await picture(push.icon, BADGES[push.disposition]),
    tag: push.tag || push.id,
    data: push,
    requireInteraction: push.urgency === "urgent" || push.urgency === "high",
    silent,
    renotify: !silent,
    actions: push.choices.map((choice, index) => ({ action: String(index), title: choice.label })),
  });
}

// The badge of each disposition that has one, as the page draws it on the
// sender's picture: its color, from the stylesheet's light palette, since
// a worker reads no stylesheet, and its glyph, a stroke in a box of 12
// units.
const BADGES = {
  problem: { color: "#c2412d", glyph: "M6 2.9v3.9M6 9.1h.01" },
  resolved: { color: "#2f7d4f", glyph: "M3.2 6.3l1.9 1.9 3.7-4.1" },
  question: {
    color: "#3d5a73",
    glyph: "M4.3 4.7a1.75 1.75 0 1 1 2.6 1.5c-.6.35-.9.7-.9 1.2M6 9.5h.01",
  },
};

// The sender's picture, as a data: URL, with `badge` on its upper right
// when the message's disposition has one. Only those signed in may see a
// picture, and browsers differ on whether they fetch a notification's
// icon with the session's cookie, so the worker fetches it with the
// cookie itself. A picture that does not load leaves the notification
// without an icon rather than without a notification, and a browser that
// cannot draw in a worker shows the picture without its badge.
async function picture(url, badge) {
  if (!url) {
    return undefined;
  }
  try {
    const response = await fetch(url);
    if (!response.ok) {
      return undefined;
    }
    const plain = await response.blob();
    return dataUrl(badge ? await badged(plain, badge).catch(() => plain) : plain);
  } catch {
    return undefined;
  }
}

// `picture` at 192 pixels with `badge` on its upper right, in the page's
// proportions: 18 of the picture's 40 pixels across, with a ring of 2,
// which is white here, where no page gives a paper color.
async function badged(picture, badge) {
  const size = 192;
  const across = (size * 18) / 40;
  const ring = (across * 2) / 18;
  const canvas = new OffscreenCanvas(size, size);
  const context = canvas.getContext("2d");
  context.drawImage(await createImageBitmap(picture), 0, 0, size, size);
  context.translate(size - across, 0);
  context.beginPath();
  context.arc(across / 2, across / 2, across / 2, 0, 2 * Math.PI);
  context.fillStyle = "#ffffff";
  context.fill();
  context.beginPath();
  context.arc(across / 2, across / 2, across / 2 - ring, 0, 2 * Math.PI);
  context.fillStyle = badge.color;
  context.fill();
  // The glyph's box of 12 units is the badge inside its ring.
  const unit = (across - 2 * ring) / 12;
  context.translate(ring, ring);
  context.scale(unit, unit);
  context.strokeStyle = "#ffffff";
  context.lineWidth = 1.6;
  context.lineCap = "round";
  context.lineJoin = "round";
  context.stroke(new Path2D(badge.glyph));
  return canvas.convertToBlob({ type: "image/png" });
}

async function dataUrl(blob) {
  // A picture is at most 256 KiB, more than one call to fromCharCode
  // takes as arguments, so it is turned into a string in pieces.
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  }
  return `data:${blob.type};base64,${btoa(binary)}`;
}

// A notification's tag can be a key that several exchanges share, so the
// exchange it shows is found by its data instead.
async function clear(ids) {
  for (const notification of await self.registration.getNotifications()) {
    if (notification.data && ids.includes(notification.data.id)) {
      notification.close();
    }
  }
}

// Dismissing a notification reads its exchange, as swiping one away does
// on the phone. Closing one from here, as a click or a clear does, fires
// no close event, so only the person's dismissal reads it.
self.addEventListener("notificationclose", (event) => {
  const push = event.notification.data;
  event.waitUntil(
    fetch(`/exchanges/${push.id}/@${push.reader}`, {
      method: "PUT",
      headers: { "hx-request": "true" },
    }),
  );
});

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
