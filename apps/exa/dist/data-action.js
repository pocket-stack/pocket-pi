(()=>{var h=globalThis.db,E=h.open("exa");if(E<0)throw Error("open exa.sqlite");var I=3;function O(){return String(h.lastError(E)||"SQLite operation failed")}function c(r){if(h.exec(E,r)!==0)throw Error(O())}function N(r,n=[]){let e=JSON.parse(h.query(E,r,JSON.stringify(n)));if(e.error)throw Error(String(e.error));return e}function l(r,n=[]){return N(r,n)}var m=N("PRAGMA user_version");if(Number(m?.user_version??0)!==I)c(`
    CREATE TABLE IF NOT EXISTS searches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    searched_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    result_count INTEGER NOT NULL DEFAULT 0,
    error TEXT
  );
  CREATE INDEX IF NOT EXISTS searches_recent ON searches(id DESC);
  CREATE TABLE IF NOT EXISTS search_results (
    search_id INTEGER NOT NULL,
    rank INTEGER NOT NULL,
    title TEXT,
    url TEXT NOT NULL,
    published_at TEXT,
    author TEXT,
    score TEXT,
    highlight TEXT,
    PRIMARY KEY(search_id, rank)
  );
  CREATE INDEX IF NOT EXISTS search_results_url ON search_results(url);
  CREATE TABLE IF NOT EXISTS documents (
    url TEXT PRIMARY KEY,
    fetched_at INTEGER NOT NULL,
    title TEXT,
    published_at TEXT,
    author TEXT,
    text TEXT NOT NULL
  );
  CREATE INDEX IF NOT EXISTS documents_recent ON documents(fetched_at DESC);
    PRAGMA user_version=3;
  `);function f(){return Math.floor(Date.now()/1000)}function A(r){return r===null||r===void 0||typeof r==="object"?null:String(r)}function s(r,n){if(!r||typeof r!=="object")return null;for(let e of n){let t=A(r[e]);if(t!==null)return t}return null}function x(r){if(Array.isArray(r?.highlights))return r.highlights.map(String).join(`
`).slice(0,1200);return s(r,["highlight","summary","text"])?.slice(0,1200)??null}function y(r,n){let e=JSON.parse(globalThis.services.call("net.http","post",JSON.stringify({connection:"exa",path:r,body:n})));if(!e.ok)throw Error(e.error||"Exa service failed");return e.value}function T(r){c("BEGIN IMMEDIATE");try{r(),c("COMMIT")}catch(n){try{c("ROLLBACK")}catch{}throw n}globalThis.app.commit()}function _(r){let n=String(r.query??"").trim();if(!n)throw Error("query is required");let e=f();try{let t={query:n,type:r.searchType??"auto",numResults:Math.max(1,Math.min(8,Number(r.numResults??5))),contents:{highlights:{maxCharacters:800}}};for(let i of["includeDomains","excludeDomains","startPublishedDate","endPublishedDate"])if(r[i]!==void 0)t[i]=r[i];let o=y("/search",t),u=Array.isArray(o?.results)?o.results.slice(0,8):[];return T(()=>{let i=l("INSERT INTO searches(query,searched_at,status,result_count,error) VALUES(?,?,?,?,NULL)",[n,e,"ok",u.length]),g=Number(i.lastInsertRowid||0);u.forEach((a,S)=>{let d=s(a,["url","id"]);if(!d)return;l(`INSERT INTO search_results(search_id,rank,title,url,published_at,author,score,highlight)
           VALUES(?,?,?,?,?,?,?,?)`,[g,S,s(a,["title"]),d,s(a,["publishedDate","published_at","date"]),s(a,["author"]),s(a,["score"]),x(a)])})}),o}catch(t){let o=t instanceof Error?t.message:String(t);throw T(()=>l("INSERT INTO searches(query,searched_at,status,result_count,error) VALUES(?,?,?,0,?)",[n,e,"error",o])),t}}function p(r){let n=String(r.url??"").trim();if(!n)throw Error("url is required");let e=y("/contents",{urls:[n],text:{maxCharacters:Math.max(200,Math.min(12000,Number(r.maxCharacters??6000))),includeHtmlTags:!1}}),t=Array.isArray(e?.results)?e.results[0]:e;if(!t)throw Error("Exa returned no document");let o=s(t,["url","id"])||n,u=s(t,["text","content","summary"])||"";return T(()=>l(`INSERT INTO documents(url,fetched_at,title,published_at,author,text) VALUES(?,?,?,?,?,?)
     ON CONFLICT(url) DO UPDATE SET fetched_at=excluded.fetched_at,title=excluded.title,
       published_at=excluded.published_at,author=excluded.author,text=excluded.text`,[o,f(),s(t,["title"]),s(t,["publishedDate","published_at","date"]),s(t,["author"]),u])),{status:"ok",provider:"exa",requestId:e?.requestId,document:t,costDollars:e?.costDollars}}globalThis.PocketPiData={invokeTask(r){return JSON.stringify({text:"Unknown Exa Data Action: "+r,isError:!0})},invokeTool(r,n){try{let e=JSON.parse(n),t=r==="research.search"?_(e):r==="research.fetch"?p(e):(()=>{throw Error("Unknown Exa tool: "+r)})();return JSON.stringify({text:JSON.stringify(t),details:t,isError:!1})}catch(e){return JSON.stringify({text:e instanceof Error?e.message:String(e),isError:!0})}}};})();
