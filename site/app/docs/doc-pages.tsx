import type { DocRecord } from "./doc-components";
import { START_DOCS } from "./pages-start";
import { USING_DOCS } from "./pages-using";
import { BUILD_DOCS } from "./pages-build";
import { RUNTIME_DOCS } from "./pages-runtime";
import { SECURITY_DOCS } from "./pages-security";
import { REFERENCE_DOCS } from "./pages-reference";
import { EXAMPLE_DOCS } from "./pages-examples";
import { PROJECT_DOCS } from "./pages-project";

type NavItem = Pick<DocRecord, "slug" | "title"> & { href: string };

const hrefFor = (slug: string) => slug === "overview" ? "/docs" : `/docs/${slug}`;

const records = [
  ...START_DOCS,
  ...USING_DOCS,
  ...BUILD_DOCS,
  ...RUNTIME_DOCS,
  ...SECURITY_DOCS,
  ...REFERENCE_DOCS,
  ...EXAMPLE_DOCS,
  ...PROJECT_DOCS,
];

export const DOC_PAGES = new Map(records.map((record) => [record.slug, record]));

const sections: { title: string; purpose: string; items: string[] }[] = [
  {
    title: "Start here",
    purpose: "Build the right mental model and reach a working runtime.",
    items: ["overview", "getting-started", "mental-model"],
  },
  {
    title: "Use the runtime",
    purpose: "Run, configure and operate PocketPi.",
    items: ["simulator", "pi-agent-workspace", "manage-apps", "esp32-p4", "esp32-s3"],
  },
  {
    title: "Build Apps",
    purpose: "Author, test, package and evolve an ordinary App.",
    items: [
      "app-guide",
      "app-quickstart",
      "app-files",
      "data-migrations",
      "actions-tools",
      "view-interaction",
      "networking-services",
      "resources",
      "schedules",
      "package-update",
      "testing-debugging",
    ],
  },
  {
    title: "Understand the runtime",
    purpose: "Trace ownership, execution, Guests and Harness boundaries.",
    items: ["runtime-flow", "guests-lifecycle", "layers-ownership", "harnesses"],
  },
  {
    title: "Security",
    purpose: "Review capabilities, credentials, isolation and recovery.",
    items: ["security", "data-isolation", "lifecycle-recovery"],
  },
  {
    title: "Reference",
    purpose: "Look up exact source, API, CLI and limit contracts.",
    items: ["manifest", "runtime-api", "view-api", "cli-reference", "limits"],
  },
  {
    title: "Examples",
    purpose: "Learn from complete Apps with real product boundaries.",
    items: ["exa-example", "robinhood-example"],
  },
  {
    title: "Project",
    purpose: "Separate implemented behavior, evidence and future direction.",
    items: ["current-boundaries", "validation-status"],
  },
];

export const DOC_SECTIONS: { title: string; purpose: string; items: NavItem[] }[] = sections.map((section) => ({
  title: section.title,
  purpose: section.purpose,
  items: section.items.map((slug) => {
    const record = DOC_PAGES.get(slug);
    if (!record) throw new Error(`Unknown documentation page: ${slug}`);
    return { slug, title: record.title, href: hrefFor(slug) };
  }),
}));

export const DOC_SLUGS = records.map((record) => record.slug).filter((slug) => slug !== "overview");
