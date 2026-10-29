"use strict";

const $ = (id) => document.getElementById(id);
const KB = 1024, MB = 1024 * KB, GB = 1024 * MB;

function int(n) { return Math.round(n).toLocaleString("en-US"); }
function bytes(n) {
  if (n >= GB * 0.9995) return (n / GB).toFixed(n >= 9.95 * GB ? 1 : 2) + " GB";
  if (n >= MB * 0.9995) return (n / MB).toFixed(1) + " MB";
  if (n >= KB * 0.9995) return Math.round(n / KB) + " KB";
  return n + " B";
}
function words(n) {
  if (n >= 0.9995e9) return (n / 1e9).toFixed(2).replace(/\.?0+$/, "") + " billion";
  if (n >= 0.9995e6) return (n / 1e6).toFixed(n >= 99.95e6 ? 0 : 1).replace(/\.0$/, "") + " million";
  return int(n);
}
// the same rounding as words(), written in the Korean units 억 (10^8) and 만 (10^4)
function wordsKo(n) {
  const r = n >= 0.9995e9 ? +(n / 1e9).toFixed(2) * 1e9 : n >= 0.9995e6 ? +(n / 1e6).toFixed(n >= 99.95e6 ? 0 : 1) * 1e6 : null;
  if (r == null) return int(n);
  return r >= 1e8 ? +(r / 1e8).toFixed(2) + "억" : int(r / 1e4) + "만";
}
function ms(v) { return v >= 999.5 ? (v / 1000).toFixed(2) + " s" : v < 9.95 ? v.toFixed(1) + " ms" : Math.round(v) + " ms"; }
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

