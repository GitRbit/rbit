// Proposed controls on fixed synthetic fixtures; no runner/network connection exists.
document.documentElement.classList.add("js");
const log = document.querySelector("#log-window");
const feedback = document.querySelector("[data-log-feedback]");
const follow = document.querySelector("[data-follow]");
const say = (message) => { if (feedback) feedback.textContent = message; };
let following = false, matchIndex = -1, nextLine = 50000;
const pending = [];
function appendFixture(number) {
  const list = document.querySelector(".log-lines"); if (!list || !log) return;
  const row = document.createElement("li"); row.className = "log-line"; row.id = `L${number}`;
  const anchor = document.createElement("a"); anchor.href = `#L${number}`; anchor.className = "line-number"; anchor.textContent = String(number); anchor.setAttribute("aria-label", `Line ${number}`);
  const code = document.createElement("code"); code.textContent = "Simulated output for the proposed follow behavior.";
  row.append(anchor, code); list.append(row);
  if (list.children.length > 80) list.firstElementChild.remove();
  const windowNote = document.querySelector("[data-line-window]");
  if (windowNote) windowNote.textContent = `Lines ${Number(list.firstElementChild.id.slice(1)).toLocaleString("en-US")}–${number.toLocaleString("en-US")} of ${number.toLocaleString("en-US")} · earlier lines omitted`;
}
function setFollow(value) {
  following = value;
  if (follow) { follow.setAttribute("aria-pressed", String(value)); follow.textContent = value ? "Pause follow" : "Follow tail"; }
  if (value && log) { for (const number of pending.splice(0)) appendFixture(number); log.scrollTop = log.scrollHeight; }
}
follow?.addEventListener("click", () => { setFollow(!following); say(following ? "Following the fixture log tail" : "Follow paused. You can read earlier output."); });
log?.addEventListener("scroll", () => {
  if (following && log.scrollHeight - log.scrollTop - log.clientHeight > 24) { setFollow(false); say("Follow paused while you read earlier output."); }
});
document.querySelector("[data-copy]")?.addEventListener("click", async () => {
  const text = [...document.querySelectorAll(".log-line code")].map((node) => node.textContent).join("\n");
  try { await navigator.clipboard.writeText(text); say("Copied the visible log window."); }
  catch { say("Copy is unavailable. Select the log text and use your browser's copy command."); }
});
for (const button of document.querySelectorAll("[data-match]")) button.addEventListener("click", () => {
  const matches = [...document.querySelectorAll(".log-line mark")];
  if (!matches.length) { say("No matches in this log window. Use Find to search it."); return; }
  matchIndex = (matchIndex + (button.dataset.match === "next" ? 1 : -1) + matches.length) % matches.length;
  setFollow(false);
  matches[matchIndex].scrollIntoView({ block: "center", inline: "nearest" });
  say(`Match ${matchIndex + 1} of ${matches.length} in this window.`);
});
document.querySelector("[data-simulate]")?.addEventListener("click", () => {
  if (!log) return;
  const number = ++nextLine;
  if (following) { appendFixture(number); log.scrollTop = log.scrollHeight; say("Fixture output added; following the tail."); }
  else {
    pending.push(number); if (pending.length > 80) pending.shift();
    say(`${pending.length === 80 ? "80 or more" : pending.length} new fixture lines waiting. Resume follow to view the latest output.`);
  }
});
// Anchors are useful without scripts too; open the owning step before scrolling to it.
const target = /^#L\d+$/.test(location.hash) ? document.getElementById(location.hash.slice(1)) : null;
if (target) { target.closest("details").open = true; target.scrollIntoView({ block: "center", inline: "nearest" }); }
