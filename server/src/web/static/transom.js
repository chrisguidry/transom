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
