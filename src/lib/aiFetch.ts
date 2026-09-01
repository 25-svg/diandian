import { fetch as tauriFetch } from "@tauri-apps/plugin-http";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: any;
  }
}

/**
 * OpenAI SDK may pass a Request whose body is a WebView ReadableStream.
 * Tauri's HTTP plugin cannot reliably forward that stream on Windows, so
 * materialize it as text before crossing IPC.
 */
export async function aiFetch(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
    return fetch(input, init);
  }

  const request = input instanceof Request ? input : new Request(input, init);
  const method = request.method.toUpperCase();
  const body = method === "GET" || method === "HEAD"
    ? undefined
    : await request.clone().text();

  const response = await tauriFetch(request.url, {
    method,
    headers: Object.fromEntries(request.headers.entries()),
    body,
    signal: init?.signal,
  });

  // Provider SDKs often collapse useful 4xx details into "Failed to fetch"
  // when a custom desktop fetch implementation is used. Surface the response
  // body so the user can distinguish an invalid model, key, quota or payload.
  if (!response.ok) {
    const details = await response.clone().text();
    throw new Error(
      `MiniMax HTTP ${response.status}${details ? `: ${details}` : ""}`,
    );
  }

  return response;
}
