import { readFileSync } from "node:fs";

// The release this dashboard is built as, which /healthz reports. package.json
// carries the version from release.toml; `capsulet-xtask release check` holds
// it there.
const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  env: { CAPSULET_DASHBOARD_VERSION: version }
};

export default nextConfig;
