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
// a Korean count with its counter word, which Korean spaces after 억 and 만
function countKo(n, unit, bold) {
  const s = wordsKo(n);
  return (bold ? `<b>${s}</b>` : s) + (/[억만]$/.test(s) ? " " : "") + unit;
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
    title: "AltoPIR · 프라이빗 조회",
    tag: "CPU 서버에서 하는 프라이빗 조회",
    langButton: "English", langAria: "언어 바꾸기",
    dark: "다크", light: "라이트", themeAria: "색 테마 바꾸기",
    connecting: "서버에 연결하는 중", connected: "AltoPIR 서버에 연결됨",
    notRunning: "데모 client가 실행 중이 아닙니다",
    h1: "무엇을 묻는지 알리지 않고 데이터베이스에 물어보세요.",
    lede: "입력한 내용은 이 기기 밖으로 나가지 않습니다. 여기서 hash하고 암호화하면 서버는 데이터베이스의 모든 레코드에 대해 " +
          "계산하고, 답은 이 기기만 읽을 수 있는 형태로 돌아옵니다.",
    tabsAria: "데이터베이스", dbPasswords: "유출된 비밀번호", dbUrls: "위험한 링크", dbDocs: "Wikipedia",
    labelPasswords: "비밀번호", labelUrls: "링크", labelDocs: "질문", check: "확인", search: "검색",
    show: "보기", hide: "숨기기", revealAria: "비밀번호 보기",
    linePasswords: (m) => `유출된 비밀번호 ${countKo(m.entries, "개")}, ${bytes(m.record_bytes)} bucket ${int(m.rows)}개, 전체 ${bytes(m.db_bytes)}`,
    lineUrls: (m) => `피싱 링크와 도메인 ${countKo(m.entries, "개")}, ${bytes(m.record_bytes)} bucket ${int(m.rows)}개, 전체 ${bytes(m.db_bytes)}`,
    lineDocs: (m) => `문서 ${int(m.articles)}개의 passage ${countKo(m.passages, "개")}, ${bytes(m.record_bytes)} cluster ${int(m.rows)}개, 전체 ${bytes(m.db_bytes)}`,
    idlePasswords: "비밀번호를 입력하세요. 강력한 무작위 비밀번호라면 유출된 비밀번호에 없을 것입니다.",
    idleUrls: "링크를 붙여 넣으세요. 링크와 그 host, 상위 도메인 최대 네 개를 요청 여섯 개로 된 batch 하나로 함께 확인합니다.",
    idleDocs: "질문을 입력하세요. 질문은 여기서 embedding되고, 가장 가까운 passage cluster를 서버가 어느 것인지 모르게 가져옵니다.",
    notLoaded: "이 데모에는 이 데이터베이스가 올라와 있지 않습니다.",
    try: "예시", recorded: "기록된 조회", pickBelow: "아래에서 기록된 조회를 고르세요", pickAbove: "위에서 기록된 조회 하나를 고르세요.",
    answerWith: "응답 scheme", oneCoreOnly: "코어 하나에서만 측정해서 여러 조회를 한꺼번에 처리하는 시간이 없습니다",
    server: "서버", busy: (v) => `바쁨, 조회 ${v.n}개가 한꺼번에`, idle: "한가함, 이 조회 하나만",
    thisServer: "이 서버", thisCpu: "이 CPU",
    whyBusy: (v) => `이런 조회 ${v.n}개가 한꺼번에 들어옵니다. 각 scheme은 ${v.where}에서 측정한 throughput으로 모두 처리하고, 마지막 조회가 끝나야 답이 나타납니다.`,
    whyIdle: (v) => `답은 같은 데이터베이스에서 나오며, ${v.where}에서 ${v.scheme}에 필요한 시간이 지난 뒤에 나타납니다. ` +
                    (v.replay ? "이 시간은 공식 구현으로 직접 측정했습니다." : "이 시간은 공식 구현으로 측정한 값입니다."),
    found: "유출된 비밀번호에 있습니다",
    seen: (v) => `유출 데이터에서 약 ${countKo(v.n, "번", true)} 나왔습니다.`,
    notFound: "유출된 비밀번호에 없습니다",
    noneMatch: (v) => `유출된 비밀번호 ${countKo(v.n, "개", true)} 중 일치하는 것이 없습니다.`,
    notListed: "피싱 목록에 없습니다",
    noForm: (v) => `이 링크의 형태 <b>${v.n}</b>가지 모두 피싱 링크와 도메인 ${countKo(v.entries, "개", true)}에 들어 있지 않습니다.`,
    listed: "피싱으로 등록되어 있습니다",
    wireTitle: "이 기기를 떠난 것", wireTitleReplay: "client를 떠난 것",
    stayedTitle: "여기 남은 것", stayedTitleReplay: "client에 남은 것", received: "서버가 받은 것",
    yourPassword: "입력한 비밀번호", itsSha1: "그 SHA-1", yourLink: "입력한 링크", yourQuestion: "입력한 질문",
    numbers: (v) => `숫자 ${v.n}개`, itsEmbedding: "그 embedding",
    bucketsOf: (v) => `bucket 번호, 전체 ${int(v.rows)}개 중`,
    bytesUp: (v) => `${int(v.n)} bytes`,
    requests: (v) => `암호화된 요청 ${v.r}개` + (v.filler > 0 ? `, 그중 ${v.filler}개는 무작위라서 모든 링크가 ${v.r}개를 보냄` : ""),
    readInFull: (v) => (v.r > 1 ? "요청마다 전부 읽음" : "요청 하나에 전부 읽음"),
    bytesBack: (v) => `${int(v.n)} bytes 응답`, readableOnly: (v) => `${v.machine}에서만 읽을 수 있음`,
    hexCap: "요청에 담긴 암호화된 key의 앞 96 bytes입니다. 나머지도 서버에게는 똑같이 보입니다.",
    timingTitle: "실제 AltoPIR 조회 시간", timingTitleReplay: "기록된 AltoPIR 조회 시간",
    here: "여기서", hereReplay: "client에서", machine: "이 기기", machineReplay: "client",
    embedAt: (v) => `${v.here} 질문 embedding`, encryptAt: (v) => `${v.here} 암호화`, network: "네트워크와 서버",
    decryptAt: (v) => `${v.here} 복호화`, total: "합계",
    compareBusy: (v) => `조회 ${v.n}개를 한꺼번에 처리하는 서버 시간`, compareIdle: "이 조회 하나만 처리하는 서버 시간",
    compareTitle: "이 조회의 서버 시간", qpsTitle: "서버 한 대의 초당 조회 수", oneCore: "코어 하나",
    answering: (v) => `${v.scheme} 응답 중`,
    waitBusy: (v) => `${v.where}에서 측정한 throughput으로 조회 ${v.n}개를 한꺼번에 처리할 때 모두 ${ms(v.target)}`,
    waitIdle: (v) => `${v.where}에서 측정한 속도로 모두 ${ms(v.target)}`,
    answeredIn: (v) => `AltoPIR로 <b>${ms(v.took)}</b> 만에 답했습니다.`,
    shownIdle: (v) => `${v.scheme}로 이 조회에 걸리는 시간인 <b>${ms(v.replayed)}</b> 뒤에 보여 줍니다. AltoPIR로는 <b>${ms(v.took)}</b> 걸렸습니다.`,
    shownBusyOurs: (v) => `조회 ${v.n}개가 한꺼번에 들어올 때 AltoPIR가 답하는 시간인 <b>${ms(v.replayed)}</b> 뒤에 보여 줍니다. 이 조회 하나만으로는 <b>${ms(v.took)}</b> 걸렸습니다.`,
    shownBusy: (v) => `조회 ${v.n}개가 한꺼번에 들어올 때 ${v.scheme}의 응답 시간인 <b>${ms(v.replayed)}</b> 뒤에 보여 줍니다. AltoPIR는 <b>${ms(v.oursBusy)}</b> 만에 답합니다.`,
    noAnswer: "데모 client가 응답하지 않았습니다: ",
    opTitle: "서버 한 대로 하루 종일", perSecond: "초당 조회 수", perDay: "하루 조회 수", alone: "조회 하나만",
    count: (v) => wordsKo(v.n), database: (v) => `${bytes(v.n)} 데이터베이스`,
    fastest: (v) => `가장 빠른 기존 scheme(${v.name}${v.size ? ", " + v.size : ""})은 초당 ${v.qps}회 조회합니다.`,
    foot: '데이터: Have I Been Pwned의 <a href="https://haveibeenpwned.com/Passwords">Pwned Passwords</a>, ' +
          '<a href="https://github.com/Phishing-Database/Phishing.Database">Phishing.Database</a> (MIT), ' +
          '<a href="https://simple.wikipedia.org/">Simple English Wikipedia</a> (CC BY-SA 4.0). ' +
          '코드: <a href="https://github.com/GyuyeongKim/AltoPIR-public">AltoPIR</a>.',
    // text that comes with the recording (replay.json) or from the demo server, by its English original
    recordedText: {
      "This page replays lookups recorded on 28 October 2026. A demo client hashed or embedded each input and encrypted it. An AltoPIR server computed over every record of its database, and only the client could read the answer. Each answer appears after the time it takes at the speeds measured on that server, and nothing you do here reaches a server.":
        "이 페이지는 2026년 10월 28일에 기록한 조회를 재생합니다. 데모 client가 입력마다 hash하거나 embedding한 뒤 암호화했습니다. AltoPIR 서버는 데이터베이스의 모든 레코드에 대해 계산했고, 답은 client만 읽을 수 있었습니다. 각 답은 그 서버에서 측정한 속도로 걸리는 시간이 지난 뒤에 나타나며, 여기서 하는 어떤 동작도 서버에 닿지 않습니다.",
      "a random one": "무작위 비밀번호",
      "a phishing link": "피싱 링크", "a phishing domain": "피싱 도메인", "a fake sign-in page": "가짜 로그인 페이지",
      "a hosted phishing page": "호스팅된 피싱 페이지", "a Wikipedia page": "Wikipedia 페이지", "a news site": "뉴스 사이트",
      "a code repository": "코드 저장소",
      "phishing link": "피싱 링크", "phishing domain": "피싱 도메인",
      "Server time measured on this server: AltoPIR on this database; the others with their official implementations on a 1 GB database with their 32 KB records, smaller than the 128 KB records here.":
        "이 서버에서 측정한 서버 시간입니다. AltoPIR는 이 데이터베이스에서 측정했고, 다른 scheme은 공식 구현으로 1 GB 데이터베이스와 각자의 32 KB 레코드에서 측정했습니다. 이 레코드는 여기의 128 KB 레코드보다 작습니다.",
      "Server time measured on this server: AltoPIR on this database; the others with their official implementations at 8 GB and 32 KB records (this database is 11.25 GB, so they would take longer).":
        "이 서버에서 측정한 서버 시간입니다. AltoPIR는 이 데이터베이스에서 측정했고, 다른 scheme은 공식 구현으로 8 GB와 32 KB 레코드에서 측정했습니다. 이 데이터베이스는 11.25 GB라서 실제로는 더 오래 걸릴 것입니다.",
      "Server time measured on this server: AltoPIR on this database; the others with their official implementations on a 64 MB database with their 32 KB records.":
        "이 서버에서 측정한 서버 시간입니다. AltoPIR는 이 데이터베이스에서 측정했고, 다른 scheme은 공식 구현으로 64 MB 데이터베이스와 각자의 32 KB 레코드에서 측정했습니다.",
      "same size": "같은 크기", "measured at 8 GB": "8 GB에서 측정",
      "Measured on one socket of a Xeon Silver 4514Y (15 engine cores), with batches of up to 128 lookups. In the AltoPIR paper, the fastest existing hintless scheme answers 24 to 50 times fewer queries on the same cores, for databases of 1 GB to 64 GB.":
        "Xeon Silver 4514Y의 socket 하나(engine 코어 15개)에서 조회 최대 128개로 된 batch로 측정했습니다. AltoPIR 논문에서 가장 빠른 기존 hintless scheme은 1 GB부터 64 GB까지의 데이터베이스에서 같은 코어로 24배에서 50배 적은 query에 답합니다.",
      "This page holds only the recorded lookups. Pick one of them above.": "이 페이지에는 기록된 조회만 있습니다. 위에서 하나를 고르세요.",
    },
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

function initialLang() {
  const q = new URLSearchParams(location.search).get("lang");
  if (q === "ko" || q === "en") return q;
  try { return localStorage.getItem("lang") === "ko" ? "ko" : "en"; } catch (e) { return "en"; }
}

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
// ?lang=ko or ?lang=en overrides the stored choice, so a link opens in its language
$("lang").onclick = () => {
  lang = lang === "ko" ? "en" : "ko";
  try { localStorage.setItem("lang", lang); } catch (e) {}
  try {
    const qs = new URLSearchParams(location.search);
    if (qs.has("lang")) { qs.set("lang", lang); history.replaceState(null, "", "?" + qs + location.hash); }
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
