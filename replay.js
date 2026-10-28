"use strict";
// Static copy of the demo page: answers its two API calls from lookups recorded on the demo client (replay.json),
// each after the time the recorded lookup took on the client, so the page runs without a client or a server.
(() => {
  const network = window.fetch.bind(window);
  let recorded = null;
  const load = () => recorded || (recorded = network("replay.json").then((r) => r.json()));
  const reply = (body) => new Response(JSON.stringify(body), { headers: { "Content-Type": "application/json" } });
  const sleep = (t) => new Promise((r) => setTimeout(r, t));
  window.fetch = async (url, opts = {}) => {
    const path = String(url);
    if (path === "/api/info") return reply({ ...(await load()).info, replay: (await load()).replay });
    const m = path.match(/^\/api\/(passwords|urls|docs)$/);
    if (!m) return network(url, opts);
    const r = await load(), input = JSON.parse(opts.body).input;
    const d = (r.lookups[m[1]] || {})[input];
    if (!d) return reply({ error: "This page holds only the recorded lookups. Pick one of them above." });
    const t = d.wire.time;
    await sleep(t.prepare_ms + t.round_trip_ms + t.decrypt_ms + (t.embed_ms || 0));
    return reply(d);
  };
})();
