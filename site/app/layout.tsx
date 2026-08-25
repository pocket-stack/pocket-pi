import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { FullPageNavigation } from "./full-page-navigation";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

const title = "PocketPi · Agent-native runtime for embedded systems";
const description = "A resident, local-first Agent runtime for embedded and dedicated devices, built on PocketJS.";

export const metadata: Metadata = {
  metadataBase: new URL("https://pi.pocketlab.build"),
  title,
  description,
  icons: { icon: "/favicon.svg", shortcut: "/favicon.svg" },
  openGraph: {
    type: "website",
    siteName: "PocketPi",
    title,
    description,
    images: [{ url: "/og.png", width: 1200, height: 630, alt: title }],
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
    images: ["/og.png"],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <FullPageNavigation>{children}</FullPageNavigation>
      </body>
    </html>
  );
}
