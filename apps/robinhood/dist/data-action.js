(()=>{var m=globalThis.db,R=m.open("robinhood");if(R<0)throw Error("open robinhood.sqlite");var C=4;function q(){return String(m.lastError(R)||"SQLite operation failed")}function g(e){if(m.exec(R,e)!==0)throw Error(q())}function v(e,t=[]){let o=JSON.parse(m.query(R,e,JSON.stringify(t)));if(o.error)throw Error(String(o.error));return o}function l(e,t=[]){return v(e,t)}var k=v("PRAGMA user_version");if(Number(k?.user_version??0)!==C)g(`
    CREATE TABLE IF NOT EXISTS accounts (
    account_number TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    suffix TEXT NOT NULL,
    account_type TEXT,
    status TEXT NOT NULL,
    agentic_allowed INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS portfolio_current (
    account_number TEXT PRIMARY KEY,
    cash TEXT,
    buying_power TEXT,
    day_pnl TEXT,
    week_pnl TEXT,
    observed_at INTEGER NOT NULL
  );
  CREATE TABLE IF NOT EXISTS total_value (
    account_number TEXT NOT NULL,
    observed_at INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY(account_number, observed_at)
  );
  CREATE TABLE IF NOT EXISTS positions (
    account_number TEXT NOT NULL,
    symbol TEXT NOT NULL,
    quantity TEXT,
    average_price TEXT,
    market_value TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, symbol)
  );
  CREATE TABLE IF NOT EXISTS activities (
    account_number TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    occurred_at TEXT,
    observed_at INTEGER NOT NULL,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    price TEXT,
    amount TEXT,
    state TEXT,
    activity_type TEXT,
    PRIMARY KEY(account_number, activity_id)
  );
  CREATE INDEX IF NOT EXISTS activities_account_recent ON activities(account_number, occurred_at DESC, observed_at DESC);
  CREATE TABLE IF NOT EXISTS refresh_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    operation_count INTEGER NOT NULL,
    success_count INTEGER NOT NULL,
    error TEXT
  );
  CREATE INDEX IF NOT EXISTS refresh_runs_recent ON refresh_runs(id DESC);
  CREATE TABLE IF NOT EXISTS equity_historicals (
    account_number TEXT NOT NULL,
    span TEXT NOT NULL,
    point_time TEXT NOT NULL,
    price TEXT,
    open TEXT,
    high TEXT,
    low TEXT,
    close TEXT,
    volume TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, span, point_time)
  );
  CREATE TABLE IF NOT EXISTS pnl_trades (
    account_number TEXT NOT NULL,
    trade_id TEXT NOT NULL,
    occurred_at TEXT,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    price TEXT,
    realized_pnl TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY(account_number, trade_id)
  );
  CREATE TABLE IF NOT EXISTS order_reviews (
    review_id TEXT PRIMARY KEY,
    account_number TEXT,
    symbol TEXT,
    side TEXT,
    quantity TEXT,
    limit_price TEXT,
    estimated_cost TEXT,
    state TEXT,
    observed_at INTEGER NOT NULL
  );
    PRAGMA user_version=4;
  `);function y(){return Math.floor(Date.now()/1000)}function h(e){return e===null||e===void 0||typeof e==="object"?null:String(e)}function i(e,t){if(e===null||e===void 0||typeof e!=="object")return null;for(let o of t){let r=h(e[o]);if(r!==null)return r}for(let o of Object.values(e)){let r=i(o,t);if(r!==null)return r}return null}function I(e,t){if(e===null||e===void 0||typeof e!=="object")return[];for(let o of t)if(Array.isArray(e[o]))return e[o];for(let o of Object.values(e)){let r=I(o,t);if(r.length)return r}return[]}function P(e,t){let o=i(e,t)?.toLowerCase();return o==="true"||o==="1"||o==="yes"}function L(e){if(e===null)return null;let t=Number(e.replace(/[$,%]/g,""));return Number.isFinite(t)?t:null}function d(e,t){return i(e,["account_number","accountNumber","account"])||String(t?.account_number??t?.accountNumber??"")}function N(e,t){let o=I(e,t);return o.length?o:Array.isArray(e)?e:e?[e]:[]}function S(e,t){let o=JSON.parse(globalThis.services.call("mcp.client","callTool",JSON.stringify({connection:"robinhood",name:e,arguments:t})));if(!o.ok)throw Error(o.error||"Robinhood service failed");return o.value}function M(e){let t=JSON.parse(globalThis.services.call("mcp.client","callTools",JSON.stringify({connection:"robinhood",calls:e.map((o)=>({name:o.operation,arguments:o.args}))})));if(!t.ok)throw Error(t.error||"Robinhood batch service failed");return Array.isArray(t.value?.results)?t.value.results:[]}function A(e){g("BEGIN IMMEDIATE");try{e(),g("COMMIT")}catch(t){try{g("ROLLBACK")}catch{}throw t}globalThis.app.commit()}function D(e,t){let o=N(e,["accounts"]);l("DELETE FROM accounts");for(let r of o){let a=d(r,{});if(!a)continue;let n=(i(r,["nickname","account_type","type"])||"").toUpperCase(),c=P(r,["agentic_allowed","agenticAllowed"]),s=c?"AGENTIC":n.includes("IRA")||n.includes("RETIRE")?"RETIREMENT":n.includes("JOINT")?"JOINT":"PERSONAL";l("INSERT INTO accounts(account_number,label,suffix,account_type,status,agentic_allowed,updated_at) VALUES(?,?,?,?,?,?,?)",[a,s,a.slice(-4),n,(i(r,["status"])||"active").toUpperCase(),c?1:0,t])}}function x(e,t,o){let r=d(e,t);if(!r)throw Error("Robinhood portfolio is missing account_number");let a=i(e,["cash","cash_available","withdrawable_amount"]),n=i(e,["buying_power","buyingPower"]),c=i(e,["day_pnl","dayPnl","equity_change"]),s=i(e,["week_pnl","weekPnl"]);l(`INSERT INTO portfolio_current(account_number,cash,buying_power,day_pnl,week_pnl,observed_at)
     VALUES(?,?,?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       cash=excluded.cash,buying_power=excluded.buying_power,
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=excluded.observed_at`,[r,a,n,c,s,o]);let u=i(e,["total_value","equity","total_equity","portfolio_value","market_value"]);if(u!==null)l("INSERT OR REPLACE INTO total_value(account_number,observed_at,value) VALUES(?,?,?)",[r,o,u])}function F(e,t,o){let r=d(e,t);if(!r)throw Error("Robinhood positions are missing account_number");let a=N(e,["positions"]).slice(0,64);l("DELETE FROM positions WHERE account_number=?",[r]);for(let n of a){let c=i(n,["symbol"]);if(!c)continue;l("INSERT INTO positions(account_number,symbol,quantity,average_price,market_value,observed_at) VALUES(?,?,?,?,?,?)",[r,c,i(n,["quantity","shares"]),i(n,["average_price","averagePrice","average_buy_price"]),i(n,["market_value","marketValue","equity"]),o])}}function G(e,t,o){let r=d(e,t);if(!r)throw Error("Robinhood activities are missing account_number");let a=N(e,["orders","activities","results"]).slice(0,64);l("DELETE FROM activities WHERE account_number=?",[r]),a.forEach((n,c)=>{let s=i(n,["symbol"]),u=(i(n,["side"])||"").toUpperCase(),_=i(n,["executed_quantity","cumulative_quantity","quantity"]),p=i(n,["average_price","averagePrice","executed_price","price"]),b=i(n,["last_transaction_at","created_at","updated_at","date"]),T=i(n,["id","order_id","orderId","activity_id"])||[b||o,s||"ORDER",u,c].join(":"),E=L(_),f=L(p),U=E!==null&&f!==null?String(E*f):p||_;l(`INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)
       VALUES(?,?,?,?,?,?,?,?,?,?,?)`,[r,T,b,o,s,u,_,p,U,(i(n,["state","status"])||"RECENT").toUpperCase(),(i(n,["type","order_type"])||"ORDER").toUpperCase()])})}function Y(e,t,o){let r=d(e,t);if(!r)throw Error("Robinhood P&L is missing account_number");let a=i(e,["total_returns","realized_pnl","total","amount","day_pnl","week_pnl"]),n=String(t?.span??"day");l(`INSERT INTO portfolio_current(account_number,day_pnl,week_pnl,observed_at) VALUES(?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=MAX(portfolio_current.observed_at,excluded.observed_at)`,[r,n==="week"?null:a,n==="week"?a:null,o])}function J(e,t,o){let r=d(e,t),a=String(t?.span??t?.interval??"default"),n=N(e,["historicals","results","data"]);l("DELETE FROM equity_historicals WHERE account_number=? AND span=?",[r,a]),n.forEach((c,s)=>{let u=i(c,["begins_at","timestamp","time","date"])||String(s);l(`INSERT INTO equity_historicals(account_number,span,point_time,price,open,high,low,close,volume,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?,?)`,[r,a,u,i(c,["price","adjusted_close","close_price"]),i(c,["open_price","open"]),i(c,["high_price","high"]),i(c,["low_price","low"]),i(c,["close_price","close"]),i(c,["volume"]),o])})}function V(e,t,o){let r=d(e,t),a=N(e,["trades","results","data"]);l("DELETE FROM pnl_trades WHERE account_number=?",[r]),a.forEach((n,c)=>{let s=i(n,["created_at","updated_at","date","timestamp"]),u=i(n,["id","trade_id","tradeId"])||[s||o,i(n,["symbol"])||"TRADE",c].join(":");l(`INSERT INTO pnl_trades(account_number,trade_id,occurred_at,symbol,side,quantity,price,realized_pnl,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?)`,[r,u,s,i(n,["symbol"]),(i(n,["side"])||"").toUpperCase(),i(n,["quantity","shares"]),i(n,["price","average_price"]),i(n,["realized_pnl","pnl","amount"]),o])})}function B(e,t,o){let r=d(e,t),a=i(e,["symbol"])||h(t?.symbol),n=(i(e,["side"])||h(t?.side)||"").toUpperCase(),c=i(e,["quantity","shares"])||h(t?.quantity),s=i(e,["limit_price","limitPrice","price"])||h(t?.limit_price??t?.price),u=i(e,["id","review_id","reviewId"])||[r,a,n,c,s,o].join(":");l(`INSERT OR REPLACE INTO order_reviews(review_id,account_number,symbol,side,quantity,limit_price,estimated_cost,state,observed_at)
     VALUES(?,?,?,?,?,?,?,?,?)`,[u,r,a,n,c,s,i(e,["estimated_cost","estimatedCost","total"]),i(e,["state","status"]),o])}function X(e){if(e.operation==="get_accounts")D(e.value,e.observedAt);else if(e.operation==="get_portfolio")x(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_positions")F(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_orders")G(e.value,e.args,e.observedAt);else if(e.operation==="get_realized_pnl")Y(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_historicals")J(e.value,e.args,e.observedAt);else if(e.operation==="get_pnl_trade_history")V(e.value,e.args,e.observedAt);else if(e.operation==="review_equity_order")B(e.value,e.args,e.observedAt);else throw Error("No Robinhood table mapping for "+e.operation)}function z(e){let t=e.toLowerCase();return t.includes("esp_err_http_connect")||t.includes("timeout")||t.includes("tls")||t.includes("socket")||t.includes("network")}function O(){let e=y(),t=[],o=[],r=0,a=(s,u)=>{r+=1;try{let _=S(s,u);return t.push({operation:s,args:u,value:_,observedAt:y()}),_}catch(_){let p=_ instanceof Error?_.message:String(_);if(o.push(s+": "+p),s==="get_accounts"||z(p))throw _;return null}},n=null;try{let s=a("get_accounts",{}),_=N(s,["accounts"]).map((T)=>d(T,{})).filter(Boolean);if(!_.length)throw Error("Robinhood returned no brokerage accounts");let p=new Date((e-604800)*1000).toISOString().slice(0,10),b=[];for(let T of _){let E={account_number:T};b.push({operation:"get_portfolio",args:E},{operation:"get_equity_positions",args:E},{operation:"get_equity_orders",args:{...E,created_at_gte:p}},{operation:"get_realized_pnl",args:{...E,span:"day",asset_classes:["equity"]}},{operation:"get_realized_pnl",args:{...E,span:"week",asset_classes:["equity"]}})}r+=b.length;let w=M(b);if(w.length!==b.length)throw Error("Robinhood batch returned an incomplete result set");if(w.forEach((T,E)=>{let f=b[E];if(T?.ok)t.push({operation:f.operation,args:f.args,value:T.value,observedAt:y()});else o.push(f.operation+": "+String(T?.error||"unknown provider error"))}),!t.some((T)=>T.operation==="get_portfolio"))throw Error("Robinhood batch returned no portfolio data")}catch(s){n=s instanceof Error?s.message:String(s)}let c=n?"failed":o.length?"partial":"succeeded";if(A(()=>{if(!n)for(let s of t)X(s);l(`INSERT INTO refresh_runs(started_at,completed_at,status,operation_count,success_count,error)
       VALUES(?,?,?,?,?,?)`,[e,y(),c,r,n?0:t.length,n||o.join(" | ")||null])}),n)throw Error(n);return{status:c,operationCount:r,successCount:t.length}}var j={"robinhood.get_accounts":"get_accounts","robinhood.get_portfolio":"get_portfolio","robinhood.get_equity_positions":"get_equity_positions","robinhood.get_equity_orders":"get_equity_orders","robinhood.get_equity_historicals":"get_equity_historicals","robinhood.get_realized_pnl":"get_realized_pnl","robinhood.get_pnl_trade_history":"get_pnl_trade_history","robinhood.review_equity_order":"review_equity_order"};function K(e,t){let o=j[e];if(!o)throw Error("Unknown Robinhood tool: "+e);let r=S(o,t);return A(()=>X({operation:o,args:t,value:r,observedAt:y()})),r}globalThis.PocketPiData={invokeTask(e){try{if(e!=="refreshPortfolio")throw Error("Unknown Robinhood Data Action: "+e);let t=O();return JSON.stringify({text:JSON.stringify(t),details:t,isError:!1})}catch(t){return JSON.stringify({text:t instanceof Error?t.message:String(t),isError:!0})}},invokeTool(e,t){try{let o=JSON.parse(t),r=e==="robinhood.refresh_portfolio"?O():K(e,o);return JSON.stringify({text:JSON.stringify(r),details:r,isError:!1})}catch(o){return JSON.stringify({text:o instanceof Error?o.message:String(o),isError:!0})}}};})();
