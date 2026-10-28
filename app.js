"use strict";

const $ = (id) => document.getElementById(id);
const KB = 1024, MB = 1024 * KB, GB = 1024 * MB;

function int(n) { return Math.round(n).toLocaleString("en-US"); }
function bytes(n) {
  if (n >= GB * 0.9995) return (n / GB).toFixed(n >= 9.95 * GB ? 1 : 2) + "\u00a0GB";
  if (n >= MB * 0.9995) return (n / MB).toFixed(1) + "\u00a0MB";
  if (n >= KB * 0.9995) return Math.round(n / KB) + "\u00a0KB";
  return n + "\u00a0B";
}
function words(n) {
  if (n >= 0.9995e9) return (n / 1e9).toFixed(2).replace(/\.?0+$/, "") + " billion";
  if (n >= 0.9995e6) return (n / 1e6).toFixed(n >= 99.95e6 ? 0 : 1).replace(/\.0$/, "") + " million";
  return int(n);
}
function ms(v) { return v >= 999.5 ? (v / 1000).toFixed(2) + "\u00a0s" : v < 9.95 ? v.toFixed(1) + "\u00a0ms" : Math.round(v) + "\u00a0ms"; }
function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v; else if (k === "html") e.innerHTML = v; else e.setAttribute(k, v);
  }
  for (const k of kids) if (k != null) e.append(k);
  return e;
}
function randomPassword() {
  const abc = "abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!#%+=?@";
  const r = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(r, (x) => abc[x % abc.length]).join("");
}

