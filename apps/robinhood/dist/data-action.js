(()=>{var h=globalThis.db,v=h.open("robinhood");if(v<0)throw Error("open robinhood.sqlite");var C=4;function q(){return String(h.lastError(v)||"SQLite operation failed")}function m(e){if(h.exec(v,e)!==0)throw Error(q())}function O(e,t=[]){let o=JSON.parse(h.query(v,e,JSON.stringify(t)));if(o.error)throw Error(String(o.error));return o}function l(e,t=[]){return O(e,t)}var k=O("PRAGMA user_version");if(Number(k?.user_version??0)!==C)m(`
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
  `);function N(){return Math.floor(Date.now()/1000)}function g(e){return e===null||e===void 0||typeof e==="object"?null:String(e)}function i(e,t){if(e===null||e===void 0||typeof e!=="object")return null;for(let o of t){let n=g(e[o]);if(n!==null)return n}for(let o of Object.values(e)){let n=i(o,t);if(n!==null)return n}return null}function I(e,t){if(e===null||e===void 0||typeof e!=="object")return[];for(let o of t)if(Array.isArray(e[o]))return e[o];for(let o of Object.values(e)){let n=I(o,t);if(n.length)return n}return[]}function P(e,t){let o=i(e,t)?.toLowerCase();return o==="true"||o==="1"||o==="yes"}function w(e){if(e===null)return null;let t=Number(e.replace(/[$,%]/g,""));return Number.isFinite(t)?t:null}function d(e,t){return i(e,["account_number","accountNumber","account"])||String(t?.account_number??t?.accountNumber??"")}function f(e,t){let o=I(e,t);return o.length?o:Array.isArray(e)?e:e?[e]:[]}function A(e,t){let o=JSON.parse(globalThis.services.call("mcp.client","callTool",JSON.stringify({connection:"robinhood",name:e,arguments:t})));if(!o.ok)throw Error(o.error||"Robinhood service failed");return o.value}function D(e){let t=JSON.parse(globalThis.services.call("mcp.client","callTools",JSON.stringify({connection:"robinhood",calls:e.map((o)=>({name:o.operation,arguments:o.args}))})));if(!t.ok)throw Error(t.error||"Robinhood batch service failed");return Array.isArray(t.value?.results)?t.value.results:[]}function S(e){m("BEGIN IMMEDIATE");try{e(),m("COMMIT")}catch(t){try{m("ROLLBACK")}catch{}throw t}globalThis.app.commit()}function M(e,t){let o=f(e,["accounts"]);l("DELETE FROM accounts");for(let n of o){let c=d(n,{});if(!c)continue;let r=(i(n,["nickname","account_type","type"])||"").toUpperCase(),a=P(n,["agentic_allowed","agenticAllowed"]),s=a?"AGENTIC":r.includes("IRA")||r.includes("RETIRE")?"RETIREMENT":r.includes("JOINT")?"JOINT":"PERSONAL";l("INSERT INTO accounts(account_number,label,suffix,account_type,status,agentic_allowed,updated_at) VALUES(?,?,?,?,?,?,?)",[c,s,c.slice(-4),r,(i(n,["status"])||"active").toUpperCase(),a?1:0,t])}}function x(e,t,o){let n=d(e,t);if(!n)throw Error("Robinhood portfolio is missing account_number");let c=i(e,["cash","cash_available","withdrawable_amount"]),r=i(e,["buying_power","buyingPower"]),a=i(e,["day_pnl","dayPnl","equity_change"]),s=i(e,["week_pnl","weekPnl"]);l(`INSERT INTO portfolio_current(account_number,cash,buying_power,day_pnl,week_pnl,observed_at)
     VALUES(?,?,?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       cash=excluded.cash,buying_power=excluded.buying_power,
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=excluded.observed_at`,[n,c,r,a,s,o]);let u=i(e,["total_value","equity","total_equity","portfolio_value","market_value"]);if(u!==null)l("INSERT OR REPLACE INTO total_value(account_number,observed_at,value) VALUES(?,?,?)",[n,o,u])}function F(e,t,o){let n=d(e,t);if(!n)throw Error("Robinhood positions are missing account_number");let c=f(e,["positions"]).slice(0,64);l("DELETE FROM positions WHERE account_number=?",[n]);for(let r of c){let a=i(r,["symbol"]);if(!a)continue;l("INSERT INTO positions(account_number,symbol,quantity,average_price,market_value,observed_at) VALUES(?,?,?,?,?,?)",[n,a,i(r,["quantity","shares"]),i(r,["average_price","averagePrice","average_buy_price"]),i(r,["market_value","marketValue","equity"]),o])}}function G(e,t,o){let n=d(e,t);if(!n)throw Error("Robinhood activities are missing account_number");let c=f(e,["orders","activities","results"]).slice(0,64);l("DELETE FROM activities WHERE account_number=?",[n]),c.forEach((r,a)=>{let s=i(r,["symbol"]),u=(i(r,["side"])||"").toUpperCase(),_=i(r,["executed_quantity","cumulative_quantity","quantity"]),p=i(r,["average_price","averagePrice","executed_price","price"]),y=i(r,["last_transaction_at","created_at","updated_at","date"]),T=i(r,["id","order_id","orderId","activity_id"])||[y||o,s||"ORDER",u,a].join(":"),E=w(_),b=w(p),U=E!==null&&b!==null?String(E*b):p||_;l(`INSERT INTO activities(account_number,activity_id,occurred_at,observed_at,symbol,side,quantity,price,amount,state,activity_type)
       VALUES(?,?,?,?,?,?,?,?,?,?,?)`,[n,T,y,o,s,u,_,p,U,(i(r,["state","status"])||"RECENT").toUpperCase(),(i(r,["type","order_type"])||"ORDER").toUpperCase()])})}function Y(e,t,o){let n=d(e,t);if(!n)throw Error("Robinhood P&L is missing account_number");let c=i(e,["total_returns","realized_pnl","total","amount","day_pnl","week_pnl"]),r=String(t?.span??"day");l(`INSERT INTO portfolio_current(account_number,day_pnl,week_pnl,observed_at) VALUES(?,?,?,?)
     ON CONFLICT(account_number) DO UPDATE SET
       day_pnl=COALESCE(excluded.day_pnl,portfolio_current.day_pnl),
       week_pnl=COALESCE(excluded.week_pnl,portfolio_current.week_pnl),
       observed_at=MAX(portfolio_current.observed_at,excluded.observed_at)`,[n,r==="week"?null:c,r==="week"?c:null,o])}function J(e,t,o){let n=d(e,t),c=String(t?.span??t?.interval??"default"),r=f(e,["historicals","results","data"]);l("DELETE FROM equity_historicals WHERE account_number=? AND span=?",[n,c]),r.forEach((a,s)=>{let u=i(a,["begins_at","timestamp","time","date"])||String(s);l(`INSERT INTO equity_historicals(account_number,span,point_time,price,open,high,low,close,volume,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?,?)`,[n,c,u,i(a,["price","adjusted_close","close_price"]),i(a,["open_price","open"]),i(a,["high_price","high"]),i(a,["low_price","low"]),i(a,["close_price","close"]),i(a,["volume"]),o])})}function V(e,t,o){let n=d(e,t),c=f(e,["trades","results","data"]);l("DELETE FROM pnl_trades WHERE account_number=?",[n]),c.forEach((r,a)=>{let s=i(r,["created_at","updated_at","date","timestamp"]),u=i(r,["id","trade_id","tradeId"])||[s||o,i(r,["symbol"])||"TRADE",a].join(":");l(`INSERT INTO pnl_trades(account_number,trade_id,occurred_at,symbol,side,quantity,price,realized_pnl,observed_at)
       VALUES(?,?,?,?,?,?,?,?,?)`,[n,u,s,i(r,["symbol"]),(i(r,["side"])||"").toUpperCase(),i(r,["quantity","shares"]),i(r,["price","average_price"]),i(r,["realized_pnl","pnl","amount"]),o])})}function B(e,t,o){let n=d(e,t),c=i(e,["symbol"])||g(t?.symbol),r=(i(e,["side"])||g(t?.side)||"").toUpperCase(),a=i(e,["quantity","shares"])||g(t?.quantity),s=i(e,["limit_price","limitPrice","price"])||g(t?.limit_price??t?.price),u=i(e,["id","review_id","reviewId"])||[n,c,r,a,s,o].join(":");l(`INSERT OR REPLACE INTO order_reviews(review_id,account_number,symbol,side,quantity,limit_price,estimated_cost,state,observed_at)
     VALUES(?,?,?,?,?,?,?,?,?)`,[u,n,c,r,a,s,i(e,["estimated_cost","estimatedCost","total"]),i(e,["state","status"]),o])}function X(e){if(e.operation==="get_accounts")M(e.value,e.observedAt);else if(e.operation==="get_portfolio")x(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_positions")F(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_orders")G(e.value,e.args,e.observedAt);else if(e.operation==="get_realized_pnl")Y(e.value,e.args,e.observedAt);else if(e.operation==="get_equity_historicals")J(e.value,e.args,e.observedAt);else if(e.operation==="get_pnl_trade_history")V(e.value,e.args,e.observedAt);else if(e.operation==="review_equity_order")B(e.value,e.args,e.observedAt);else throw Error("No Robinhood table mapping for "+e.operation)}function z(e){let t=e.toLowerCase();return t.includes("esp_err_http_connect")||t.includes("timeout")||t.includes("tls")||t.includes("socket")||t.includes("network")}function L(){let e=N(),t=[],o=[],n=0,c=(s,u)=>{n+=1;try{let _=A(s,u);return t.push({operation:s,args:u,value:_,observedAt:N()}),_}catch(_){let p=_ instanceof Error?_.message:String(_);if(o.push(s+": "+p),s==="get_accounts"||z(p))throw _;return null}},r=null;try{let s=c("get_accounts",{}),_=f(s,["accounts"]).map((T)=>d(T,{})).filter(Boolean);if(!_.length)throw Error("Robinhood returned no brokerage accounts");let p=new Date((e-604800)*1000).toISOString().slice(0,10),y=[];for(let T of _){let E={account_number:T};y.push({operation:"get_portfolio",args:E},{operation:"get_equity_positions",args:E},{operation:"get_equity_orders",args:{...E,created_at_gte:p}},{operation:"get_realized_pnl",args:{...E,span:"day",asset_classes:["equity"]}},{operation:"get_realized_pnl",args:{...E,span:"week",asset_classes:["equity"]}})}n+=y.length;let R=D(y);if(R.length!==y.length)throw Error("Robinhood batch returned an incomplete result set");if(R.forEach((T,E)=>{let b=y[E];if(T?.ok)t.push({operation:b.operation,args:b.args,value:T.value,observedAt:N()});else o.push(b.operation+": "+String(T?.error||"unknown provider error"))}),!t.some((T)=>T.operation==="get_portfolio"))throw Error("Robinhood batch returned no portfolio data")}catch(s){r=s instanceof Error?s.message:String(s)}let a=r?"failed":o.length?"partial":"succeeded";if(S(()=>{if(!r)for(let s of t)X(s);l(`INSERT INTO refresh_runs(started_at,completed_at,status,operation_count,success_count,error)
       VALUES(?,?,?,?,?,?)`,[e,N(),a,n,r?0:t.length,r||o.join(" | ")||null])}),r)throw Error(r);return{status:a,operationCount:n,successCount:t.length}}var j={"robinhood.get_accounts":"get_accounts","robinhood.get_portfolio":"get_portfolio","robinhood.get_equity_positions":"get_equity_positions","robinhood.get_equity_orders":"get_equity_orders","robinhood.get_equity_historicals":"get_equity_historicals","robinhood.get_realized_pnl":"get_realized_pnl","robinhood.get_pnl_trade_history":"get_pnl_trade_history","robinhood.review_equity_order":"review_equity_order"};function K(e,t){let o=j[e];if(!o)throw Error("Unknown Robinhood tool: "+e);let n=A(o,t);return S(()=>X({operation:o,args:t,value:n,observedAt:N()})),n}globalThis.PocketPiData={invokeTask(e){try{if(e!=="refreshPortfolio")throw Error("Unknown Robinhood Data Action: "+e);let t=L();return JSON.stringify({text:JSON.stringify(t),details:t,isError:!1})}catch(t){return JSON.stringify({text:t instanceof Error?t.message:String(t),isError:!0})}},invokeTool(e,t){try{let o=JSON.parse(t),n=e==="robinhood.refresh_portfolio"?L():K(e,o);return JSON.stringify({text:JSON.stringify(n),details:n,isError:!1})}catch(o){return JSON.stringify({text:o instanceof Error?o.message:String(o),isError:!0})}}};})();
