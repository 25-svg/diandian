export interface Env {
  ARTIFACTS?: R2Bucket;
  DB: D1Database;
  GITHUB_RELEASES_REPOSITORY?: string;
  GITHUB_RELEASES_TOKEN?: string;
}
