import path from "node:path";

// Container builds set NEXT_OUTPUT=standalone to ship a minimal server.
const standalone = process.env.NEXT_OUTPUT === "standalone";

/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ["@pordenone/shared-types"],
  reactStrictMode: true,
  poweredByHeader: false,
  ...(standalone ? { output: "standalone", outputFileTracingRoot: path.join(import.meta.dirname, "../..") } : {}),
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          { key: "X-Content-Type-Options", value: "nosniff" },
          { key: "Referrer-Policy", value: "no-referrer" },
          { key: "X-Frame-Options", value: "DENY" },
        ],
      },
    ];
  },
};

export default nextConfig;
