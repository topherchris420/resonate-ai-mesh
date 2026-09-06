/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ["@pordenone/shared-types", "three", "@react-three/fiber", "@react-three/drei"],
  reactStrictMode: true,
};

export default nextConfig;
