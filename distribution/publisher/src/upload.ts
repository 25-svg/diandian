import { createReadStream } from "node:fs";
import { createHash } from "node:crypto";
import { Transform } from "node:stream";
import { AbortMultipartUploadCommand, S3Client } from "@aws-sdk/client-s3";
import { Upload } from "@aws-sdk/lib-storage";
import { NodeHttpHandler, type NodeHttpHandlerOptions } from "@smithy/node-http-handler";
import type { PublisherCredentials } from "./credentials.js";
import type { ReleaseManifest } from "./manifest.js";

type UploadOptions = ConstructorParameters<typeof Upload>[0];
export const R2_CONNECTION_TIMEOUT_MS = 10000;
export const R2_REQUEST_TIMEOUT_MS = 5 * 60 * 1000;
export const R2_SOCKET_TIMEOUT_MS = 5 * 60 * 1000;
// Allows a 4 GiB bundle at roughly 5 Mbps, including multipart/retry overhead.
export const R2_UPLOAD_TIMEOUT_MS = 2 * 60 * 60 * 1000;
export const R2_ABORT_TIMEOUT_MS = 10000;
export interface UploadAdapter {
  client?: S3Client;
  operationTimeoutMs?: number;
  httpHandlerOptions?: Pick<NodeHttpHandlerOptions, "connectionTimeout" | "requestTimeout" | "socketTimeout">;
  createUpload?: (options: UploadOptions) => { done(): Promise<unknown> };
}
export async function uploadBundle(path: string, manifest: ReleaseManifest, credentials: PublisherCredentials, adapter: UploadAdapter = {}): Promise<void> {
  const client = adapter.client ?? new S3Client({
    endpoint: credentials.endpoint, region: credentials.region, forcePathStyle: true,
    credentials: { accessKeyId: credentials.accessKeyId, secretAccessKey: credentials.secretAccessKey },
    maxAttempts: 3, retryMode: "standard",
    requestHandler: new NodeHttpHandler({
      connectionTimeout: adapter.httpHandlerOptions?.connectionTimeout ?? R2_CONNECTION_TIMEOUT_MS,
      requestTimeout: adapter.httpHandlerOptions?.requestTimeout ?? R2_REQUEST_TIMEOUT_MS,
      socketTimeout: adapter.httpHandlerOptions?.socketTimeout ?? R2_SOCKET_TIMEOUT_MS,
      // Smithy's request deadline otherwise only logs a warning and keeps waiting.
      throwOnRequestTimeout: true,
    }),
  });
  const source = createReadStream(path), hash = createHash("sha256"); let size = 0;
  const body = new Transform({ transform(chunk: Buffer, _encoding, callback) {
    size += chunk.length;
    if (size > manifest.size) { callback(new Error("BUNDLE_CHANGED")); return; }
    hash.update(chunk); callback(null, chunk);
  } });
  source.on("error", () => body.destroy(new Error("BUNDLE_READ_FAILED")));
  source.pipe(body);
  const controller = new AbortController();
  const activeRequests = new Set<Promise<unknown>>();
  // lib-storage 3.1130.0 races its controller in done(), but does not pass its
  // signal to client.send(). Scope signal forwarding to this upload's client
  // view, preserving an injected/shared client's behavior outside this call.
  const uploadClient = new Proxy(client, { get(target, property, receiver) {
    if (property !== "send") return Reflect.get(target, property, receiver);
    return (command: Parameters<S3Client["send"]>[0]) => {
      const cleanup = command instanceof AbortMultipartUploadCommand;
      const cleanupController = cleanup ? new AbortController() : undefined;
      const cleanupTimer = cleanupController ? setTimeout(() => cleanupController.abort(), R2_ABORT_TIMEOUT_MS) : undefined;
      cleanupTimer?.unref();
      const request = target.send(command, { abortSignal: (cleanupController ?? controller).signal });
      activeRequests.add(request);
      const settled = () => { clearTimeout(cleanupTimer); activeRequests.delete(request); };
      void request.then(settled, settled);
      return request;
    };
  } });
  const deadline = setTimeout(() => {
    controller.abort();
    source.destroy(); body.destroy();
  }, adapter.operationTimeoutMs ?? R2_UPLOAD_TIMEOUT_MS);
  deadline.unref();
  try {
    const uploader = (adapter.createUpload ?? (options => new Upload(options)))({
      client: uploadClient, abortController: controller, params: { Bucket: credentials.bucket, Key: manifest.objectKey, Body: body,
        ContentLength: manifest.size, ContentType: "application/octet-stream", Metadata: { sha256: manifest.sha256 } },
      partSize: 16 * 1024 * 1024, leavePartsOnError: false,
    });
    await uploader.done();
    if (size !== manifest.size || hash.digest("hex") !== manifest.sha256) throw new Error("BUNDLE_CHANGED");
  } catch { throw new Error("UPLOAD_FAILED: no release was created; check the bundle and connection before retrying."); }
  finally {
    clearTimeout(deadline);
    source.destroy(); body.destroy();
    // done() can reject before SDK workers finish aborting. Drain their requests
    // and allow multipart cleanup to start before destroying our owned client.
    do {
      await Promise.allSettled([...activeRequests]);
      await new Promise<void>(resolve => setImmediate(resolve));
    } while (activeRequests.size);
    if (!adapter.client) client.destroy();
  }
}
