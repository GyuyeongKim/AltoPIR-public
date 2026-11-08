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
// from demo/i18n/ko_codex.json; the race and throughput strings from demo/i18n/race_strings_codex.json and the scan
// strings from demo/i18n/scan_strings_codex.json, also verbatim.
// {name} marks a value the page fills in.
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
  "dbRag": "RAG",
  "labelPasswords": "비밀번호",
  "labelUrls": "링크",
  "labelDocs": "질문",
  "labelRag": "질문",
  "ask": "질문",
  "check": "확인",
  "search": "검색",
  "show": "보기",
  "hide": "숨기기",
  "revealAria": "비밀번호 표시",
  "linePasswords": "유출 비밀번호 {entries}개 · 버킷 {rows}개(각 {record}) · 총 {db}",
  "lineUrls": "피싱 링크·도메인 {entries}개 · 버킷 {rows}개(각 {record}) · 총 {db}",
  "lineDocs": "문서 {articles}개에서 모은 단락 {passages}개 · 클러스터 {rows}개(각 {record}) · 총 {db}",
  "lineRag": "Wikipedia 단락 {passages}개 · 클러스터 {rows}개(각 {record}) · 총 {db}",
  "idlePasswords": "비밀번호를 입력하세요. 무작위로 만든 강력한 비밀번호라면 유출 목록에 없을 것입니다.",
  "idleUrls": "링크를 붙여 넣으세요. 링크 자체와 호스트, 상위 도메인 최대 4개를 확인합니다. 요청 6개를 한 번에 묶어 보냅니다.",
  "idleDocs": "질문을 입력하세요. 클라이언트에서 질문을 임베딩하고, 임베딩이 가장 가까운 단락 클러스터를 가져옵니다. 서버는 어떤 클러스터를 가져오는지 알 수 없습니다.",
  "idleRag": "질문을 입력하세요. 기기에서 질문을 임베딩하고, 가장 가까운 단락 클러스터를 서버가 모르게 가져옵니다. {model}은 그 안에서 질문과 가장 가까운 단락 다섯 개를 읽고 답합니다.",
  "notLoaded": "이 데모에는 해당 데이터베이스를 불러오지 않았습니다.",
  "try": "예시",
  "recorded": "조회 기록",
  "pickBelow": "아래에서 조회 기록 선택",
  "pickAbove": "위에서 조회 기록을 하나 선택하세요.",
  "oneCoreOnly": "코어 하나에서만 측정한 결과라 여러 조회를 동시에 처리할 때 걸리는 시간은 제공하지 않습니다.",
  "server": "서버 부하",
  "busy": "바쁨 · 조회 {n}건 동시 요청",
  "idle": "한가함 · 이번 조회만 요청",
  "thisServer": "이 서버",
  "thisCpu": "이 CPU",
  "whyBusy": "이번과 같은 조회 {n}건이 한꺼번에 들어옵니다. 각 방식은 {where}에서 측정한 throughput으로 모든 조회를 처리합니다. 마지막 조회까지 끝나면 막대가 가득 찹니다.",
  "whyIdleRace": "서버에는 이번 조회만 들어옵니다. 기존 방식마다 {where}에서 공식 구현으로 측정한 서버 처리 시간에 AltoPIR 조회 때 걸린 클라이언트·네트워크 시간을 더합니다.",
  "boardTitle": "가장 빠른 기존 방식과 throughput 비교",
  "times": "{x}배",
  "moreThan": "{name}보다 초당 조회 수가 많음",
  "raceTitle": "답을 받기까지 걸린 시간",
  "raceTitleRag": "첫 토큰까지 걸린 시간",
  "slower": "{r}배 오래 걸림",
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
  "clusterOf": "가져올 클러스터(전체 {rows}개)",
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
  "searchAt": "{here} 가까운 클러스터 찾기",
  "prefill": "{model}의 단락 읽기",
  "total": "전체 소요 시간",
  "ttft": "첫 토큰까지",
  "modelAnswer": "{model}의 답변",
  "timingTitleRag": "실시간 질문의 첫 토큰까지 걸린 시간",
  "timingTitleRagReplay": "기록 당시 첫 토큰까지 걸린 시간",
  "answeredInRag": "AltoPIR로 첫 토큰까지 <b>{took}</b> 걸렸습니다.",
  "shownBusyOursRag": "AltoPIR로 동시 질문 {n}건을 모두 처리하는 시간에 맞춰 <b>{replayed}</b> 후에 답을 표시했습니다. 이 질문 하나만 처리했을 때는 첫 토큰까지 <b>{took}</b> 걸렸습니다.",
  "oneCore": "코어 하나",
  "answeredIn": "AltoPIR로 답을 받는 데 <b>{took}</b> 걸렸습니다.",
  "shownBusyOurs": "AltoPIR로 동시 조회 {n}건을 모두 처리하는 시간에 맞춰 <b>{replayed}</b> 후에 결과를 표시했습니다. 이번 조회 한 건만 처리했을 때는 <b>{took}</b> 걸렸습니다.",
  "noAnswer": "데모 클라이언트에서 응답이 오지 않았습니다: ",
  "foot": "데이터: Have I Been Pwned의 <a href=\"https://haveibeenpwned.com/Passwords\">Pwned Passwords</a>, <a href=\"https://github.com/Phishing-Database/Phishing.Database\">Phishing.Database</a> (MIT), <a href=\"https://simple.wikipedia.org/\">Simple English Wikipedia</a> (CC BY-SA 4.0), <a href=\"https://github.com/facebookresearch/DPR\">DPR</a>의 Wikipedia 단락과 <a href=\"https://ai.google.com/research/NaturalQuestions\">Natural Questions</a> (CC BY-SA 3.0). Built with Llama. 코드: <a href=\"https://github.com/GyuyeongKim/AltoPIR-public\">AltoPIR</a>.",
  "rec.lede": "미리 기록한 조회를 재생하는 페이지입니다. 데모 클라이언트는 각 입력을 해시하거나 임베딩한 뒤 암호화했습니다. AltoPIR 서버는 데이터베이스의 모든 레코드를 사용해 답을 계산했고, 클라이언트만 답을 읽을 수 있었습니다. 각 결과를 보여 주기까지 걸리는 시간도 당시 서버에서 측정한 처리 속도에 맞췄습니다. 이 페이지에서는 서버로 아무것도 보내지 않습니다.",
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
  "rec.baselineNote.rag": "모든 방식의 서버 처리 시간은 Xeon Silver 4514Y 서버의 코어 16개로, 이 데이터베이스와 같은 모양의 데이터베이스에서 측정했습니다. Llama 3.1 8B는 NVIDIA A30 GPU에서 실행했습니다.",
  "rec.sizeNote.same": "같은 크기에서 측정",
  "rec.sizeNote.at8gb": "8 GB에서 측정",
  "rec.operatorNote": "Xeon Silver 4514Y 서버에서 연산에 코어 15개를 쓰고, 조회를 최대 128개씩 묶어 측정했습니다. AltoPIR 논문에서는 1 GB~64 GB 데이터베이스로 성능을 비교했습니다. 같은 코어를 사용했을 때 AltoPIR의 쿼리 처리량은 힌트 없이 동작하는 기존 방식 중 가장 빠른 방식의 24~50배였습니다.",
  "rec.error.notRecorded": "이 페이지에는 미리 기록한 조회만 있습니다. 위에서 하나를 선택하세요.",
  "scanTitle": "서버 내부 동작 · 느리게 재생",
  "scanSlow": "실제로 {time} 걸린 과정을 {x}배 느리게 재생",
  "scanIdle": "조회 기록을 고르면 서버가 데이터베이스를 읽는 과정을 볼 수 있습니다.",
  "scanQueries": "쿼리",
  "scanAnswers": "응답",
  "scanEach": "개당 {size}",
  "scanCores": "코어 {n}개",
  "scanBatch": "전체 {total}개 배치 중 {p}번째",
  "unitBucket": "버킷",
  "unitBuckets": "버킷 {n}개",
  "unitCluster": "클러스터",
  "unitClusters": "클러스터 {n}개",
  "step1Busy": "조회 {n}건이 한꺼번에 들어옵니다. 서버는 암호화한 쿼리 {q}개를 받아 최대 {max}개씩 배치로 묶어 처리합니다.",
  "step1One": "이번 조회만 들어옵니다. 서버는 암호화한 쿼리 1개를 받습니다.",
  "step1Many": "이번 조회만 들어옵니다. 서버는 암호화한 쿼리 {q}개를 받아 한 배치로 처리합니다.",
  "step2": "코어 {n}개가 데이터베이스를 나눠 처리합니다. 각 코어는 모든 {unit}에서 자신이 맡은 바이트 구간을 배치마다 한 번씩 읽고, 행렬 연산 장치로 읽은 데이터와 배치에 담긴 모든 쿼리를 곱합니다.",
  "step3": "쿼리마다 암호화한 응답을 하나씩 만듭니다. 서버는 쿼리마다 모든 {unit}에서 데이터를 같은 방식으로 읽으므로 무엇을 조회했는지 알 수 없습니다.",
  "step4": "자신이 보낸 쿼리의 응답은 {machine}만 복호화할 수 있습니다. 조회 결과는 {what}입니다.",
  "whatOne": "{ids} {unit}",
};
// text that comes with the recording, by its English original -> its key in KO
const KO_RECORDED = {
  "This page replays recorded lookups. A demo client hashed or embedded each input and encrypted it. An AltoPIR server computed over every record of its database, and only the client could read the answer. Each answer appears after the time it takes at the speeds measured on that server, and nothing you do here reaches a server.": "rec.lede",
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
  "Server time of every scheme measured on 16 cores of a Xeon Silver 4514Y server, on a database of this shape. Llama 3.1 8B ran on an NVIDIA A30 GPU.": "rec.baselineNote.rag",
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
    tabsAria: "Databases", dbPasswords: "Leaked passwords", dbUrls: "Unsafe links", dbDocs: "Wikipedia", dbRag: "RAG",
    labelPasswords: "Password", labelUrls: "Link", labelDocs: "Question", labelRag: "Question", check: "Check", search: "Search",
    ask: "Ask",
    show: "Show", hide: "Hide", revealAria: "Show the password",
    linePasswords: (m) => `${words(m.entries)} leaked passwords, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    lineUrls: (m) => `${words(m.entries)} phishing links and domains, ${int(m.rows)} buckets of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    lineDocs: (m) => `${words(m.passages)} passages from ${int(m.articles)} articles, ${int(m.rows)} clusters of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    lineRag: (m) => `${words(m.passages)} Wikipedia passages, ${int(m.rows)} clusters of ${bytes(m.record_bytes)}, ${bytes(m.db_bytes)} in all`,
    idlePasswords: "Type a password. A strong random one should not appear among the leaked passwords.",
    idleUrls: "Paste a link. The link, its host and up to four parent domains are checked together, in one batch of six requests.",
    idleDocs: "Ask a question. It is embedded here, and the nearest clusters of passages are fetched without the server learning which.",
    idleRag: (v) => "Ask a question. It is embedded here, and the nearest cluster of passages is fetched without the server learning which. " +
                    `${v.model} then answers from the five passages of the cluster closest to the question.`,
    notLoaded: "This database is not loaded on this demo.",
    try: "Try", recorded: "Recorded", pickBelow: "Pick a recorded lookup below", pickAbove: "Pick one of the recorded lookups above.",
    oneCoreOnly: "Measured on one core only, so it has no time for many lookups at once",
    server: "Server", busy: (v) => `Busy, ${v.n} lookups at once`, idle: "Idle, this lookup alone",
    thisServer: "this server", thisCpu: "this CPU",
    boardTitle: "Throughput against the fastest existing scheme",
    times: (v) => `${v.x}×`, moreThan: (v) => `more lookups per second than ${v.name}`,
    raceTitle: "Time to the answer", raceTitleRag: "Time to the first token", slower: (v) => `${v.r}× slower`,
    scanTitle: "Inside the server, slowed down", scanSlow: (v) => `${v.x}× slower than the ${v.time} it took`,
    scanIdle: "Pick a lookup to watch the server read the database.", scanQueries: "queries", scanAnswers: "answers",
    scanEach: (v) => `${v.size} each`, scanCores: (v) => `${v.n} cores`, scanBatch: (v) => `batch ${v.p} of ${v.total}`,
    unitBucket: "bucket", unitBuckets: (v) => `${v.n} buckets`, unitCluster: "cluster", unitClusters: (v) => `${v.n} clusters`,
    step1Busy: (v) => `${v.n} lookups arrive at once and send ${v.q} encrypted queries, taken in batches of up to ${v.max}.`,
    step1One: "This lookup alone sends one encrypted query.",
    step1Many: (v) => `This lookup alone sends ${v.q} encrypted queries, taken as one batch.`,
    step2: (v) => `${v.n} cores split the database: each core takes its own slice of every ${v.unit}, reads it once per batch, and multiplies it with all queries of the batch on its matrix engine.`,
    step3: (v) => `Every query gets its own encrypted answer. The server read every ${v.unit} the same way for each query, so it cannot tell which one was asked for.`,
    step4: (v) => `Only ${v.machine} can open the answers to its own queries: ${v.what}.`,
    whatOne: (v) => `${v.unit} ${v.ids}`,
    whyBusy: (v) => `${v.n} lookups like this one arrive together. Each scheme answers all of them at the throughput measured on ${v.where}, and its bar ends when the last one is done.`,
    whyIdleRace: (v) => `Only this lookup reaches the server. Each existing scheme takes the server time of its official implementation on ${v.where}, with the client and network time of the AltoPIR lookup.`,
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
    clusterOf: (v) => `cluster of ${int(v.rows)}`,
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
    searchAt: (v) => `Find the nearest cluster ${v.here}`, prefill: (v) => `${v.model} reads the passages`,
    ttft: "First token", modelAnswer: (v) => `Answer of ${v.model}`,
    timingTitleRag: "Time to the first token of the live question", timingTitleRagReplay: "Time to the first token of the recorded question",
    answeredInRag: (v) => `First token after <b>${ms(v.took)}</b> with AltoPIR.`,
    shownBusyOursRag: (v) => `Shown after <b>${ms(v.replayed)}</b>, when AltoPIR answers ${v.n} questions that arrive at once. This question alone had its first token after <b>${ms(v.took)}</b>.`,
    oneCore: "one core",
    answeredIn: (v) => `Answered in <b>${ms(v.took)}</b> with AltoPIR.`,
    shownBusyOurs: (v) => `Shown after <b>${ms(v.replayed)}</b>, when AltoPIR answers ${v.n} lookups that arrive at once. This lookup alone took <b>${ms(v.took)}</b>.`,
    noAnswer: "The demo client did not answer: ",
    foot: 'Data: <a href="https://haveibeenpwned.com/Passwords">Pwned Passwords</a> by Have I Been Pwned, ' +
          '<a href="https://github.com/Phishing-Database/Phishing.Database">Phishing.Database</a> (MIT), and ' +
          '<a href="https://simple.wikipedia.org/">Simple English Wikipedia</a> (CC BY-SA 4.0), Wikipedia passages of ' +
          '<a href="https://github.com/facebookresearch/DPR">DPR</a> and ' +
          '<a href="https://ai.google.com/research/NaturalQuestions">Natural Questions</a> (CC BY-SA 3.0). ' +
          'Built with Llama. Code: <a href="https://github.com/GyuyeongKim/AltoPIR-public">AltoPIR</a>.',
  },
  ko: {
    ...Object.fromEntries(Object.entries(KO).filter(([k, s]) => !k.startsWith("rec.") && !s.includes("{"))),
    langButton: "English",
    linePasswords: (m) => fill(KO.linePasswords, { entries: wordsKo(m.entries), rows: int(m.rows), record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    lineUrls: (m) => fill(KO.lineUrls, { entries: wordsKo(m.entries), rows: int(m.rows), record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    lineDocs: (m) => fill(KO.lineDocs, { passages: wordsKo(m.passages), articles: int(m.articles), rows: int(m.rows),
                                         record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    lineRag: (m) => fill(KO.lineRag, { passages: wordsKo(m.passages), rows: int(m.rows), record: bytes(m.record_bytes), db: bytes(m.db_bytes) }),
    clusterOf: (v) => fill(KO.clusterOf, { rows: int(v.rows) }), searchAt: (v) => fill(KO.searchAt, v),
    idleRag: (v) => fill(KO.idleRag, v), prefill: (v) => fill(KO.prefill, v), modelAnswer: (v) => fill(KO.modelAnswer, v),
    answeredInRag: (v) => fill(KO.answeredInRag, { took: ms(v.took) }),
    shownBusyOursRag: (v) => fill(KO.shownBusyOursRag, { replayed: ms(v.replayed), n: v.n, took: ms(v.took) }),
    busy: (v) => fill(KO.busy, v), whyBusy: (v) => fill(KO.whyBusy, v), whyIdleRace: (v) => fill(KO.whyIdleRace, v),
    times: (v) => fill(KO.times, v), moreThan: (v) => fill(KO.moreThan, v), slower: (v) => fill(KO.slower, v),
    scanSlow: (v) => fill(KO.scanSlow, v), scanEach: (v) => fill(KO.scanEach, v), scanCores: (v) => fill(KO.scanCores, v),
    scanBatch: (v) => fill(KO.scanBatch, v), unitBuckets: (v) => fill(KO.unitBuckets, v), unitClusters: (v) => fill(KO.unitClusters, v),
    step1Busy: (v) => fill(KO.step1Busy, v), step1Many: (v) => fill(KO.step1Many, v), step2: (v) => fill(KO.step2, v),
    step3: (v) => fill(KO.step3, v), step4: (v) => fill(KO.step4, v), whatOne: (v) => fill(KO.whatOne, v),
    seen: (v) => fill(KO.seen, { n: wordsKo(v.n) }), noneMatch: (v) => fill(KO.noneMatch, { n: wordsKo(v.n) }),
    noForm: (v) => fill(KO.noForm, { forms: v.n, entries: wordsKo(v.entries) }),
    numbers: (v) => fill(KO.numbers, v), bucketsOf: (v) => fill(KO.bucketsOf, { rows: int(v.rows) }),
    bytesUp: (v) => fill(KO.bytesUp, { n: int(v.n) }), bytesBack: (v) => fill(KO.bytesBack, { n: int(v.n) }),
    requests: (v) => fill(v.filler > 0 ? KO.requestsFiller : KO.requests, v),
    readInFull: (v) => (v.r > 1 ? KO.readInFull : KO.readInFullOne), readableOnly: (v) => fill(KO.readableOnly, v),
    embedAt: (v) => fill(KO.embedAt, v), encryptAt: (v) => fill(KO.encryptAt, v), decryptAt: (v) => fill(KO.decryptAt, v),
    answeredIn: (v) => fill(KO.answeredIn, { took: ms(v.took) }),
    shownBusyOurs: (v) => fill(KO.shownBusyOurs, { replayed: ms(v.replayed), n: v.n, took: ms(v.took) }),
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
  rag: {
    label: "labelRag", type: "text", button: "ask", reveal: false, examples: [],
    line: "lineRag", idle: "idleRag", name: "dbRag",
  },
};

let info = { databases: {}, operator: {} };
let ready = false;           // the databases are known and a tab is drawn
let serverMsg = { key: "connecting" };   // the masthead status, as a text key or as the server's own text
let current = "passwords";
let seq = 0, busy = false;   // only the latest request may render
let autoShown = false;       // an example password is shown until the user types their own
const CROWD = 100;   // lookups that arrive at once on a busy server
let crowd = true;    // busy by default; idle shows one lookup alone
let last = null;     // the lookup on screen, so the load switch can run it again
let race = null;     // the race on screen: its tab, start time, rows and animation frame
let boardDrawn = false;   // the throughput bars grow only when the page first draws them
let view = { kind: "idle" };   // what the answer box shows, so a language switch can draw it again
const sleep = (t) => new Promise((r) => setTimeout(r, t));
function baselines(db) { return ((info.operator || {}).baselines || {})[db]; }
// server time of k requests from a scheme's measured single-query time and throughput on this server
function estMs(s, k) { return s.qps ? Math.max(s.single_ms, 1000 * k / s.qps) : k * s.single_ms; }
// the client's own time of a lookup (with RAG, up to the model's first token) and the whole time it took
function clientMs(tm) { return tm.prepare_ms + tm.decrypt_ms + (tm.embed_ms || 0) + (tm.search_ms || 0) + (tm.prefill_ms || 0); }
function tookMs(tm) { return clientMs(tm) + tm.round_trip_ms; }
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
  last = null;
  $("wire").hidden = true;
  stopRace();
  stopScan();
  renderTab();
  renderView();
  renderRace();
  renderScan();
  renderBoard();
  $("input").focus({ preventScroll: true });
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

// a multiplier as the page writes it: whole numbers from 10 up, one decimal below
function times(x) { return x >= 9.95 ? String(Math.round(x)) : x.toFixed(1); }
function perSec(v) { return v < 10 ? v.toFixed(1) : int(v); }

// The time each scheme takes for a lookup, by one rule for all: the client work and network time of the recorded
// AltoPIR lookup plus the scheme's server time for its requests (times the lookups that arrive at once), from the
// single-query time and throughput measured on this server. AltoPIR alone on an idle server is the recorded lookup itself.
// A scheme measured on one core only has no time for many lookups at once (null).
function raceTimes(db, d, many) {
  const tm = d.wire.time, k = d.wire.requests, n = many ? CROWD : 1, client = clientMs(tm);
  const own = ((info.operator || {}).databases || {})[db], b = baselines(db);
  const network = own ? Math.max(0, tm.round_trip_ms - estMs(own, k)) : 0;
  const rows = [{ name: "AltoPIR", ours: true, ms: many && own ? client + network + estMs(own, k * n) : tookMs(tm) }];
  for (const [name, s] of Object.entries(b ? b.schemes : {}))
    rows.push({ name, ms: many && !s.qps ? null : client + network + estMs(s, k * n) });
  // AltoPIR first, then the others in the order they finish
  return [rows[0], ...rows.slice(1).sort((x, y) => (x.ms ?? Infinity) - (y.ms ?? Infinity))];
}

function stopRace() {
  if (race && race.frame) cancelAnimationFrame(race.frame);
  race = null;
}
function startRace(db, d, many, t0) {
  stopRace();
  race = { db, t0, many, rows: raceTimes(db, d, many), frame: 0 };
  renderRace();
  startScan(db, d, many, t0);
}
// a clock that runs while a scheme answers: milliseconds up to a second, then tenths of a second
function clock(e) { return e < 1000 ? Math.round(e) + " ms" : (e / 1000).toFixed(1) + " s"; }

// one bar of the race at e milliseconds after the click: it fills over the scheme's own time, then shows that time and,
// for an existing scheme, how many times longer it took than AltoPIR (written from the start but shown only then, so the
// columns keep their width)
function paintRow(r, e) {
  if (r.ms == null || !r.fill) return false;
  const f = Math.min(1, e / r.ms), done = f >= 1;
  r.fill.style.width = (100 * f).toFixed(2) + "%";
  if (!done) { r.t.textContent = clock(e); return true; }
  if (!r.painted) { r.painted = true; r.el.classList.add("done"); r.t.textContent = ms(r.ms); }
  return false;
}
function tick() {
  if (!race) return;
  const e = performance.now() - race.t0;
  let running = false;
  for (const r of race.rows) running = paintRow(r, e) || running;
  race.frame = running ? requestAnimationFrame(tick) : 0;
}

// the race box of the current tab: the load switch, one bar per scheme, and what the bars mean
function renderRace() {
  const db = current, b = baselines(db), box = $("race");
  if (!b) { box.hidden = true; return; }
  box.hidden = false;
  $("race-title").textContent = t(db === "rag" ? "raceTitleRag" : "raceTitle");
  const load = $("load");
  load.replaceChildren(el("span", {}, t("server")));
  for (const [label, value] of [[t("busy", { n: CROWD }), true], [t("idle"), false]]) {
    const btn = el("button", { type: "button", "aria-pressed": String(value === crowd) }, label);
    btn.onclick = () => {
      if (crowd === value || busy) return;   // a lookup on its way keeps the load it started with
      crowd = value;
      if (last && last.db === current) { $("input").value = last.input; submit(); } else renderRace();
    };
    load.append(btn);
  }
  // before the first lookup the bars stand empty; a race in progress is drawn again where it is
  const live = race && race.db === db ? race : null;
  const rows = live ? live.rows : [{ name: "AltoPIR", ours: true },
    ...Object.entries(b.schemes).map(([name, s]) => ({ name, ms: crowd && !s.qps ? null : undefined }))];
  const box2 = $("race-rows");
  box2.replaceChildren();
  const ours = rows[0].ms;
  for (const r of rows) {
    const x = r.ms && ours ? r.ms / ours : 0;
    r.fill = el("span");
    r.t = el("span", { class: "t" });
    r.x = el("span", { class: "x" }, !r.ours && x >= 1.095 ? t("slower", { r: times(x) }) : "");
    r.x.hidden = !r.x.textContent;
    r.painted = false;
    const n = el("span", { class: "n" }, r.name), v = el("span", { class: "v" }, r.t, r.x);
    r.el = el("div", { class: "rrow" + (r.ours ? " ours" : "") + (r.ms === null ? " na" : "") },
      n, el("span", { class: "b" }, r.ms === null ? null : r.fill), v);
    if (r.ms === null) { r.t.textContent = t("oneCore"); n.title = v.title = t("oneCoreOnly"); }
    box2.append(r.el);
  }
  $("race-why").textContent = crowd ? t("whyBusy", { n: CROWD, where: where() }) : t("whyIdleRace", { where: where() });
  $("race-cap").textContent = tx(b.note);
  if (live) {
    if (live.frame) cancelAnimationFrame(live.frame);
    tick();
  }
}

// the throughput of AltoPIR against the fastest existing scheme on each database, as a multiplier and two bars
function renderBoard() {
  const dbs = (info.operator || {}).databases || {}, tiles = $("tiles");
  const grow = !boardDrawn;
  tiles.replaceChildren();
  for (const db of ["passwords", "urls", "rag", "docs"]) {
    const o = dbs[db], b = baselines(db);
    if (!o || !b || !info.databases[db]) continue;
    const best = Object.entries(b.schemes).filter(([, s]) => s.qps).sort((x, y) => y[1].qps - x[1].qps)[0];
    if (!best) continue;
    const [name, s] = best;
    const bar = (cls, n, v) => el("span", { class: "mrow" + cls }, el("span", { class: "n" }, n),
      el("span", { class: "b" }, el("span", { style: `width:${Math.max(1, 100 * v / o.qps).toFixed(2)}%` })),
      el("span", { class: "v" }, perSec(v)));
    const tile = el("button", { type: "button", class: "tile", "aria-pressed": String(db === current) },
      el("span", { class: "app" }, t(DBS[db].name)),
      el("span", { class: "x" }, t("times", { x: times(o.qps / s.qps) })),
      el("span", { class: "than" }, t("moreThan", { name })),
      el("span", { class: "mini" }, bar(" ours", "AltoPIR", o.qps), bar("", name, s.qps)));
    tile.onclick = () => {
      if (db !== current) setTab(db);
      document.querySelector(".tabs").scrollIntoView({ behavior: "smooth", block: "start" });
    };
    tiles.append(tile);
  }
  $("board").hidden = !tiles.children.length;
  if (tiles.children.length && grow) {
    boardDrawn = true;
    $("board").classList.add("grow");
    setTimeout(() => $("board").classList.remove("grow"), 1600);
  }
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
  const wiki = db === "rag" ? "https://en.wikipedia.org/wiki/" : "https://simple.wikipedia.org/wiki/";
  for (const p of d.passages) {
    const title = p.title.normalize("NFC"), href = wiki + encodeURIComponent(title.replace(/ /g, "_"));
    list.append(el("li", {}, el("p", { class: "t" }, el("a", { href, target: "_blank", rel: "noopener" }, title)),
                   el("p", { class: "x" }, p.text.normalize("NFC"))));
  }
  if (db !== "rag") return list;
  return el("div", {}, el("div", { class: "verdict model" }, el("p", { class: "label" }, t("modelAnswer", m)),
                          el("p", { class: "say" }, d.answer)), list);
}

// the answer box: the tab's hint, the answer with its note, or an error
function renderView() {
  const v = view, box = $("answer");
  if (v.kind === "idle") {
    box.replaceChildren(el("p", { class: "placeholder" }, t(info.replay ? "pickAbove" : DBS[current].idle, info.databases[current])));
  } else if (v.kind === "error") {
    box.replaceChildren(el("p", { class: "error" }, t("noAnswer") + v.message));
  } else {
    const { db, d } = v;
    box.replaceChildren(renderAnswer(db, d));
    // the note goes above a list of passages, which can run below the fold, and under a short verdict
    if (d.wire && !d.error) {
      const took = tookMs(d.wire.time), rag = db === "rag" ? "Rag" : "";
      const vars = { replayed: v.replayed, took, n: CROWD };
      const note = v.replayed == null ? t("answeredIn" + rag, vars) : t("shownBusyOurs" + rag, vars);
      const top = db === "docs" || db === "rag";
      box[top ? "prepend" : "append"](el("p", { class: "replayed" + (top ? " top" : ""), html: note }));
    }
    if (d.wire) renderWire(db, v.input, d); else $("wire").hidden = true;
  }
}

function renderWire(db, input, d) {
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
  item(stayed, buckets, t(db === "rag" ? "clusterOf" : "bucketsOf", { k: w.local.length, rows: w.rows }), "n");
  item(sent, t("bytesUp", { n: w.up_bytes }), t("requests", { r: w.requests, filler: w.requests - w.local.length }), "n");
  item(sent, bytes(w.rows * w.record_bytes), t("readInFull", { r: w.requests }), "n");
  item(sent, t("bytesBack", { n: w.down_bytes }), t("readableOnly", { machine: machine() }), "n");

  // the recorded RAG questions carry no request bytes, so their panel has no preview
  $("hex").closest("figure").hidden = !w.preview_hex;
  if (w.preview_hex) $("hex").textContent = w.preview_hex.match(/.{1,8}/g).join(" ").match(/.{1,36}/g).join("\n");
  const replay = info.replay;
  $("timing-title").textContent = t(db === "rag" ? (replay ? "timingTitleRagReplay" : "timingTitleRag")
                                                 : (replay ? "timingTitleReplay" : "timingTitle"));

  const tm = w.time, parts = [];
  if (tm.embed_ms != null) parts.push([t("embedAt", { here: here() }), tm.embed_ms, "var(--t-embed)"]);
  if (tm.search_ms != null) parts.push([t("searchAt", { here: here() }), tm.search_ms, "var(--t-embed)"]);
  parts.push([t("encryptAt", { here: here() }), tm.prepare_ms, "var(--t-encrypt)"]);
  parts.push([t("network"), tm.round_trip_ms, "var(--accent)"]);
  parts.push([t("decryptAt", { here: here() }), tm.decrypt_ms, "var(--t-decrypt)"]);
  if (tm.prefill_ms != null) parts.push([t("prefill", m), tm.prefill_ms, "var(--t-model)"]);
  const total = parts.reduce((s, p) => s + p[1], 0);
  $("bar").replaceChildren(...parts.map(([n, v, c]) => el("span", { style: `width:${(100 * v / total).toFixed(2)}%;background:${c}`, title: n })));
  const lg = $("legend");
  lg.replaceChildren();
  for (const [n, v, c] of parts) lg.append(el("dt", { style: `background:${c}` }), el("dd", {}, n), el("dd", { class: "v" }, ms(v)));
  lg.append(el("dt", { class: "total" }), el("dd", { class: "total" }, t(db === "rag" ? "ttft" : "total")), el("dd", { class: "v total" }, ms(total)));
  last = { db, d, input };
  $("wire").hidden = false;
}

// The scan of one lookup, slowed down. It follows the engine (src/amx_kernels.cpp, sw_scan_impl): the database is a
// matrix with one row per record and one column per byte of a record, and the query of a client holds one value per
// record. The cores split the byte columns, so each core owns the same slice of every record. Each core walks down the
// records one block at a time, and inside a block it sweeps its columns from left to right, multiplying them with the
// block of every query of the batch (up to the batch limit). One batch reads the whole database once; a busy server
// reads it once per batch. Then every query gets its answer, and only the client can open the record it asked for.
// cores behind each database's numbers: 15 engine cores for passwords, links and Wikipedia (rec.operatorNote), 16 for
// RAG (rec.baselineNote.rag)
const SCAN_CORES = { passwords: 15, urls: 15, docs: 15, rag: 16 };
const SCAN_UNIT = { passwords: "Bucket", urls: "Bucket", docs: "Cluster", rag: "Cluster" };
// the pace of the drawing, not a measurement: the batch forms, each batch is read and answered, the client opens its answer
const SCAN_FORM = 700, SCAN_READ = 2800, SCAN_SEND = 450, SCAN_OPEN = 500;
const SCAN_BLOCKS = 12, SCAN_STRIPES = 36;   // drawn blocks of records, and drawn record stripes
let scan = null;   // the scan on screen: its plan, start time and animation frame

function scanPlan(db, d, many) {
  const m = info.databases[db], own = ((info.operator || {}).databases || {})[db];
  const k = d.wire.requests, n = many ? CROWD : 1, q = k * n, max = (own && own.batch) || q;
  const batches = [];
  for (let left = q; left > 0; left -= max) batches.push(Math.min(max, left));
  const read = SCAN_READ / batches.length, send = Math.min(SCAN_SEND, 1200 / batches.length);
  return { db, many, q, max, batches, read, send, rows: m.rows, record: m.record_bytes, cores: SCAN_CORES[db] || 1,
           real: own ? estMs(own, q) : null, mine: d.wire.local.map((x) => x.bucket),
           end: SCAN_FORM + batches.length * (read + send) + SCAN_OPEN };
}
// where the drawing is e milliseconds after the click: phase 0 the batch forms, 1 a batch is read, 2 it is answered,
// 3 the client opens its answer; f is the progress inside the phase and b the batch
function scanAt(p, e) {
  if (e < SCAN_FORM) return { phase: 0, f: e / SCAN_FORM, b: 0 };
  let t = e - SCAN_FORM;
  for (let b = 0; b < p.batches.length; b++) {
    if (t < p.read) return { phase: 1, f: t / p.read, b };
    t -= p.read;
    if (t < p.send) return { phase: 2, f: t / p.send, b };
    t -= p.send;
  }
  return { phase: 3, f: Math.min(1, t / SCAN_OPEN), b: p.batches.length - 1 };
}
function stopScan() {
  if (scan && scan.frame) cancelAnimationFrame(scan.frame);
  scan = null;
}
function startScan(db, d, many, t0) {
  stopScan();
  scan = { plan: scanPlan(db, d, many), t0, frame: 0, phase: -1 };
  renderScan();
}
function scanTick() {
  if (!scan) return;
  const e = performance.now() - scan.t0, st = scanAt(scan.plan, e);
  drawScan(st);
  if (st.phase !== scan.phase) {
    scan.phase = st.phase;
    $("scan-steps").querySelectorAll("li").forEach((li, i) => {
      li.className = i < st.phase ? "done" : i === st.phase ? "on" : "";
    });
  }
  scan.frame = e < scan.plan.end + 50 ? requestAnimationFrame(scanTick) : 0;
}
function andList(xs) { return xs.length < 2 ? xs.join("") : xs.slice(0, -1).join(", ") + " and " + xs[xs.length - 1]; }

// the card of the current tab: before a lookup the database stands still; a lookup fills in the steps and plays
function renderScan() {
  const db = current, m = info.databases[db], box = $("scan");
  if (!m || !m.rows || !baselines(db)) { box.hidden = true; return; }
  box.hidden = false;
  $("scan-title").textContent = t("scanTitle");
  const p = scan && scan.plan.db === db ? scan.plan : null, u = SCAN_UNIT[db];
  $("scan-hint").textContent = t("scanIdle");
  $("scan-hint").hidden = !!p;
  const reading = p ? p.batches.length * (p.read + p.send) : 0;
  $("scan-slow").textContent = p && p.real ? t("scanSlow", { x: times(reading / p.real), time: ms(p.real) }) : "";
  const steps = $("scan-steps");
  steps.replaceChildren();
  if (p) {
    const unit = t("unit" + u), ids = p.mine.map(int);
    const what = t("whatOne", { unit: lang === "en" && ids.length > 1 ? unit + "s" : unit,
                                ids: lang === "ko" ? ids.map((x) => x + "번").join(", ") : andList(ids) });
    for (const s of [p.many ? t("step1Busy", { n: CROWD, q: int(p.q), max: p.max })
                       : p.q === 1 ? t("step1One") : t("step1Many", { q: p.q }),
                     t("step2", { n: p.cores, unit }), t("step3", { unit }), t("step4", { machine: machine(), what })])
      steps.append(el("li", {}, s));
  }
  if (scan) scan.phase = -1;
  if (scan && scan.plan.db === db) {
    if (scan.frame) cancelAnimationFrame(scan.frame);
    scanTick();
  } else drawScan(null);
}

function drawScan(st) {
  const cv = $("scan-canvas"), W = cv.clientWidth, H = cv.clientHeight;
  if (!W || !H || $("scan").hidden) return;
  const dpr = window.devicePixelRatio || 1;
  if (cv.width !== Math.round(W * dpr) || cv.height !== Math.round(H * dpr)) {
    cv.width = Math.round(W * dpr);
    cv.height = Math.round(H * dpr);
  }
  const g = cv.getContext("2d");
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, W, H);
  const css = getComputedStyle(document.documentElement), v = (n) => css.getPropertyValue(n).trim();
  const C = { todo: v("--scan-todo"), read: v("--scan-read"), on: v("--scan-on"), mark: v("--scan-mark"),
              ink: v("--ink"), ink3: v("--ink-3"), font: v("--sans") };
  const db = current, m = info.databases[db], p = scan && scan.plan.db === db ? scan.plan : null;
  if (!p) st = null;
  const side = W < 360 ? 34 : 44, gap = W < 360 ? 10 : 14, top = 34, bottom = 22;
  const dbx = side + gap, dbw = W - 2 * (side + gap), ax = dbx + dbw + gap, y0 = top, h = H - top - bottom;
  const pitch = h / SCAN_STRIPES, sh = Math.max(1, pitch - (pitch > 3 ? 1 : 0.5));
  const block = (s) => Math.floor(s * SCAN_BLOCKS / SCAN_STRIPES);
  const reading = st && st.phase === 1, cur = reading ? Math.floor(st.f * SCAN_BLOCKS) : -1;
  const sweep = reading ? st.f * SCAN_BLOCKS - cur : 0;
  const allRead = st && st.phase >= 2;

  // labels: the queries and answers of the batch on the sides, the records of the database in the middle
  const text = (s, x, y, align, weight, size, color) => {
    g.font = `${weight} ${size}px ${C.font}`;
    g.textAlign = align;
    g.fillStyle = color;
    g.fillText(s, x, y);
  };
  const B = p ? p.batches[st.b] : 0, shown = st && st.phase === 0 ? Math.round(st.f * B) : B;
  if (p) {
    text(int(shown), side / 2, 12, "center", 600, 12, C.ink);
    text(t("scanQueries"), side / 2, 26, "center", 400, 11, C.ink3);
    text(st.phase >= 1 ? int(B) : "", ax + side / 2, 12, "center", 600, 12, C.ink);
    text(t("scanAnswers"), ax + side / 2, 26, "center", 400, 11, C.ink3);
  }
  text(t("unit" + SCAN_UNIT[db] + "s", { n: int(m.rows) }), dbx + dbw / 2, 12, "center", 600, 12, C.ink);
  text(t("scanEach", { size: bytes(m.record_bytes) }), dbx + dbw / 2, 26, "center", 400, 11, C.ink3);
  const cores = p ? p.cores : SCAN_CORES[db] || 1;
  text(t("scanCores", { n: cores }), dbx, H - 6, "left", 400, 11, C.ink3);
  if (p && p.batches.length > 1 && st.phase >= 1)
    text(t("scanBatch", { p: st.b + 1, total: p.batches.length }), dbx + dbw, H - 6, "right", 600, 11, C.ink);

  // the database: one column strip per core, record stripes across them
  const cg = cores > 1 ? 2 : 0, cw = (dbw - cg * (cores - 1)) / cores;
  for (let s = 0; s < SCAN_STRIPES; s++) {
    const y = y0 + s * pitch, bl = block(s);
    for (let c = 0; c < cores; c++) {
      const x = dbx + c * (cw + cg);
      g.fillStyle = allRead || bl < cur ? C.read : C.todo;
      g.fillRect(x, y, cw, sh);
      if (bl === cur) { g.fillStyle = C.on; g.fillRect(x, y, cw * sweep, sh); }
    }
  }
  // the queries and the answers of the batch, one thin column each (at most 20 drawn)
  const cols = (x, n, paint) => {
    const k = Math.max(1, Math.min(20, n)), w = (side - (k - 1)) / k;
    for (let i = 0; i < k; i++) paint(x + i * (w + 1), w, i, k);
  };
  if (p) {
    const nq = st.phase === 0 ? Math.max(0, shown) : B;
    cols(0, B, (x, w, i, k) => {
      if (i >= Math.ceil(nq * k / B)) return;
      for (let s = 0; s < SCAN_STRIPES; s++) {
        g.fillStyle = block(s) === cur ? C.on : C.todo;
        g.fillRect(x, y0 + s * pitch, w, sh);
      }
    });
    if (st.phase >= 1) cols(ax, B, (x, w) => {
      const full = st.phase === 1 ? st.f : 1, away = st.phase === 2 ? st.f : 0;
      g.globalAlpha = st.phase === 2 ? 1 - 0.7 * away : 1;
      g.fillStyle = st.phase === 1 ? C.read : C.on;
      g.fillRect(x + 6 * away, y0, w, h * full);
      g.globalAlpha = 1;
    });
  } else {
    for (const x of [0, ax]) cols(x, 20, (cx, w) => { g.fillStyle = C.todo; g.globalAlpha = 0.5; g.fillRect(cx, y0, w, h); g.globalAlpha = 1; });
  }
  // the client opens its answer: its own records, as the amber stripe of the logo
  if (st && st.phase === 3) {
    g.globalAlpha = st.f;
    for (const id of p.mine) {
      const y = y0 + ((id + 0.5) / m.rows) * h;
      g.fillStyle = C.mark;
      g.fillRect(dbx - 3, y - 2, dbw + 6, 4);
      g.font = `700 11px ${C.font}`;
      const label = int(id), tw = g.measureText(label).width;
      g.fillStyle = C.mark;
      g.fillRect(dbx + dbw - tw - 10, y - 9, tw + 10, 18);
      text(label, dbx + dbw - 5, y + 4, "right", 700, 11, v("--scan-mark-ink"));
    }
    g.globalAlpha = 1;
  }
}

// One lookup, raced: every scheme's bar starts with the click. AltoPIR's answer appears when its own bar ends (on a
// busy server, when it has answered all the lookups that arrived at once); the other bars keep running to their times.
async function submit(ev) {
  if (ev) ev.preventDefault();
  const raw = $("input").value;
  const input = current === "passwords" ? raw : raw.trim();   // a password is checked exactly as typed
  if (!input || !info.databases[current] || busy) return;
  const db = current, mine = ++seq, many = crowd, t0 = performance.now();
  busy = true;
  stopRace();
  renderRace();
  stopScan();
  renderScan();
  document.body.classList.add("loading");
  $("go").disabled = true;
  try {
    // the recorded copy knows the lookup at once, so its race starts with the click; a live lookup starts it on arrival
    const early = window.altopirReplay ? await window.altopirReplay.peek(db, input) : null;
    if (early && early.wire && mine === seq && db === current) startRace(db, early, many, t0);
    const r = await fetch("/api/" + db, { method: "POST", headers: { "Content-Type": "application/json" },
                                          body: JSON.stringify({ input }) });
    const d = await r.json();
    if (mine !== seq || db !== current) return;
    let replayed = null;
    if (d.wire && baselines(db)) {
      if (!race) startRace(db, d, many, t0);
      const ours = race.rows[0].ms, left = ours - (performance.now() - t0);
      if (many) replayed = ours;
      if (left > 0) await sleep(left);
      if (mine !== seq || db !== current) return;
    }
    view = { kind: "answer", db, d, input, many, replayed };
    renderView();
  } catch (e) {
    if (mine !== seq) return;
    view = { kind: "error", message: e.message };
    renderView();
    $("wire").hidden = true;
    stopRace();
    renderRace();
    stopScan();
    renderScan();
  } finally {
    busy = false;
    document.body.classList.remove("loading");
    $("go").disabled = !info.databases[current];
  }
}

// every text on the page in the current language; the tab, the load switch, a race and a shown answer keep their state
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
  if (!ready) return;
  renderTab();
  renderView();
  // the panel of an earlier lookup stays on screen while the next one runs
  if (view.kind !== "answer" && last && !$("wire").hidden) renderWire(last.db, last.input, last.d);
  renderRace();
  renderScan();
  renderBoard();
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
  if (ready) renderScan();   // the drawing takes its colors from the theme
};
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { if (ready) renderScan(); });
if (window.ResizeObserver) new ResizeObserver(() => { if (ready && !(scan && scan.frame)) drawScan(scan ? scanAt(scan.plan, performance.now() - scan.t0) : null); }).observe($("scan-canvas"));
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
