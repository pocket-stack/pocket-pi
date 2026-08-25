# PocketPi website

The PocketPi landing page, documentation, blog index and changelog.

The site uses React through [vinext](https://github.com/cloudflare/vinext) and
exports every route as static assets. Wrangler publishes those assets through
a Cloudflare Worker, matching the hosting shape used by `pocketjs.dev` and
`pocketlab.build`. The site does not need a server script, database or runtime
storage.

## Local development

Requires Bun 1.3.10 and Node.js 22.13 or newer.

```bash
bun install
bun run dev
```

The development server prints the local preview URL. The default in this
workspace is `http://[::1]:3000/`.

## Validation

```bash
bun run lint
bun run test
```

`bun run test` type-checks the source, statically exports all routes, then
checks the root, every documentation page, the blog and the changelog from the
deployable asset tree.

## Source layout

- `app/page.tsx`: landing page
- `app/pocketpi-device-stage.tsx`: interactive Three.js device stage
- `app/pocketjs-screen-runtime.ts`: simulator-frame switching and touch mapping
- `app/docs/`: documentation records and routes
- `app/blog/` and `app/changelog/`: first-party content indexes
- `public/pocketpi-device/`: web GLB and real S3 simulator frames
- `pocketpi-s3-simulator/`: reproducible simulator screenshot script
- `wrangler.jsonc`: Cloudflare Worker, asset and custom-domain configuration

## Hardware assets

The Hero loads `public/pocketpi-device/device.glb`. Its live screen uses the
four 480 by 800 frames in `public/pocketpi-device/screens/`.

The two board photographs shown near the end of the landing page are in
`public/device-photos/`. They are intentionally isolated on a black background
for consistent product presentation.

## Hosting shape

`bun run build` exports the deployable tree under `dist/client/`. The
repository stores source and `bun.lock`, not `dist/`, `node_modules/` or local
Wrangler state.

`bun run preview` serves that asset-only Worker locally. `bun run deploy`
publishes it to the `pocketpi` Worker and attaches `pi.pocketlab.build` as a
Custom Domain.

## Production deployment

The `pocketpi` Worker is independent from the existing `pocketlab` and
`pocketjs` Workers. Its Wrangler configuration owns only the
`pi.pocketlab.build` custom domain.

The GitHub Actions workflow is intentionally manual. To use it, configure the
repository secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`; local
deployments can use Wrangler OAuth instead.
