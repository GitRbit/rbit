// Passkeys (WebAuthn) for two-factor authentication. Every form still works without this
// file with a code; passkey controls stay hidden unless the browser supports passkeys.
//
// * `<form data-passkey-create="{options}">`: submitting runs `navigator.credentials.create`
//   and posts the answer in the form's hidden `credential` field.
// * `<button data-passkey-get="{options}">` inside a form: runs `navigator.credentials.get`
//   and posts the answer in the form's hidden `passkey` field. A `password` field in the
//   same form must be filled first.
// * `[data-passkey-only]` is revealed, and `[data-passkey-unsupported]` hidden, when
//   passkeys are available. `[data-passkey-error]` in the form shows what went wrong.
//
// Options come from the server as JSON with binary fields in base64url; answers go back in
// the shape of `PublicKeyCredential.toJSON()`.

const supported = typeof window.PublicKeyCredential === "function" && !!navigator.credentials;

function fromBase64url(value) {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64 + "=".repeat((4 - (base64.length % 4)) % 4));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0)).buffer;
}

function toBase64url(buffer) {
  let binary = "";
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function withBinaryIds(list) {
  return (list ?? []).map((descriptor) => ({ ...descriptor, id: fromBase64url(descriptor.id) }));
}

function creationOptions(json) {
  if (PublicKeyCredential.parseCreationOptionsFromJSON) return PublicKeyCredential.parseCreationOptionsFromJSON(json);
  return {
    ...json,
    challenge: fromBase64url(json.challenge),
    user: { ...json.user, id: fromBase64url(json.user.id) },
    excludeCredentials: withBinaryIds(json.excludeCredentials),
  };
}

function requestOptions(json) {
  if (PublicKeyCredential.parseRequestOptionsFromJSON) return PublicKeyCredential.parseRequestOptionsFromJSON(json);
  return { ...json, challenge: fromBase64url(json.challenge), allowCredentials: withBinaryIds(json.allowCredentials) };
}

function credentialJson(credential) {
  const r = credential.response;
  const response = { clientDataJSON: toBase64url(r.clientDataJSON) };
  if (r.attestationObject) {
    response.attestationObject = toBase64url(r.attestationObject);
    response.transports = r.getTransports?.() ?? [];
  } else {
    response.authenticatorData = toBase64url(r.authenticatorData);
    response.signature = toBase64url(r.signature);
    if (r.userHandle) response.userHandle = toBase64url(r.userHandle);
  }
  return JSON.stringify({ id: credential.id, rawId: toBase64url(credential.rawId), type: credential.type, response });
}

function message(error, creating) {
  switch (error?.name) {
    case "NotAllowedError":
      return (globalThis.rbitMessages?.text("browser-passkey-cancelled") ?? "The passkey request was cancelled or timed out. Try again.");
    case "InvalidStateError":
      return creating ? (globalThis.rbitMessages?.text("browser-passkey-registered") ?? "That passkey is already registered to your account.") : (globalThis.rbitMessages?.text("browser-passkey-unknown") ?? "That passkey isn't registered here.");
    case "SecurityError":
      return (globalThis.rbitMessages?.text("browser-passkey-address") ?? "Passkeys don't work at this address. Open the site at its configured URL.");
    default:
      return (globalThis.rbitMessages?.text("browser-passkey-failed") ?? "Your browser couldn't use a passkey. Try again or use a code.");
  }
}

function showError(form, text) {
  const target = form.querySelector("[data-passkey-error]");
  if (!target) return;
  target.textContent = text;
  target.hidden = false;
}

let busy = false;

async function ceremony(form, run, field, creating) {
  if (busy) return;
  busy = true;
  const controls = form.querySelectorAll("button");
  controls.forEach((button) => (button.disabled = true));
  try {
    const credential = await run();
    if (!credential) throw new Error("no credential");
    form.elements[field].value = credentialJson(credential);
    form.submit();
  } catch (error) {
    showError(form, message(error, creating));
    controls.forEach((button) => (button.disabled = false));
  } finally {
    busy = false;
  }
}

function reveal(root) {
  if (!supported || !(root instanceof Element || root instanceof Document)) return;
  for (const element of root.querySelectorAll("[data-passkey-only]")) element.hidden = false;
  for (const element of root.querySelectorAll("[data-passkey-unsupported]")) element.hidden = true;
}

if (supported) {
  // Capture phase, so this runs before htmx's boosted-form handler.
  document.addEventListener(
    "submit",
    (event) => {
      const form = event.target;
      if (!(form instanceof HTMLFormElement) || !form.dataset.passkeyCreate) return;
      event.preventDefault();
      event.stopPropagation();
      const options = creationOptions(JSON.parse(form.dataset.passkeyCreate));
      ceremony(form, () => navigator.credentials.create({ publicKey: options }), "credential", true);
    },
    true,
  );

  document.addEventListener("click", (event) => {
    const button = event.target instanceof Element ? event.target.closest("[data-passkey-get]") : null;
    const form = button?.form;
    if (!form) return;
    event.preventDefault();
    const password = form.elements.password;
    if (password && !password.value) {
      showError(form, (globalThis.rbitMessages?.text("browser-passkey-password") ?? "Enter your password first."));
      password.focus();
      return;
    }
    const options = requestOptions(JSON.parse(button.dataset.passkeyGet));
    ceremony(form, () => navigator.credentials.get({ publicKey: options }), "passkey", false);
  });

  reveal(document);
  document.addEventListener("htmx:load", (event) => reveal(event.target));
}
