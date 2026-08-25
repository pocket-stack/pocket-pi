import type { Metadata } from "next";
import { SiteFooter, SiteHeader } from "../site-chrome";

export const metadata: Metadata = {
  title: "Blog · PocketPi",
  description: "Published engineering articles about PocketPi and its Agent-native App model.",
};

const posts = [
  {
    date: "August 15, 2026",
    iso: "2026-08-15",
    title: "Taking a Step Further Towards an Agent-Native Runtime on Embedded Systems",
    description: "Protected native mechanisms, one PocketJS substrate, bounded Guests, durable App Data and an explicit System Framework.",
    href: "https://pocketjs.dev/blog/agent-native-runtime-embedded-systems/",
  },
  {
    date: "August 11, 2026",
    iso: "2026-08-11",
    title: "Designing Apps for Humans and Agents in an Agent-Native Runtime",
    description: "Deriving App = Data + Actions + View from the shared needs of human interaction, Agent tools and schedules.",
    href: "https://pocketjs.dev/blog/pocket-pi-agent-native-runtime/",
  },
  {
    date: "August 6, 2026",
    iso: "2026-08-06",
    title: "Just Enough Node: Porting the Pi Coding Agent to the ESP32-P4",
    description: "How a Node-shaped runtime, compact Pi Agent profile, native tools and PocketJS UI brought a complete Agent onto a microcontroller.",
    href: "https://pocketjs.dev/blog/pocket-pi-on-esp32-p4/",
  },
];

export default function BlogPage() {
  return (
    <div className="site-shell subpage-shell">
      <SiteHeader active="blog" />
      <main className="blog-index">
        <header className="blog-index-header">
          <p className="page-eyebrow">PocketPi writing</p>
          <h1>Blog</h1>
          <p>
            Design notes and implementation stories about resident Agents, embedded runtimes and
            Apps built for humans and Agents as first-class users.
          </p>
        </header>

        <div className="blog-story-list">
          {posts.map((post, index) => (
            <a className={`blog-story${index === 0 ? " blog-story-featured" : ""}`} href={post.href} key={post.href} target="_blank" rel="noreferrer">
              <div className="blog-story-meta">
                <time dateTime={post.iso}>{post.date}</time>
              </div>
              <div className="blog-story-copy">
                <h2>{post.title}</h2>
                <p>{post.description}</p>
              </div>
              <span className="blog-story-action">Read article <span aria-hidden="true">↗</span></span>
            </a>
          ))}
        </div>
      </main>
      <SiteFooter />
    </div>
  );
}
