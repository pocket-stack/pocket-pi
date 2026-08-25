import type { Metadata } from "next";
import { DOC_PAGES, DOC_SLUGS } from "../doc-pages";
import { DocsShell } from "../docs-shell";

type PageProps = { params: Promise<{ slug: string }> };

export function generateStaticParams() {
  return DOC_SLUGS.map((slug) => ({ slug }));
}

export async function generateMetadata({ params }: PageProps): Promise<Metadata> {
  const { slug } = await params;
  const page = DOC_PAGES.get(slug);
  const title = page ? `${page.title} · PocketPi Docs` : "Not found · PocketPi Docs";
  return page
    ? {
        title,
        description: page.description,
        openGraph: { title, description: page.description, images: [] },
        twitter: { title, description: page.description, images: [] },
      }
    : { title: "Not found · PocketPi Docs", robots: { index: false, follow: false } };
}

export default async function DocPage({ params }: PageProps) {
  const { slug } = await params;
  const page = DOC_PAGES.get(slug);
  if (!page) {
    return <DocsShell slug=""><h1>Page not found</h1><p>This documentation page does not exist.</p></DocsShell>;
  }
  return <DocsShell slug={page.slug}>{page.render()}</DocsShell>;
}