// Korean copy: demo/i18n/strings_before_codex.json rewritten by Codex with demo/i18n/codex_ko_prompt.md, applied verbatim
// from demo/i18n/ko_codex.json. {name} marks a value the page fills in.
const KO = {
  "title": "AltoPIR · 검색 내용은 비공개",
  "tag": "CPU 서버에서 검색 내용을 숨기고 조회하기",
  "langAria": "언어 변경",
  "dark": "다크",
  "light": "라이트",
  "themeAria": "화면 테마 변경",
  "connecting": "서버 연결 중",
  "connected": "AltoPIR 서버 연결 완료",
  "notRunning": "데모 클라이언트가 실행 중이 아닙니다.",
  "h1": "무엇을 찾는지 서버에 알리지 않고 데이터베이스를 조회해 보세요.",
  "lede": "입력한 내용은 이 기기 밖으로 나가지 않습니다. 기기 안에서 해시한 뒤 암호화해 보내면, 서버는 데이터베이스의 모든 레코드를 사용해 답을 계산합니다. 답은 이 기기에서만 읽을 수 있습니다.",
  "tabsAria": "데이터베이스",
  "dbPasswords": "유출 비밀번호",
  "dbUrls": "위험한 링크",
  "dbDocs": "Wikipedia",
  "labelPasswords": "비밀번호",
  "labelUrls": "링크",
  "labelDocs": "질문",
  "check": "확인",
  "search": "검색",
  "show": "보기",
  "hide": "숨기기",
  "revealAria": "비밀번호 표시",
  "linePasswords": "유출 비밀번호 {entries}개 · 버킷 {rows}개(각 {record}) · 총 {db}",
  "lineUrls": "피싱 링크·도메인 {entries}개 · 버킷 {rows}개(각 {record}) · 총 {db}",
  "lineDocs": "문서 {articles}개에서 모은 단락 {passages}개 · 클러스터 {rows}개(각 {record}) · 총 {db}",
  "idlePasswords": "비밀번호를 입력하세요. 무작위로 만든 강력한 비밀번호라면 유출 목록에 없을 것입니다.",
  "idleUrls": "링크를 붙여 넣으세요. 링크 자체와 호스트, 상위 도메인 최대 4개를 확인합니다. 요청 6개를 한 번에 묶어 보냅니다.",
  "idleDocs": "질문을 입력하세요. 클라이언트에서 질문을 임베딩하고, 임베딩이 가장 가까운 단락 클러스터를 가져옵니다. 서버는 어떤 클러스터를 가져오는지 알 수 없습니다.",
  "notLoaded": "이 데모에는 해당 데이터베이스를 불러오지 않았습니다.",
  "try": "예시",
  "recorded": "조회 기록",
  "pickBelow": "아래에서 조회 기록 선택",
  "pickAbove": "위에서 조회 기록을 하나 선택하세요.",
  "answerWith": "조회 방식",
  "oneCoreOnly": "코어 하나에서만 측정한 결과라 여러 조회를 동시에 처리할 때 걸리는 시간은 제공하지 않습니다.",
  "server": "서버 부하",
  "busy": "바쁨 · 조회 {n}건 동시 요청",
  "idle": "한가함 · 이번 조회만 요청",
  "thisServer": "이 서버",
  "thisCpu": "이 CPU",
  "whyBusy": "이번과 같은 조회 {n}건이 한꺼번에 들어옵니다. 각 방식은 {where}에서 측정한 throughput으로 모든 조회를 처리합니다. 마지막 조회까지 끝나면 결과를 보여 줍니다.",
  "whyIdleReplay": "조회 결과는 같은 데이터베이스에서 가져옵니다. {scheme} 공식 구현을 {where}에서 실행해 측정한 시간만큼 기다린 뒤 결과를 보여 줍니다.",
  "whyIdleLive": "조회 결과는 같은 데이터베이스에서 가져옵니다. {scheme} 공식 구현을 {where}에서 실행해 측정한 시간만큼 기다린 뒤 결과를 보여 줍니다.",
  "found": "유출 목록에서 발견",
  "seen": "유출 데이터에 약 <b>{n}</b>번 등장합니다.",
  "notFound": "유출 목록에 없음",
  "noneMatch": "유출 비밀번호 <b>{n}</b>개 중 일치하는 비밀번호가 없습니다.",
  "notListed": "피싱 목록에 없음",
  "noForm": "링크를 <b>{forms}</b>가지 형태로 확인했지만, 피싱 링크·도메인 <b>{entries}</b>개 중 일치하는 항목이 없습니다.",
  "listed": "피싱 목록에서 발견",
  "wireTitle": "기기 밖으로 보낸 데이터",
  "wireTitleReplay": "클라이언트가 보낸 데이터",
  "stayedTitle": "기기에 남은 데이터",
  "stayedTitleReplay": "클라이언트에 남은 데이터",
  "received": "서버가 받은 데이터",
  "yourPassword": "입력한 비밀번호",
  "itsSha1": "비밀번호의 SHA-1 해시",
  "yourLink": "입력한 링크",
  "yourQuestion": "입력한 질문",
  "numbers": "숫자 {n}개",
  "itsEmbedding": "질문 임베딩",
  "bucketsOf": "조회할 버킷(전체 {rows}개)",
  "bytesUp": "{n} bytes",
  "requests": "암호화한 요청 {r}개",
  "requestsFiller": "암호화한 요청 {r}개 ({filler}개는 무작위 요청으로 채워 모든 링크의 요청 수를 맞춤)",
  "readInFull": "요청마다 전체 읽기",
  "readInFullOne": "요청 처리 시 전체 읽기",
  "bytesBack": "응답 {n} bytes",
  "readableOnly": "{machine}에서만 읽을 수 있음",
  "hexCap": "요청에 담긴 암호화 키의 앞부분 96 bytes입니다. 나머지 부분도 서버에는 똑같이 보입니다.",
  "timingTitle": "실시간 AltoPIR 조회 소요 시간",
  "timingTitleReplay": "기록 당시 AltoPIR 조회 소요 시간",
  "here": "이 기기에서",
  "hereReplay": "클라이언트에서",
  "machine": "이 기기",
  "machineReplay": "클라이언트",
  "embedAt": "{here} 질문 임베딩",
  "encryptAt": "{here} 암호화",
  "network": "네트워크와 서버",
  "decryptAt": "{here} 복호화",
  "total": "전체 소요 시간",
  "compareBusy": "조회 {n}건 동시 요청 시 서버 처리 시간",
  "compareIdle": "이번 조회만 요청했을 때 서버 처리 시간",
  "compareTitle": "이번 조회의 서버 처리 시간",
  "qpsTitle": "서버 한 대의 초당 조회 수",
  "oneCore": "코어 하나",
  "answering": "{scheme} 처리 중",
  "waitBusy": "조회 {n}건 동시 처리에 총 {time} 소요 ({where}에서 측정한 throughput 기준)",
  "waitIdle": "총 {time} 소요 ({where}에서 측정한 속도 기준)",
  "answeredIn": "AltoPIR로 답을 받는 데 <b>{took}</b> 걸렸습니다.",
  "shownIdle": "이번 조회의 {scheme} 처리 시간에 맞춰 <b>{replayed}</b> 후에 결과를 표시했습니다. AltoPIR로는 <b>{took}</b> 걸렸습니다.",
  "shownBusyOurs": "AltoPIR로 동시 조회 {n}건을 모두 처리하는 시간에 맞춰 <b>{replayed}</b> 후에 결과를 표시했습니다. 이번 조회 한 건만 처리했을 때는 <b>{took}</b> 걸렸습니다.",
  "shownBusy": "{scheme} 방식으로 동시 조회 {n}건을 모두 처리하는 시간에 맞춰 <b>{replayed}</b> 후에 결과를 표시했습니다. AltoPIR로 모두 처리하는 데는 <b>{oursBusy}</b> 걸립니다.",
  "noAnswer": "데모 클라이언트에서 응답이 오지 않았습니다: ",
  "opTitle": "서버 한 대의 하루 처리량",
  "perSecond": "초당 조회 수",
  "perDay": "하루 조회 수",
  "alone": "조회 한 건만 처리할 때",
  "database": "데이터베이스 {size}",
  "fastest": "가장 빠른 기존 방식: 초당 {qps}회 ({name}, {size})",
  "fastestNoSize": "가장 빠른 기존 방식: 초당 {qps}회 ({name})",
  "foot": "데이터: Have I Been Pwned의 <a href=\"https://haveibeenpwned.com/Passwords\">Pwned Passwords</a>, <a href=\"https://github.com/Phishing-Database/Phishing.Database\">Phishing.Database</a> (MIT), <a href=\"https://simple.wikipedia.org/\">Simple English Wikipedia</a> (CC BY-SA 4.0). 코드: <a href=\"https://github.com/GyuyeongKim/AltoPIR-public\">AltoPIR</a>.",
  "rec.lede": "2026년 10월 28일에 기록한 조회를 재생하는 페이지입니다. 데모 클라이언트는 각 입력을 해시하거나 임베딩한 뒤 암호화했습니다. AltoPIR 서버는 데이터베이스의 모든 레코드를 사용해 답을 계산했고, 클라이언트만 답을 읽을 수 있었습니다. 각 결과를 보여 주기까지 걸리는 시간도 당시 서버에서 측정한 처리 속도에 맞췄습니다. 이 페이지에서는 서버로 아무것도 보내지 않습니다.",
  "rec.example.random": "무작위 비밀번호",
  "rec.example.phishingLink": "피싱 링크",
  "rec.example.phishingDomain": "피싱 도메인",
  "rec.example.fakeSignIn": "가짜 로그인 페이지",
  "rec.example.hostedPhishing": "호스팅 서비스에 올라온 피싱 페이지",
  "rec.example.wikipedia": "Wikipedia 페이지",
  "rec.example.news": "뉴스 사이트",
  "rec.example.code": "코드 저장소",
  "rec.category.link": "피싱 링크",
  "rec.category.domain": "피싱 도메인",
  "rec.baselineNote.docs": "서버 처리 시간은 모두 이 서버에서 측정했습니다. AltoPIR 측정에는 현재 데이터베이스를 사용했습니다. 나머지 방식은 공식 구현으로 1 GB 데이터베이스에서 측정했습니다. 레코드 크기는 32 KB로, 현재 데이터베이스의 128 KB보다 작습니다.",
  "rec.baselineNote.passwords": "서버 처리 시간은 모두 이 서버에서 측정했습니다. AltoPIR 측정에는 현재 데이터베이스를 사용했습니다. 나머지 방식은 공식 구현으로 8 GB 데이터베이스에서 측정했으며, 레코드 크기는 32 KB입니다. 현재 데이터베이스는 11.25 GB이므로, 나머지 방식으로 조회하면 측정값보다 오래 걸릴 것입니다.",
  "rec.baselineNote.urls": "서버 처리 시간은 모두 이 서버에서 측정했습니다. AltoPIR 측정에는 현재 데이터베이스를 사용했습니다. 나머지 방식은 공식 구현으로 64 MB 데이터베이스에서 측정했으며, 레코드 크기는 32 KB입니다.",
  "rec.sizeNote.same": "같은 크기에서 측정",
  "rec.sizeNote.at8gb": "8 GB에서 측정",
  "rec.operatorNote": "Xeon Silver 4514Y 서버에서 연산에 코어 15개를 쓰고, 조회를 최대 128개씩 묶어 측정했습니다. AltoPIR 논문에서는 1 GB~64 GB 데이터베이스로 성능을 비교했습니다. 같은 코어를 사용했을 때 AltoPIR의 쿼리 처리량은 힌트 없이 동작하는 기존 방식 중 가장 빠른 방식의 24~50배였습니다.",
  "rec.error.notRecorded": "이 페이지에는 미리 기록한 조회만 있습니다. 위에서 하나를 선택하세요.",
};
// text that comes with the recording, by its English original -> its key in KO
const KO_RECORDED = {
  "This page replays lookups recorded on 28 October 2026. A demo client hashed or embedded each input and encrypted it. An AltoPIR server computed over every record of its database, and only the client could read the answer. Each answer appears after the time it takes at the speeds measured on that server, and nothing you do here reaches a server.": "rec.lede",
  "a random one": "rec.example.random",
  "a phishing link": "rec.example.phishingLink",
  "a phishing domain": "rec.example.phishingDomain",
  "a fake sign-in page": "rec.example.fakeSignIn",
  "a hosted phishing page": "rec.example.hostedPhishing",
  "a Wikipedia page": "rec.example.wikipedia",
  "a news site": "rec.example.news",
  "a code repository": "rec.example.code",
  "phishing link": "rec.category.link",
  "phishing domain": "rec.category.domain",
  "Server time measured on this server: AltoPIR on this database; the others with their official implementations on a 1 GB database with their 32 KB records, smaller than the 128 KB records here.": "rec.baselineNote.docs",
  "Server time measured on this server: AltoPIR on this database; the others with their official implementations at 8 GB and 32 KB records (this database is 11.25 GB, so they would take longer).": "rec.baselineNote.passwords",
  "Server time measured on this server: AltoPIR on this database; the others with their official implementations on a 64 MB database with their 32 KB records.": "rec.baselineNote.urls",
  "same size": "rec.sizeNote.same",
  "measured at 8 GB": "rec.sizeNote.at8gb",
  "Measured on a Xeon Silver 4514Y server with 15 engine cores and batches of up to 128 lookups. In the AltoPIR paper, the fastest existing hintless scheme answers 24 to 50 times fewer queries on the same cores, for databases of 1 GB to 64 GB.": "rec.operatorNote",
  "This page holds only the recorded lookups. Pick one of them above.": "rec.error.notRecorded",
};
const fill = (s, v) => s.replace(/\{(\w+)\}/g, (m, k) => (k in v ? String(v[k]) : m));

