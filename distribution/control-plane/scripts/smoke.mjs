import path from "node:path";
import { pathToFileURL } from "node:url";

function origin(value, name) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${name} must be an HTTPS root origin`);
  }
  if (url.protocol !== "https:" || !url.hostname || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new Error(`${name} must be an HTTPS root origin without credentials, path, query, or fragment`);
  }
  return url.origin;
}

export function parseSmokeArgs(args) {
  if (args.length !== 4 || args[0] !== "--admin-origin" || args[2] !== "--update-origin") {
    throw new Error("Usage: node scripts/smoke.mjs --admin-origin https://admin.example/ --update-origin https://updates.example/");
  }
  const adminOrigin = origin(args[1], "--admin-origin");
  const updateOrigin = origin(args[3], "--update-origin");
  if (adminOrigin === updateOrigin) throw new Error("Admin and update origins must be different HTTPS origins");
  return { adminOrigin, updateOrigin };
}

export async function smoke(config, fetchImpl = fetch) {
  const probes = [
    { name: "admin-access", url: `${config.adminOrigin}/api/admin/me`, method: "GET", allowed: [302, 401, 403] },
    { name: "update-metadata", url: `${config.updateOrigin}/v1/update/windows/x86_64/0.0.0`, method: "GET", allowed: [401, 403] },
    { name: "lease-renew", url: `${config.updateOrigin}/v1/lease/renew`, method: "POST", body: "{}", allowed: [401, 403] },
    { name: "fake-download", url: `${config.updateOrigin}/v1/download/smoke-release/smoke-ticket`, method: "GET", allowed: [401, 403, 404] },
    { name: "invalid-activation", url: `${config.updateOrigin}/v1/activate`, method: "POST", body: "{}", allowed: [400] },
  ];
  const results = [];
  for (const probe of probes) {
    const response = await fetchImpl(probe.url, {
      method: probe.method,
      redirect: "manual",
      headers: probe.body ? { accept: "application/json", "content-type": "application/json" } : { accept: "application/json" },
      body: probe.body,
      signal: AbortSignal.timeout(15_000),
    });
    try {
      await response.body?.cancel();
    } catch {
      // The status is sufficient; a cleanup failure must not expose or parse a response body.
    }
    results.push({ name: probe.name, url: probe.url, status: response.status });
    if (!probe.allowed.includes(response.status)) {
      throw new Error(`Unexpected status for ${probe.name}: ${response.status}`);
    }
  }
  return results;
}

export async function runSmoke(config, io = console, fetchImpl = fetch) {
  try {
    for (const result of await smoke(config, fetchImpl)) io.log(`${result.name} ${result.url} ${result.status}`);
    return 0;
  } catch (error) {
    io.error(error instanceof Error ? error.message : "Smoke check failed");
    return 1;
  }
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    const config = parseSmokeArgs(process.argv.slice(2));
    process.exitCode = await runSmoke(config);
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Smoke check failed");
    process.exitCode = 1;
  }
}
