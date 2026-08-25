import type { ReactNode } from "react";
import { SiteFooter, SiteHeader } from "../site-chrome";
import { DOC_SECTIONS } from "./doc-pages";

const pages = DOC_SECTIONS.flatMap((section) => section.items);

export function DocsShell({ slug, children }: { slug: string; children: ReactNode }) {
  const index = pages.findIndex((page) => page.slug === slug);
  const previous = index > 0 ? pages[index - 1] : undefined;
  const next = index >= 0 ? pages[index + 1] : undefined;

  return (
    <div className="site-shell subpage-shell">
      <SiteHeader active="docs" />
      <main className="doc-shell">
        <details className="doc-mobile-nav">
          <summary>Browse documentation</summary>
          <div>
            {DOC_SECTIONS.map((section) => (
              <section key={section.title}>
                <h2>{section.title}</h2>
                {section.items.map((item) => (
                  <a className={item.slug === slug ? "on" : ""} href={item.href} key={item.slug}>{item.title}</a>
                ))}
              </section>
            ))}
          </div>
        </details>
        <aside className="doc-nav" aria-label="Documentation navigation">
          {DOC_SECTIONS.map((section) => (
            <div className="doc-sec" key={section.title}>
              <div className="doc-sec-t">{section.title}</div>
              {section.items.map((item) => (
                <a className={item.slug === slug ? "on" : ""} href={item.href} key={item.slug}>{item.title}</a>
              ))}
            </div>
          ))}
        </aside>

        <article className="doc-body" data-slug={slug}>
          <div className="prose doc-content">{children}</div>
          <nav className="doc-pager" aria-label="Documentation pager">
            {previous ? (
              <a className="prev" href={previous.href}><span>Previous</span>{previous.title}</a>
            ) : <span />}
            {next ? (
              <a className="next" href={next.href}><span>Next</span>{next.title}</a>
            ) : <span />}
          </nav>
        </article>
      </main>
      <SiteFooter />
    </div>
  );
}