// the page's text; a value is a string or a function of the variables given to t()
const I18N = {
  en: {
    title: "AltoPIR · Private lookups",
    tag: "private lookups on a CPU server",
    langButton: "한국어", langAria: "Switch the language",
    dark: "Dark", light: "Light", themeAria: "Switch the color theme",
    connecting: "Connecting to the server", connected: "Connected to the AltoPIR server",
    notRunning: "The demo client is not running",
    h1: "Ask a database a question without telling it what you asked.",
    lede: "Your input never leaves this machine. It is hashed and encrypted here, the server computes over every record of its " +
          "database, and the answer comes back in a form that only this machine can read.",
    tabsAria: "Databases", dbPasswords: "Leaked passwords", dbUrls: "Unsafe links", dbDocs: "Wikipedia",
    labelPasswords: "Password", labelUrls: "Link", labelDocs: "Question", check: "Check", search: "Search",
    show: "Show", hide: "Hide", revealAria: "Show the password",
    linePasswords: (m) => `${words(m.entries)} leaked passwords, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    lineUrls: (m) => `${words(m.entries)} phishing links and domains, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    lineDocs: (m) => `${words(m.passages)} passages from ${int(m.articles)} articles, ${int(m.rows)} clusters of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    idlePasswords: "Type a password. A strong random one should not appear among the leaked passwords.",
    idleUrls: "Paste a link. The link, its host and up to four parent domains are checked together, in one batch of six requests.",
    idleDocs: "Ask a question. It is embedded here, and the nearest clusters of passages are fetched without the server learning which.",
    notLoaded: "This database is not loaded on this demo.",
    try: "Try", recorded: "Recorded", pickBelow: "Pick a recorded lookup below", pickAbove: "Pick one of the recorded lookups above.",
    answerWith: "Answer with", oneCoreOnly: "Measured on one core only, so it has no time for many lookups at once",
    server: "Server", busy: (v) => `Busy, ${v.n} lookups at once`, idle: "Idle, this lookup alone",
    thisServer: "this server", thisCpu: "this CPU",
    whyBusy: (v) => `${v.n} lookups like this one arrive together. Each scheme answers all of them at the throughput measured on ${v.where}, and the answer appears when the last one is done.`,
    whyIdle: (v) => `The answer comes from the same database; it appears after the time ${v.scheme} needs on ${v.where}, ${v.replay ? "which we measured" : "measured"} with its official implementation.`,
    found: "Found in leaked passwords",
    seen: (v) => `Seen about <b>${words(v.n)}</b> ${v.n === 1 ? "time" : "times"} in breach data.`,
    notFound: "Not among the leaked passwords",
    noneMatch: (v) => `None of the <b>${words(v.n)}</b> leaked passwords matches it.`,
    notListed: "Not on the phishing lists",
    noForm: (v) => `None of the <b>${v.n}</b> forms of this link is among <b>${words(v.entries)}</b> phishing links and domains.`,
    listed: "Listed as phishing",
    wireTitle: "What left this machine", wireTitleReplay: "What left the client",
    stayedTitle: "Stayed here", stayedTitleReplay: "Stayed on the client", received: "The server received",
    yourPassword: "your password", itsSha1: "its SHA-1", yourLink: "your link", yourQuestion: "your question",
    numbers: (v) => `${v.n} numbers`, itsEmbedding: "its embedding",
    bucketsOf: (v) => `${v.k > 1 ? "buckets" : "bucket"} of ${int(v.rows)}`,
    bytesUp: (v) => `${int(v.n)} bytes`,
    requests: (v) => `${v.r} encrypted ${v.r > 1 ? "requests" : "request"}` +
                     (v.filler > 0 ? `, ${v.filler} of them random, so every link sends ${v.r}` : ""),
    readInFull: (v) => `read in full for ${v.r > 1 ? "each request" : "the request"}`,
    bytesBack: (v) => `${int(v.n)} bytes back`, readableOnly: (v) => `readable only on ${v.machine}`,
    hexCap: "96 bytes from the start of the encrypted key in the request. The rest looks the same to the server.",
    timingTitle: "Time of the live AltoPIR lookup", timingTitleReplay: "Time of the recorded AltoPIR lookup",
    here: "here", hereReplay: "on the client", machine: "this machine", machineReplay: "the client",
    embedAt: (v) => `Embed the question ${v.here}`, encryptAt: (v) => `Encrypt ${v.here}`, network: "Network and server",
    decryptAt: (v) => `Decrypt ${v.here}`, total: "Total",
    compareBusy: (v) => `Server time for ${v.n} lookups at once`, compareIdle: "Server time of this lookup alone",
    compareTitle: "Server time of this lookup", qpsTitle: "Lookups per second, one server", oneCore: "one core",
    answering: (v) => `${v.scheme} is answering`,
    waitBusy: (v) => `${ms(v.target)} in all, for ${v.n} lookups at once at the throughput measured on ${v.where}`,
    waitIdle: (v) => `${ms(v.target)} in all, at the speed measured on ${v.where}`,
    answeredIn: (v) => `Answered in <b>${ms(v.took)}</b> with AltoPIR.`,
    shownIdle: (v) => `Shown after <b>${ms(v.replayed)}</b>, the time of this lookup with ${v.scheme}. With AltoPIR it took <b>${ms(v.took)}</b>.`,
    shownBusyOurs: (v) => `Shown after <b>${ms(v.replayed)}</b>, when AltoPIR answers ${v.n} lookups that arrive at once. This lookup alone took <b>${ms(v.took)}</b>.`,
    shownBusy: (v) => `Shown after <b>${ms(v.replayed)}</b>, when ${v.scheme} answers ${v.n} lookups that arrive at once. AltoPIR answers them in <b>${ms(v.oursBusy)}</b>.`,
    noAnswer: "The demo client did not answer: ",
    opTitle: "One server, all day", perSecond: "Lookups per second", perDay: "Lookups per day", alone: "One lookup alone",
    count: (v) => words(v.n), database: (v) => `${bytes(v.n)} database`,
    fastest: (v) => `Fastest existing scheme: ${v.qps} lookups per second (${v.name}${v.size ? ", " + v.size : ""}).`,
    foot: 'Data: <a href="https://haveibeenpwned.com/Passwords">Pwned Passwords</a> by Have I Been Pwned, ' +
          '<a href="https://github.com/Phishing-Database/Phishing.Database">Phishing.Database</a> (MIT), and ' +
          '<a href="https://simple.wikipedia.org/">Simple English Wikipedia</a> (CC BY-SA 4.0). ' +
          'Code: <a href="https://github.com/GyuyeongKim/AltoPIR-public">AltoPIR</a>.',
  },
  ko: {
    ...Object.fromEntries(Object.entries(KO).filter(([k, s]) => !k.startsWith("rec.") && !s.includes("{"))),
    langButton: "English",
    linePasswords: (m) => fill(KO.linePasswords, { entries: wordsKo(m.entries), rows: int(m.rows), record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    lineUrls: (m) => fill(KO.lineUrls, { entries: wordsKo(m.entries), rows: int(m.rows), record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    lineDocs: (m) => fill(KO.lineDocs, { passages: wordsKo(m.passages), articles: int(m.articles), rows: int(m.rows),
                                         record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    busy: (v) => fill(KO.busy, v), whyBusy: (v) => fill(KO.whyBusy, v),
    whyIdle: (v) => fill(v.replay ? KO.whyIdleReplay : KO.whyIdleLive, v),
    seen: (v) => fill(KO.seen, { n: wordsKo(v.n) }), noneMatch: (v) => fill(KO.noneMatch, { n: wordsKo(v.n) }),
    noForm: (v) => fill(KO.noForm, { forms: v.n, entries: wordsKo(v.entries) }),
    numbers: (v) => fill(KO.numbers, v), bucketsOf: (v) => fill(KO.bucketsOf, { rows: int(v.rows) }),
    bytesUp: (v) => fill(KO.bytesUp, { n: int(v.n) }), bytesBack: (v) => fill(KO.bytesBack, { n: int(v.n) }),
    requests: (v) => fill(v.filler > 0 ? KO.requestsFiller : KO.requests, v),
    readInFull: (v) => (v.r > 1 ? KO.readInFull : KO.readInFullOne), readableOnly: (v) => fill(KO.readableOnly, v),
    embedAt: (v) => fill(KO.embedAt, v), encryptAt: (v) => fill(KO.encryptAt, v), decryptAt: (v) => fill(KO.decryptAt, v),
    compareBusy: (v) => fill(KO.compareBusy, v), answering: (v) => fill(KO.answering, v),
    waitBusy: (v) => fill(KO.waitBusy, { time: ms(v.target), n: v.n, where: v.where }),
    waitIdle: (v) => fill(KO.waitIdle, { time: ms(v.target), where: v.where }),
    answeredIn: (v) => fill(KO.answeredIn, { took: ms(v.took) }),
    shownIdle: (v) => fill(KO.shownIdle, { replayed: ms(v.replayed), scheme: v.scheme, took: ms(v.took) }),
    shownBusyOurs: (v) => fill(KO.shownBusyOurs, { replayed: ms(v.replayed), n: v.n, took: ms(v.took) }),
    shownBusy: (v) => fill(KO.shownBusy, { replayed: ms(v.replayed), scheme: v.scheme, n: v.n, oursBusy: ms(v.oursBusy) }),
    count: (v) => wordsKo(v.n), database: (v) => fill(KO.database, { size: bytes(v.n) }),
    fastest: (v) => fill(v.size ? KO.fastest : KO.fastestNoSize, v),
    recordedText: Object.fromEntries(Object.entries(KO_RECORDED).map(([en, k]) => [en, KO[k]])),
  },
};
let lang = "en";
function t(key, v) {
  const s = I18N[lang][key] ?? I18N.en[key];
  return typeof s === "function" ? s(v || {}) : s;
}
// text from the recording or the server: in Korean when the page has a translation of it, as it came otherwise
function tx(s) { return (lang === "ko" && s && I18N.ko.recordedText[s]) || s; }

const DBS = {
  passwords: {
    label: "labelPasswords", type: "password", button: "check", reveal: true,
    examples: [["password", "password"], ["iloveyou", "iloveyou"], ["correct horse battery staple", "correct horse battery staple"],
               ["a random one", () => randomPassword()]],
    line: "linePasswords", idle: "idlePasswords", name: "dbPasswords",
  },
  urls: {
    label: "labelUrls", type: "text", button: "check", reveal: false,
    examples: [["a phishing link", "http://netflix-billing-problem.com/bankverification.php"],
               ["a phishing domain", "https://account-paypal-security.com/signin"],
               ["a Wikipedia page", "https://en.wikipedia.org/wiki/Private_information_retrieval"]],
    line: "lineUrls", idle: "idleUrls", name: "dbUrls",
  },
  docs: {
    label: "labelDocs", type: "text", button: "search", reveal: false,
    examples: [["Who painted the Mona Lisa?", "Who painted the Mona Lisa?"], ["How do volcanoes form?", "How do volcanoes form?"],
               ["What does the liver do?", "What does the liver do?"]],
    line: "lineDocs", idle: "idleDocs", name: "dbDocs",
  },
};

let info = { databases: {}, operator: {} };
let ready = false;           // the databases are known and a tab is drawn
let serverMsg = { key: "connecting" };   // the masthead status, as a text key or as the server's own text
let current = "passwords";
let seq = 0, busy = false;   // only the latest request may render
let autoShown = false;       // an example password is shown until the user types their own
let scheme = "AltoPIR";
const CROWD = 100;   // lookups that arrive at once on a busy server
let crowd = true;    // busy by default; idle shows one lookup alone
let last = null;     // the lookup on screen, so the comparison follows the scheme and load pickers
let view = { kind: "idle" };   // what the answer box shows, so a language switch can draw it again
const sleep = (t) => new Promise((r) => setTimeout(r, t));
function baselines(db) { return ((info.operator || {}).baselines || {})[db]; }
// server time of k requests from a scheme's measured single-query time and throughput on this server
function estMs(s, k) { return s.qps ? Math.max(s.single_ms, 1000 * k / s.qps) : k * s.single_ms; }
// where the client work happens and where the speeds were measured (a replay: the recording client and server)
const here = () => t(info.replay ? "hereReplay" : "here");
const machine = () => t(info.replay ? "machineReplay" : "machine");
const where = () => t(info.replay ? "thisServer" : "thisCpu");
const isDark = () => document.documentElement.dataset.theme === "dark" ||
  (!document.documentElement.dataset.theme && matchMedia("(prefers-color-scheme: dark)").matches);

function setTab(db) {
  current = db;
  document.body.dataset.db = db;
  const c = DBS[db], replay = info.replay;
  $("input").type = replay ? "text" : c.type;   // a replay holds only recorded inputs, so a password shows as typed
  $("input").value = "";
  autoShown = false;
  $("reveal").hidden = !c.reveal || !!replay;
  $("go").disabled = !info.databases[db] || busy;
  $("input").readOnly = !!replay;
  view = { kind: "idle" };
  $("wire").hidden = true;
  renderTab();
  renderView();
  renderSchemes(db);
  renderOperator();
  $("input").focus();
}

// the text of the current tab; drawn again when the language changes, without touching the input
function renderTab() {
  const db = current, c = DBS[db], m = info.databases[db], replay = info.replay;
  document.querySelectorAll(".tab").forEach((x) => x.setAttribute("aria-selected", String(x.dataset.db === db)));
  $("input-label").textContent = t(c.label);
  $("reveal").textContent = t(c.reveal && !replay && $("input").type === "text" ? "hide" : "show");
  $("go").textContent = t(c.button);
  $("db-line").textContent = m ? t(c.line, m) : t("notLoaded");
  const ex = $("examples");
  ex.replaceChildren(el("span", {}, t(replay ? "recorded" : "try")));
  $("input").placeholder = replay ? t("pickBelow") : "";
  for (const [name, val] of replay ? replay.examples[db] || [] : c.examples) {
    const b = el("button", { type: "button" }, tx(name));
    const pick = () => (typeof val === "function" ? val() : Array.isArray(val) ? val[Math.floor(Math.random() * val.length)] : val);
    b.onclick = () => {
      if (busy) return;
      if (c.reveal && !replay) { $("input").type = "text"; $("reveal").textContent = t("hide"); autoShown = true; }
      $("input").value = pick();
      submit();
    };
    ex.append(b);
  }
}

function renderSchemes(db) {
  const b = baselines(db), box = $("schemes");
  if (!b) { box.hidden = true; scheme = "AltoPIR"; return; }
  // a scheme without a measured throughput (YPIR+SP runs on one core) has no time for many lookups at once
  const usable = (name) => name === "AltoPIR" || (b.schemes[name] && (!crowd || b.schemes[name].qps));
  if (!usable(scheme)) scheme = "AltoPIR";
  box.hidden = false;
  const who = el("div", { class: "row" }, el("span", {}, t("answerWith")));
  for (const name of ["AltoPIR", ...Object.keys(b.schemes)]) {
    const btn = el("button", { type: "button", class: name === "AltoPIR" ? "ours" : "", "aria-pressed": String(name === scheme) }, name);
    if (!usable(name)) { btn.disabled = true; btn.title = t("oneCoreOnly"); }
    btn.onclick = () => { scheme = name; renderSchemes(db); refreshCompare(db); };
    who.append(btn);
  }
  const load = el("div", { class: "row" }, el("span", {}, t("server")));
  for (const [label, value] of [[t("busy", { n: CROWD }), true], [t("idle"), false]]) {
    const btn = el("button", { type: "button", "aria-pressed": String(value === crowd) }, label);
    btn.onclick = () => { crowd = value; renderSchemes(db); refreshCompare(db); };
    load.append(btn);
  }
  const why = crowd
    ? t("whyBusy", { n: CROWD, where: where() })
    : scheme !== "AltoPIR"
      ? t("whyIdle", { scheme, where: where(), replay: !!info.replay })
      : null;
  box.replaceChildren(who, load, why ? el("p", { class: "why" }, why) : "");
}

function verdict(kind, head, sub) {
  return el("div", { class: "verdict " + kind }, el("p", { class: "head" }, head), sub ? el("p", { class: "sub", html: sub }) : null);
}

function renderAnswer(db, d) {
  const m = info.databases[db];
  if (d.error) return el("p", { class: "error" }, tx(d.error));
  if (db === "passwords") {
    return d.found
      ? verdict("bad", t("found"), t("seen", { n: d.seen_about }))
      : verdict("good", t("notFound"), t("noneMatch", { n: m.entries }));
  }
  if (db === "urls") {
    if (!d.unsafe) return verdict("good", t("notListed"), t("noForm", { n: d.candidates, entries: m.entries }));
    const v = verdict("bad", t("listed"), null);
    v.append(el("ul", { class: "matches" }, ...d.matches.map((x) => el("li", {}, `${x.entry}  ·  ${tx(x.category)}`))));
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

// the answer box: the tab's hint, the wait for the chosen scheme, the answer with its note, or an error
function renderView() {
  const v = view, box = $("answer");
  if (v.kind === "idle") {
    box.replaceChildren(el("p", { class: "placeholder" }, t(info.replay ? "pickAbove" : DBS[current].idle)));
  } else if (v.kind === "wait") {   // the bar keeps running; only its text changes
    v.head.textContent = t("answering", v);
    v.small.textContent = t(v.many ? "waitBusy" : "waitIdle", { ...v, where: where() });
  } else if (v.kind === "error") {
    box.replaceChildren(el("p", { class: "error" }, t("noAnswer") + v.message));
  } else {
    const { db, d } = v;
    box.replaceChildren(renderAnswer(db, d));
    // the note goes above a list of passages, which can run below the fold, and under a short verdict
    if (d.wire && !d.error) {
      const tm = d.wire.time, took = tm.prepare_ms + tm.round_trip_ms + tm.decrypt_ms + (tm.embed_ms || 0);
      const vars = { ...v, took, n: CROWD };
      const note = v.replayed == null ? t("answeredIn", vars)
        : !v.many ? t("shownIdle", vars)
        : v.scheme === "AltoPIR" ? t("shownBusyOurs", vars)
        : t("shownBusy", vars);
      box[db === "docs" ? "prepend" : "append"](el("p", { class: "replayed" + (db === "docs" ? " top" : ""), html: note }));
    }
    if (d.wire) renderWire(db, v.input, d, last && last.d === d ? last.sel : v.scheme); else $("wire").hidden = true;
  }
}

function renderWire(db, input, d, chosen) {
  const w = d.wire, m = info.databases[db];
  const stayed = $("stayed"), sent = $("sent");
  stayed.replaceChildren();
  sent.replaceChildren();
  const item = (ul, main, small, cls) => ul.append(el("li", {}, el("span", { class: cls || "" }, main), small ? el("small", {}, small) : null));
  if (db === "passwords") {
    const shown = $("input").type === "text";   // the panel shows the password only when the field does
    item(stayed, shown ? input : "•".repeat(Math.min(input.length, 16)), t("yourPassword"), shown ? "m" : "");
    item(stayed, w.local[0].hash + "…", t("itsSha1"), "m");
  } else if (db === "urls") {
    item(stayed, input.length > 48 ? input.slice(0, 46) + "…" : input, t("yourLink"), "m");
  } else {
    item(stayed, input, t("yourQuestion"));
    item(stayed, t("numbers", { n: m.dim }), t("itsEmbedding"), "n");
  }
  const buckets = w.local.map((x) => int(x.bucket)).join(", ");
  item(stayed, buckets, t("bucketsOf", { k: w.local.length, rows: w.rows }), "n");
  item(sent, t("bytesUp", { n: w.up_bytes }), t("requests", { r: w.requests, filler: w.requests - w.local.length }), "n");
  item(sent, bytes(w.rows * w.record_bytes), t("readInFull", { r: w.requests }), "n");
  item(sent, t("bytesBack", { n: w.down_bytes }), t("readableOnly", { machine: machine() }), "n");

  $("hex").textContent = w.preview_hex.match(/.{1,8}/g).join(" ").match(/.{1,36}/g).join("\n");

  const tm = w.time, parts = [];
  if (tm.embed_ms != null) parts.push([t("embedAt", { here: here() }), tm.embed_ms, "var(--t-embed)"]);
  parts.push([t("encryptAt", { here: here() }), tm.prepare_ms, "var(--t-encrypt)"]);
  parts.push([t("network"), tm.round_trip_ms, "var(--accent)"]);
  parts.push([t("decryptAt", { here: here() }), tm.decrypt_ms, "var(--t-decrypt)"]);
  const total = parts.reduce((s, p) => s + p[1], 0);
  $("bar").replaceChildren(...parts.map(([n, v, c]) => el("span", { style: `width:${(100 * v / total).toFixed(2)}%;background:${c}`, title: n })));
  const lg = $("legend");
  lg.replaceChildren();
  for (const [n, v, c] of parts) lg.append(el("dt", { style: `background:${c}` }), el("dd", {}, n), el("dd", { class: "v" }, ms(v)));
  lg.append(el("dt", { class: "total" }), el("dd", { class: "total" }, t("total")), el("dd", { class: "v total" }, ms(total)));
  last = { db, d, input };
  renderCompare(db, d, chosen);
  $("wire").hidden = false;
}

function refreshCompare(db) { if (last && last.db === db && !$("wire").hidden) renderCompare(db, last.d, scheme); }

function renderCompare(db, d, chosen) {
  const b = baselines(db), box = $("compare");
  if (last) last.sel = chosen;
  if (!b) { box.hidden = true; return; }
  // every bar uses the same rule: measured single-query time and throughput on this server, for this many requests
  const k = d.wire.requests * (crowd ? CROWD : 1), ours = ((info.operator || {}).databases || {})[db];
  const rows = ours ? [["AltoPIR", estMs(ours, k)]] : [];
  for (const [name, s] of Object.entries(b.schemes)) rows.push([name, crowd && !s.qps ? null : estMs(s, k)]);
  $("compare-title").textContent = crowd ? t("compareBusy", { n: CROWD }) : t("compareIdle");
  const bars = (box, rows, fmt) => {
    const top = Math.max(...rows.filter((r) => r[1] != null).map((r) => r[1]));
    box.replaceChildren();
    for (const [name, v] of rows) {
      const cls = "crow" + (name === "AltoPIR" ? " ours" : "") + (name === chosen && name !== "AltoPIR" ? " sel" : "");
      box.append(el("div", { class: cls }, el("span", { class: "n" }, name),
        el("span", { class: "b" }, v == null ? null : el("span", { style: `width:${Math.max(0.4, 100 * v / top).toFixed(2)}%` })),
        el("span", { class: "v" }, v == null ? t("oneCore") : fmt(v))));
    }
  };
  bars($("compare-rows"), rows, (v) => (v >= 1000 ? (v / 1000).toFixed(2) + " s" : ms(v)));
  const qrows = [["AltoPIR", ours ? ours.qps : null]];
  for (const [name, s] of Object.entries(b.schemes)) qrows.push([name, s.qps]);
  bars($("compare-qps"), qrows, (v) => (v < 10 ? v.toFixed(1) : int(v)));
  $("compare-cap").textContent = tx(b.note);
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
    const many = crowd, n = many ? CROWD : 1;
    const ours = ((info.operator || {}).databases || {})[db];
    const base = !d.wire || !baselines(db) ? null : chosen === "AltoPIR" ? ours : baselines(db).schemes[chosen];
    let replayed = null, oursBusy = null;
    if (base && (many || chosen !== "AltoPIR") && (!many || base.qps)) {
      // the chosen scheme's measured speed: same client work and network, its server time for n lookups at once
      const tm = d.wire.time, k = d.wire.requests, client = tm.prepare_ms + tm.decrypt_ms + (tm.embed_ms || 0);
      const network = ours ? Math.max(0, tm.round_trip_ms - estMs(ours, k)) : 0;
      const target = client + network + estMs(base, k * n), left = target - (performance.now() - t0);
      replayed = target;
      if (ours) oursBusy = client + network + estMs(ours, k * n);
      if (left > 0) {
        const fill = el("div", { class: "fill" }), head = el("p"), small = el("small");
        view = { kind: "wait", scheme: chosen, target, many, n: CROWD, head, small };
        $("answer").replaceChildren(el("div", { class: "wait" }, head, el("div", { class: "track" }, fill), small));
        renderView();
        document.body.classList.remove("loading");
        requestAnimationFrame(() => { fill.style.transition = `width ${left}ms linear`; fill.style.width = "100%"; });
        await sleep(left);
        if (mine !== seq || db !== current) return;
      }
    }
    view = { kind: "answer", db, d, input, scheme: chosen, many, replayed, oursBusy };
    renderView();
  } catch (e) {
    if (mine !== seq) return;
    view = { kind: "error", message: e.message };
    renderView();
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
  $("op-note").textContent = tx(op.note || "");
  const cards = $("op-cards");
  cards.replaceChildren();
  for (const db of ["passwords", "urls", "docs"]) {
    const o = dbs[db], m = info.databases[db];
    if (!o || !m) continue;
    const dl = el("dl");
    const row = (k, v, big) => dl.append(el("dt", {}, k), el("dd", { class: big ? "big" : "" }, v));
    row(t("perSecond"), int(o.qps), true);
    row(t("perDay"), t("count", { n: o.qps * 86400 }));
    row(t("alone"), ms(o.single_ms));
    const card = el("div", { class: "card" }, el("h3", {}, t(DBS[db].name)),
                    el("p", { class: "db" }, t("database", { n: m.db_bytes })), dl);
    const b = baselines(db);
    if (b) {
      const best = Object.entries(b.schemes).filter(([, s]) => s.qps).sort((x, y) => y[1].qps - x[1].qps)[0];
      if (best) card.append(el("p", { class: "vs" }, t("fastest", { qps: best[1].qps < 10 ? best[1].qps.toFixed(1) : int(best[1].qps),
                                                                  name: best[0], size: tx(b.size_note) })));
    }
    if (db === current) card.classList.add("current");
    cards.append(card);
  }
}

// every text on the page in the current language; the tab, the pickers and a shown answer keep their state
function applyLang() {
  document.documentElement.lang = lang;
  document.title = t("title");
  document.querySelectorAll("[data-i18n]").forEach((e) => (e.textContent = t(e.dataset.i18n)));
  document.querySelectorAll("[data-i18n-html]").forEach((e) => (e.innerHTML = t(e.dataset.i18nHtml)));
  document.querySelectorAll("[data-i18n-aria]").forEach((e) => e.setAttribute("aria-label", t(e.dataset.i18nAria)));
  $("lang").textContent = t("langButton");
  $("theme").textContent = t(isDark() ? "light" : "dark");
  $("server-text").textContent = serverMsg.key ? t(serverMsg.key) : tx(serverMsg.text);
  const replay = info.replay;
  if (replay) $("lede-text").textContent = tx(replay.lede);
  $("wire-title").textContent = t(replay ? "wireTitleReplay" : "wireTitle");
  $("stayed-title").textContent = t(replay ? "stayedTitleReplay" : "stayedTitle");
  $("timing-title").textContent = t(replay ? "timingTitleReplay" : "timingTitle");
  if (!last) $("compare-title").textContent = t("compareTitle");
  if (!ready) return;
  renderTab();
  renderView();
  // the panel of an earlier lookup stays on screen while the next one waits
  if (view.kind !== "answer" && last && !$("wire").hidden) renderWire(last.db, last.input, last.d, last.sel);
  renderSchemes(current);
  renderOperator();
}

// a plain URL opens in English; only ?lang=ko opens Korean
function initialLang() { return new URLSearchParams(location.search).get("lang") === "ko" ? "ko" : "en"; }

async function load() {
  try {
    const r = await fetch("/api/info");
    info = await r.json();
    const s = info.operator && info.operator.server;
    serverMsg = s ? { text: s } : { key: "connected" };
    $("dot").className = "dot ok";
    if (info.replay) {   // a static copy of the page answers from lookups recorded on the demo client
      $("server").hidden = true;
      serverMsg = { text: "" };
    }
  } catch (e) {
    serverMsg = { key: "notRunning" };
    $("dot").className = "dot bad";
  }
  // ?db=urls&q=...&theme=light opens a tab and runs one lookup (for slides and screenshots)
  const qs = new URLSearchParams(location.search);
  if (qs.get("theme")) document.documentElement.dataset.theme = qs.get("theme");
  if (qs.get("scheme")) scheme = qs.get("scheme");
  if (qs.get("load") === "idle") crowd = false;
  applyLang();
  ready = true;
  setTab(Object.hasOwn(DBS, qs.get("db") || "") ? qs.get("db") : current);
  if (qs.get("q")) { $("input").value = qs.get("q"); submit(); }
}

document.querySelectorAll(".tab").forEach((x) => (x.onclick = () => setTab(x.dataset.db)));
$("form").addEventListener("submit", submit);
$("theme").onclick = () => {
  const th = isDark() ? "light" : "dark";
  document.documentElement.dataset.theme = th;
  try { localStorage.setItem("theme", th); } catch (e) {}
  $("theme").textContent = t(th === "dark" ? "light" : "dark");
};
// the switch writes the language into the URL (?lang=ko, none for English), so a reload keeps it
$("lang").onclick = () => {
  lang = lang === "ko" ? "en" : "ko";
  try {
    const u = new URL(location.href);
    if (lang === "en") u.searchParams.delete("lang"); else u.searchParams.set("lang", lang);
    history.replaceState(null, "", u);
  } catch (e) {}
  applyLang();
};
$("input").addEventListener("input", () => {
  if (!autoShown) return;
  autoShown = false;
  $("input").type = "password";
  $("reveal").textContent = t("show");
});
$("reveal").onclick = () => {
  autoShown = false;
  const i = $("input"), show = i.type === "password";
  i.type = show ? "text" : "password";
  $("reveal").textContent = t(show ? "hide" : "show");
};
lang = initialLang();
applyLang();
load();
