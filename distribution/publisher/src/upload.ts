import { createReadStream } from "node:fs";
import { createHash } from "node:crypto";
import { Transform } from "node:stream";
import { S3Client } from "@aws-sdk/client-s3";
import { Upload } from "@aws-sdk/lib-storage";
import { NodeHttpHandler, type NodeHttpHandlerOptions } from "@smithy/node-http-handler";
import type { PublisherCredentials } from "./credentials.js";
import type { ReleaseManifest } from "./manifest.js";

type UploadOptions = ConstructorParameters<typeof Upload>[0];
export const R2_CONNECTION_TIMEOUT_MS = 10000;
export const R2_REQUEST_TIMEOUT_MS = 5 * 60 * 1000;
export const R2_SOCKET_TIMEOUT_MS = 5 * 60 * 1000;
export interface UploadAdapter {
  client?: S3Client;
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
  try {
    const uploader = (adapter.createUpload ?? (options => new Upload(options)))({
      client, params: { Bucket: credentials.bucket, Key: manifest.objectKey, Body: body,
        ContentLength: manifest.size, ContentType: "application/octet-stream", Metadata: { sha256: manifest.sha256 } },
      partSize: 16 * 1024 * 1024, leavePartsOnError: false,
    });
    await uploader.done();
    if (size !== manifest.size || hash.digest("hex") !== manifest.sha256) throw new Error("BUNDLE_CHANGED");
  } catch { throw new Error("UPLOAD_FAILED: no release was created; check the bundle and connection before retrying."); }
  finally { source.destroy(); body.destroy(); if (!adapter.client) client.destroy(); }
}
