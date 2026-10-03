import "./globals.css";
import React from "react";

export const metadata = {
  title: "Resonate AI Mesh — research cockpit",
  description:
    "Replay, verify, and explain decisions of the Pordenone kernel: deterministic validation, bounded judgment, and human authority, recorded in a hash-chained event log.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-ink font-sans text-[13px] leading-relaxed text-text antialiased">{children}</body>
    </html>
  );
}
