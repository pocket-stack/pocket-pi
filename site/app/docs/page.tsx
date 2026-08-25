import type { Metadata } from "next";
import { DOC_PAGES, DOC_SECTIONS } from "./doc-pages";
import { DocsShell } from "./docs-shell";

const page = DOC_PAGES.get("overview")!;

export const metadata: Metadata = {
  title: `${page.title} · PocketPi Docs`,
  description: page.description,
  openGraph: { title: `${page.title} · PocketPi Docs`, description: page.description, images: [] },
  twitter: { title: `${page.title} · PocketPi Docs`, description: page.description, images: [] },
};

export default function OverviewPage() {
  return (
    <DocsShell slug={page.slug}>
      <header className="docs-home-intro">
        <p className="docs-home-kicker">PocketPi documentation</p>
        <h1>Build with the Agent-native runtime</h1>
        <p>
          Start the runtime, develop an App, operate physical hardware, or trace the system from
          an actor request to a rendered View. Every page below has one explicit job.
        </p>
        <nav className="docs-home-starts" aria-label="Recommended documentation starting points">
          <a href="/docs/getting-started"><span>Run it</span>Getting started</a>
          <a href="/docs/app-quickstart"><span>Build with it</span>Your first App</a>
          <a href="/docs/mental-model"><span>Understand it</span>The mental model</a>
        </nav>
      </header>

      <section className="docs-home-directory" aria-labelledby="documentation-map">
        <div className="docs-home-directory-heading">
          <p className="docs-home-kicker">35 focused pages</p>
          <h2 id="documentation-map">Documentation map</h2>
        </div>
        <div className="docs-home-groups">
          {DOC_SECTIONS.map((section) => (
            <section className="docs-home-group" key={section.title}>
              <h3>{section.title}</h3>
              <p>{section.purpose}</p>
              <div className="docs-home-links">
                {section.items.map((item) => (
                  <a href={item.href} key={item.slug}>{item.title}<span aria-hidden="true">↗</span></a>
                ))}
              </div>
            </section>
          ))}
        </div>
      </section>

      <section className="docs-home-overview" aria-labelledby="product-overview">
        {page.render()}
      </section>
    </DocsShell>
  );
}
