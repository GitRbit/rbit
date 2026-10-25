// Replies remain plain Markdown, so links and quotes survive edits and exports.
let pending = null;
let timer = 0;
let revision = 0;
function mentionAt(text, caret) {
  const match = /(?:^|\s)([@#])([\p{L}\p{N}_.-]{0,64})$/u.exec(text.slice(0, caret));
  return match ? { start: caret - match[1].length - match[2].length, end: caret, accounts: match[1] === '@', query: match[2] } : null;
}
function quoteReply(author, anchor, text) {
  const quoted = text.slice(0, 4000).split(/\r?\n/).map(line => `> ${line}`).join('\n');
  return `[Replying${author ? ` to @${author}` : ''}](#${anchor})\n\n${quoted}\n\n`;
}
function editor() { return document.querySelector('[data-discussion-editor]'); }
function listFor(input) { return input.form.querySelector('[role=listbox]'); }
function cancel() { clearTimeout(timer); pending?.abort(); pending = null; revision += 1; }
function close(input) {
  listFor(input).hidden = true;
  input.removeAttribute('aria-activedescendant');
}
function enhance() {
  if (!editor()) return;
  for (const button of document.querySelectorAll('[data-reply]')) button.hidden = false;
}
function insert(input, option) {
  const mention = mentionAt(input.value, input.selectionStart);
  if (!mention) return;
  const value = `${option.dataset.mention} `;
  if (input.value.length - (mention.end - mention.start) + value.length > input.maxLength) {
    input.form.querySelector('[data-discussion-status]').textContent = 'This mention would exceed the comment limit. Shorten your draft first.';
    cancel(); close(input); return;
  }
  input.setRangeText(value, mention.start, mention.end, 'end');
  input.focus(); cancel(); close(input);
}
async function suggestions(input, version) {
  const mention = mentionAt(input.value, input.selectionStart);
  if (!mention) return;
  const controller = new AbortController(); pending = controller;
  try {
    const response = await fetch(`${input.dataset.suggestions}?${new URLSearchParams({ q: mention.query, accounts: String(mention.accounts) })}`, { signal: controller.signal, headers: { Accept: 'application/json' } });
    if (!response.ok) return;
    const choices = await response.json();
    if (version !== revision || !input.isConnected) return;
    const list = listFor(input);
    list.replaceChildren();
    for (const [index, [value, label]] of choices.slice(0, 10).entries()) {
      const button = document.createElement('button');
      button.type = 'button'; button.className = 'menu-item';
      button.id = `discussion-choice-${index}`;
      button.setAttribute('role', 'option'); button.setAttribute('aria-selected', 'false');
      button.dataset.mention = value; button.textContent = `${value} · ${label}`;
      list.append(button);
    }
    list.hidden = !choices.length;
  } catch { /* Typed mentions still work when search fails. */ }
}
document.addEventListener('click', event => {
  const reply = event.target.closest?.('[data-reply]');
  const input = editor();
  if (reply && input) {
    const text = reply.closest('article').querySelector('[data-reply-source]').value;
    const quote = quoteReply(reply.dataset.replyAuthor, reply.dataset.reply, text);
    if (input.value.length + quote.length + 2 > input.maxLength) {
      input.form.querySelector('[data-discussion-status]').textContent = 'This reply would exceed the comment limit. Shorten your draft first.';
      return;
    }
    input.value += `${input.value ? '\n\n' : ''}${quote}`;
    input.focus(); input.setSelectionRange(input.value.length, input.value.length);
    input.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }
  const option = event.target.closest?.('[data-mention]');
  if (option && input) insert(input, option);
});
document.addEventListener('input', event => {
  if (!event.target.matches?.('[data-discussion-editor]')) return;
  const input = event.target;
  cancel(); close(input);
  const version = revision;
  if (mentionAt(input.value, input.selectionStart)) timer = setTimeout(() => suggestions(input, version), 150);
});
document.addEventListener('focusin', event => {
  const input = editor();
  // Keep option buttons available for click selection, but close suggestions when
  // the user moves to Preview or another control. Cancelling also invalidates a
  // lookup that resolves after the source editor has lost focus.
  if (!input || event.target === input || listFor(input).contains(event.target)) return;
  cancel(); close(input);
});
document.addEventListener('keydown', event => {
  const input = event.target;
  if (!input.matches?.('[data-discussion-editor]')) return;
  if (event.key === 'Escape') { cancel(); close(input); return; }
  const list = listFor(input), choices = [...list.children];
  if (list.hidden || !choices.length) return;
  const selected = choices.findIndex(choice => choice.id === input.getAttribute('aria-activedescendant'));
  if (event.key === 'Enter' && selected >= 0) { event.preventDefault(); insert(input, choices[selected]); }
  if (!['ArrowDown', 'ArrowUp'].includes(event.key)) return;
  event.preventDefault();
  const next = ((selected < 0 && event.key === 'ArrowUp' ? 0 : selected) + (event.key === 'ArrowDown' ? 1 : choices.length - 1) + choices.length) % choices.length;
  for (const [i, choice] of choices.entries()) {
    choice.setAttribute('aria-selected', String(i === next));
    choice.classList.toggle('bg-subtle', i === next);
  }
  input.setAttribute('aria-activedescendant', choices[next].id);
  choices[next].scrollIntoView({ block: 'nearest' });
});
document.addEventListener('htmx:beforeSwap', cancel);
document.addEventListener('htmx:afterSettle', enhance);
document.addEventListener('htmx:historyRestore', enhance);
enhance();
