import "./globals.css";
import React from "react";

export const metadata = {
  title: "Pordenone NEXUS C2 Dashboard",
  description: "Cognitive Cyber-Physical Command & Control Research Platform",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="h-screen w-screen bg-[#0d1117] text-[#c9d1d9] flex flex-col antialiased select-none">
        {children}
      </body>
    </html>
  );
}
