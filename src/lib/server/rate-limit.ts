/**
 * Simple in-memory sliding-window rate limiter.
 * Not shared across instances — fine for a single-dyno Railway deployment.
 */

interface RateLimitEntry {
  timestamps: number[];
}

const stores = new Map<string, Map<string, RateLimitEntry>>();

/**
 * Check if a key is rate-limited within a named bucket.
 * @returns true if the request should be BLOCKED
 */
export function isRateLimited(
  bucket: string,
  key: string,
  maxRequests: number,
  windowMs: number
): boolean {
  if (!stores.has(bucket)) stores.set(bucket, new Map());
  const store = stores.get(bucket)!;

  const now = Date.now();
  const entry = store.get(key) ?? { timestamps: [] };

  // Remove expired timestamps
  entry.timestamps = entry.timestamps.filter((t) => now - t < windowMs);

  if (entry.timestamps.length >= maxRequests) {
    return true; // blocked
  }

  entry.timestamps.push(now);
  store.set(key, entry);
  return false; // allowed
}
