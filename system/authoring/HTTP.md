# HTTP Actions

Read this only for an App that declares `net.http`. Network work belongs in an
Action; a View never fetches.

Framework API 1 installs `fetch` only when `net.http` is declared:

```json
{
  "capabilities": ["data.sqlite", "net.http"],
  "nativeServices": {
    "http": [{
      "method": "GET",
      "urls": ["https://example.com/data.json"],
      "allowedRequestHeaders": ["accept"],
      "credential": null
    }]
  }
}
```

```js
async function refresh() {
  const response = await fetch("https://example.com/data.json", {
    method: "GET",
    headers: { accept: "application/json" },
    timeoutMs: PocketPi.actionContext.remainingMs(),
    maxBytes: 64 * 1024,
  });
  const body = await response.json();
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return PocketPi.data.transaction(() => {
    // normalize and persist bounded fields here
    return body;
  });
}
```

Request options are `method`, `headers`, `body` (string, `Uint8Array`, or
`ArrayBuffer`), `timeoutMs`, and `maxBytes`. A response has `status`, `url`,
`headers`, `ok`, `bytes()`, `arrayBuffer()`, `text()`, and `json()`.

Native policy must allow the exact method, URL and request headers. For the
first authoring version, use public fixed endpoints without secrets. Never put a
secret in the checkout, source, SQLite, returned error, or authoring docs.