const DBS = {
  passwords: {
    label: "Password", type: "password", button: "Check", reveal: true,
    examples: [["password", "password"], ["iloveyou", "iloveyou"], ["correct horse battery staple", "correct horse battery staple"],
               ["a random one", () => randomPassword()]],
    line: (m) => `${words(m.entries)} leaked passwords, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    idle: "Type a password. A strong random one should not appear among the leaked passwords.",
  },
  urls: {
    label: "Link", type: "text", button: "Check", reveal: false,
    examples: [["a phishing link", "http://netflix-billing-problem.com/bankverification.php"],
               ["a phishing domain", "https://account-paypal-security.com/signin"],
               ["a Wikipedia page", "https://en.wikipedia.org/wiki/Private_information_retrieval"]],
    line: (m) => `${words(m.entries)} phishing links and domains, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    idle: "Paste a link. The link, its host and up to four parent domains are checked together, in one batch of six requests.",
  },
  docs: {
    label: "Question", type: "text", button: "Search", reveal: false,
    examples: [["Who painted the Mona Lisa?", "Who painted the Mona Lisa?"], ["How do volcanoes form?", "How do volcanoes form?"],
               ["What does the liver do?", "What does the liver do?"]],
    line: (m) => `${words(m.passages)} passages from ${int(m.articles)} articles, ${int(m.rows)} clusters of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    idle: "Ask a question. It is embedded here, and the nearest clusters of passages are fetched without the server learning which.",
  },
};

let info = { databases: {}, operator: {} };
const at = { here: "here", machine: "this machine" };   // where the client work happens (the recording client in a replay)
let current = "passwords";
let seq = 0, busy = false;   // only the latest request may render
let autoShown = false;       // an example password is shown until the user types their own
let scheme = "AltoPIR";
const sleep = (t) => new Promise((r) => setTimeout(r, t));
function baselines(db) { return ((info.operator || {}).baselines || {})[db]; }
// server time of k requests from a scheme's measured single-query time and throughput on this server
function estMs(s, k) { return s.qps ? Math.max(s.single_ms, 1000 * k / s.qps) : k * s.single_ms; }

function setTab(db) {
  current = db;
  document.body.dataset.db = db;
  const c = DBS[db], m = info.databases[db];
  document.querySelectorAll(".tab").forEach((t) => t.setAttribute("aria-selected", String(t.dataset.db === db)));
  $("input-label").textContent = c.label;
  const replay = info.replay;
  $("input").type = replay ? "text" : c.type;   // a replay holds only recorded inputs, so a password shows as typed
  $("input").value = "";
  autoShown = false;
  $("reveal").hidden = !c.reveal || !!replay;
  $("reveal").textContent = "Show";
  $("go").textContent = c.button;
  $("db-line").textContent = m ? c.line(m) : "This database is not loaded on this demo.";
  $("go").disabled = !m || busy;
  const ex = $("examples");
  ex.replaceChildren(el("span", {}, "Try"));
  $("input").readOnly = !!replay;
  $("input").placeholder = replay ? "Pick a recorded lookup below" : "";
  if (replay) ex.firstChild.textContent = "Recorded";
  for (const [name, val] of replay ? replay.examples[db] || [] : c.examples) {
    const b = el("button", { type: "button" }, name);
    const pick = () => (typeof val === "function" ? val() : Array.isArray(val) ? val[Math.floor(Math.random() * val.length)] : val);
    b.onclick = () => {
      if (busy) return;
      if (c.reveal && !replay) { $("input").type = "text"; $("reveal").textContent = "Hide"; autoShown = true; }
      $("input").value = pick();
      submit();
    };
    ex.append(b);
  }
  $("answer").replaceChildren(el("p", { class: "placeholder" }, replay ? "Pick one of the recorded lookups above. Its answer appears after the time it took when it was recorded." : c.idle));
  $("wire").hidden = true;
  renderSchemes(db);
  renderOperator();
  $("input").focus();
}

function renderSchemes(db) {
  const b = baselines(db), box = $("schemes");
  if (!b) { box.hidden = true; scheme = "AltoPIR"; return; }
  if (scheme !== "AltoPIR" && !b.schemes[scheme]) scheme = "AltoPIR";
  box.hidden = false;
  box.replaceChildren(el("span", {}, "Answer with"));
  for (const name of ["AltoPIR", ...Object.keys(b.schemes)]) {
    const btn = el("button", { type: "button", class: name === "AltoPIR" ? "ours" : "", "aria-pressed": String(name === scheme) }, name);
    btn.onclick = () => { scheme = name; renderSchemes(db); };
    box.append(btn);
  }
  if (scheme !== "AltoPIR") box.append(el("p", { class: "why" },
    `The answer comes from the same database; it appears after the time ${scheme} needs on this server, which we measured with its official implementation.`));
}

function verdict(kind, head, sub) {
  return el("div", { class: "verdict " + kind }, el("p", { class: "head" }, head), sub ? el("p", { class: "sub", html: sub }) : null);
}

function renderAnswer(db, d) {
  const m = info.databases[db];
  if (d.error) return el("p", { class: "error" }, d.error);
  if (db === "passwords") {
    return d.found
      ? verdict("bad", "Found in leaked passwords", `Seen about <b>${words(d.seen_about)}</b> ${d.seen_about === 1 ? "time" : "times"} in breach data.`)
      : verdict("good", "Not among the leaked passwords", `None of the <b>${words(m.entries)}</b> leaked passwords matches it.`);
  }
  if (db === "urls") {
    if (!d.unsafe) {
      const n = d.candidates;
      return verdict("good", "Not on the phishing lists",
        `None of the <b>${n}</b> forms of this link is among <b>${words(m.entries)}</b> phishing links and domains.`);
    }
    const v = verdict("bad", "Listed as phishing", null);
    v.append(el("ul", { class: "matches" }, ...d.matches.map((x) => el("li", {}, `${x.entry}  ·  ${x.category}`))));
    return v;
  }
  const list = el("ol", { class: "passages" });
  for (const p of d.passages) {
    const href = "https://simple.wikipedia.org/wiki/" + encodeURIComponent(p.title.replace(/ /g, "_"));
    list.append(el("li", {}, el("p", { class: "t" }, el("a", { href, target: "_blank", rel: "noopener" }, p.title)),
                   el("p", { class: "x" }, p.text)));
  }
  return list;
}

function renderWire(db, input, d, chosen) {
  const w = d.wire, m = info.databases[db];
  const stayed = $("stayed"), sent = $("sent");
  stayed.replaceChildren();
  sent.replaceChildren();
  const item = (ul, main, small, cls) => ul.append(el("li", {}, el("span", { class: cls || "" }, main), small ? el("small", {}, small) : null));
  if (db === "passwords") {
    const shown = $("input").type === "text";   // the panel shows the password only when the field does
    item(stayed, shown ? input : "•".repeat(Math.min(input.length, 16)), "your password", shown ? "m" : "");
    item(stayed, w.local[0].hash + "…", "its SHA-1", "m");
  } else if (db === "urls") {
    item(stayed, input.length > 48 ? input.slice(0, 46) + "…" : input, "your link", "m");
  } else {
    item(stayed, input, "your question");
    item(stayed, `${m.dim} numbers`, "its embedding", "n");
  }
  const buckets = w.local.map((x) => int(x.bucket)).join(", ");
  item(stayed, buckets, `${w.local.length > 1 ? "buckets" : "bucket"} of ${int(w.rows)}`, "n");
  const filler = w.requests - w.local.length;
  item(sent, `${int(w.up_bytes)} bytes`, `${w.requests} encrypted ${w.requests > 1 ? "requests" : "request"}` +
       (filler > 0 ? `, ${filler} of them random, so every link sends ${w.requests}` : ""), "n");
  item(sent, bytes(w.rows * w.record_bytes), `read in full for ${w.requests > 1 ? "each request" : "the request"}`, "n");
  item(sent, `${int(w.down_bytes)} bytes back`, `readable only on ${at.machine}`, "n");

  $("hex").textContent = w.preview_hex.match(/.{1,8}/g).join(" ").match(/.{1,36}/g).join("\n");

  const t = w.time, parts = [];
  if (t.embed_ms != null) parts.push([`Embed the question ${at.here}`, t.embed_ms, "var(--rule)"]);
  parts.push([`Encrypt ${at.here}`, t.prepare_ms, "var(--ink-3)"]);
  parts.push(["Network and server", t.round_trip_ms, "var(--coral-fill)"]);
  parts.push([`Decrypt ${at.here}`, t.decrypt_ms, "var(--blue)"]);
  const total = parts.reduce((s, p) => s + p[1], 0);
  $("bar").replaceChildren(...parts.map(([n, v, c]) => el("span", { style: `width:${(100 * v / total).toFixed(2)}%;background:${c}`, title: n })));
  const lg = $("legend");
  lg.replaceChildren();
  for (const [n, v, c] of parts) lg.append(el("dt", { style: `background:${c}` }), el("dd", {}, n), el("dd", { class: "v" }, ms(v)));
  lg.append(el("dt", { class: "total" }), el("dd", { class: "total" }, "Total"), el("dd", { class: "v total" }, ms(total)));
  renderCompare(db, d, chosen);
  $("wire").hidden = false;
}

function renderCompare(db, d, chosen) {
  const b = baselines(db), box = $("compare");
  if (!b) { box.hidden = true; return; }
  // every bar uses the same rule: measured single-query time and throughput on this server, for this many requests
  const k = d.wire.requests, ours = ((info.operator || {}).databases || {})[db];
  const rows = ours ? [["AltoPIR", estMs(ours, k)]] : [];
  for (const [name, s] of Object.entries(b.schemes)) rows.push([name, estMs(s, k)]);
  const bars = (box, rows, fmt) => {
    const top = Math.max(...rows.filter((r) => r[1] != null).map((r) => r[1]));
    box.replaceChildren();
    for (const [name, v] of rows) {
      const cls = "crow" + (name === "AltoPIR" ? " ours" : "") + (name === chosen && name !== "AltoPIR" ? " sel" : "");
      box.append(el("div", { class: cls }, el("span", { class: "n" }, name),
        el("span", { class: "b" }, v == null ? null : el("span", { style: `width:${Math.max(0.4, 100 * v / top).toFixed(2)}%` })),
        el("span", { class: "v" }, v == null ? "one core" : fmt(v))));
    }
  };
  bars($("compare-rows"), rows, (v) => (v >= 1000 ? (v / 1000).toFixed(2) + "\u00a0s" : ms(v)));
  const qrows = [["AltoPIR", ours ? ours.qps : null]];
  for (const [name, s] of Object.entries(b.schemes)) qrows.push([name, s.qps]);
  bars($("compare-qps"), qrows, (v) => (v < 10 ? v.toFixed(1) : int(v)));
  $("compare-cap").textContent = b.note;
  box.hidden = false;
}

async function submit(ev) {
  if (ev) ev.preventDefault();
  const raw = $("input").value;
  const input = current === "passwords" ? raw : raw.trim();   // a password is checked exactly as typed
  if (!input || !info.databases[current] || busy) return;
  const db = current, mine = ++seq, chosen = scheme, t0 = performance.now();
  busy = true;
  document.body.classList.add("loading");
  $("go").disabled = true;
  try {
    const r = await fetch("/api/" + db, { method: "POST", headers: { "Content-Type": "application/json" },
                                          body: JSON.stringify({ input }) });
    const d = await r.json();
    if (mine !== seq || db !== current) return;
    const base = chosen !== "AltoPIR" && d.wire && baselines(db) ? baselines(db).schemes[chosen] : null;
    let replayed = null;
    if (base) {   // replay the measured speed of the chosen scheme: same client work and network, its server time
      const t = d.wire.time, k = d.wire.requests, client = t.prepare_ms + t.decrypt_ms + (t.embed_ms || 0);
      const ours = ((info.operator || {}).databases || {})[db];
      const network = ours ? Math.max(0, t.round_trip_ms - estMs(ours, k)) : 0;
      const target = client + network + estMs(base, k), left = target - (performance.now() - t0);
      replayed = target;
      if (left > 0) {
        const fill = el("div", { class: "fill" });
        $("answer").replaceChildren(el("div", { class: "wait" }, el("p", {}, `${chosen} is answering`),
          el("div", { class: "track" }, fill), el("small", {}, `${ms(target)} in all, at the speed measured on this server`)));
        document.body.classList.remove("loading");
        requestAnimationFrame(() => { fill.style.transition = `width ${left}ms linear`; fill.style.width = "100%"; });
        await sleep(left);
        if (mine !== seq || db !== current) return;
      }
    }
    const ans = renderAnswer(db, d);
    $("answer").replaceChildren(ans);
    // the note goes above a list of passages, which can run below the fold, and under a short verdict
    if (replayed != null && !d.error) $("answer")[db === "docs" ? "prepend" : "append"](el("p", { class: "replayed" + (db === "docs" ? " top" : "") },
      `Shown after ${ms(replayed)}, the time of this lookup with ${chosen}. With AltoPIR it took ${ms(d.wire.time.prepare_ms + d.wire.time.round_trip_ms + d.wire.time.decrypt_ms + (d.wire.time.embed_ms || 0))}.`));
    if (d.wire) renderWire(db, input, d, chosen); else $("wire").hidden = true;
  } catch (e) {
    if (mine !== seq) return;
    $("answer").replaceChildren(el("p", { class: "error" }, "The demo client did not answer: " + e.message));
    $("wire").hidden = true;
  } finally {
    busy = false;
    document.body.classList.remove("loading");
    $("go").disabled = !info.databases[current];
  }
}

function renderOperator() {
  const op = info.operator || {}, dbs = op.databases || {};
  if (!Object.keys(dbs).length) { $("operator").hidden = true; return; }
  $("operator").hidden = false;
  const cloud = op.cloud;
  $("op-note").textContent = op.note || "";
  const cards = $("op-cards");
  cards.replaceChildren();
  for (const db of ["passwords", "urls", "docs"]) {
    const o = dbs[db], m = info.databases[db];
    if (!o || !m) continue;
    const dl = el("dl");
    const row = (k, v, big) => dl.append(el("dt", {}, k), el("dd", { class: big ? "big" : "" }, v));
    row("Lookups per second", int(o.qps), true);
    row("Lookups per day", words(o.qps * 86400));
    row("One lookup alone", ms(o.single_ms));
    if (cloud) row("Per million lookups", "$" + (cloud.usd_per_hour / (o.qps * 3600) * 1e6).toFixed(2));
    const card = el("div", { class: "card" }, el("h3", {}, { passwords: "Leaked passwords", urls: "Unsafe links", docs: "Wikipedia" }[db]),
                    el("p", { class: "db" }, bytes(m.db_bytes) + " database"), dl);
    const b = baselines(db);
    if (b) {
      const best = Object.entries(b.schemes).filter(([, s]) => s.qps).sort((x, y) => y[1].qps - x[1].qps)[0];
      if (best) card.append(el("p", { class: "vs" }, `Fastest existing scheme: ${best[1].qps < 10 ? best[1].qps.toFixed(1) : int(best[1].qps)} lookups per second (${best[0]}${b.size_note ? ", " + b.size_note : ""}).`));
    }
    if (db === current) card.style.background = "var(--surface)";
    cards.append(card);
  }
}

async function load() {
  try {
    const r = await fetch("/api/info");
    info = await r.json();
    const s = info.operator && info.operator.server;
    $("server-text").textContent = s ? s : "Connected to the AltoPIR server";
    $("dot").className = "dot ok";
    if (info.replay) {   // a static copy of the page answers from lookups recorded on the demo client
      at.here = "on the client"; at.machine = "the client";
      $("server-text").textContent = info.replay.banner;
      $("dot").className = "dot replay";
      $("lede-text").textContent = info.replay.lede;
      $("wire-title").textContent = "What left the client";
      $("stayed-title").textContent = "Stayed on the client";
      $("timing-title").textContent = "Time of the recorded AltoPIR lookup";
    }
  } catch (e) {
    $("server-text").textContent = "The demo client is not running";
    $("dot").className = "dot bad";
  }
  // ?db=urls&q=...&theme=light opens a tab and runs one lookup (for slides and screenshots)
  const qs = new URLSearchParams(location.search);
  if (qs.get("theme")) document.documentElement.dataset.theme = qs.get("theme");
  if (qs.get("scheme")) scheme = qs.get("scheme");
  setTab(Object.hasOwn(DBS, qs.get("db") || "") ? qs.get("db") : current);
  if (qs.get("q")) { $("input").value = qs.get("q"); submit(); }
}

document.querySelectorAll(".tab").forEach((t) => (t.onclick = () => setTab(t.dataset.db)));
$("form").addEventListener("submit", submit);
$("input").addEventListener("input", () => {
  if (!autoShown) return;
  autoShown = false;
  $("input").type = "password";
  $("reveal").textContent = "Show";
});
$("reveal").onclick = () => {
  autoShown = false;
  const i = $("input"), show = i.type === "password";
  i.type = show ? "text" : "password";
  $("reveal").textContent = show ? "Hide" : "Show";
};
load();
