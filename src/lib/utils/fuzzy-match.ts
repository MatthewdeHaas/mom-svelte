/**
 * Fuzzy string matching utilities for ingredient name deduplication.
 *
 * Uses Levenshtein distance with normalized scoring to detect near-duplicate
 * ingredient names during recipe import.
 */

/**
 * Compute Levenshtein edit distance between two strings.
 * Standard dynamic-programming implementation, O(n*m).
 */
export function levenshtein(a: string, b: string): number {
  const la = a.length;
  const lb = b.length;
  if (la === 0) return lb;
  if (lb === 0) return la;

  // Use a single row buffer for space efficiency
  let prev = Array.from({ length: lb + 1 }, (_, i) => i);
  let curr = new Array<number>(lb + 1);

  for (let i = 1; i <= la; i++) {
    curr[0] = i;
    for (let j = 1; j <= lb; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(
        prev[j] + 1,        // deletion
        curr[j - 1] + 1,    // insertion
        prev[j - 1] + cost  // substitution
      );
    }
    [prev, curr] = [curr, prev];
  }

  return prev[lb];
}

/**
 * Normalized similarity score between 0 and 1.
 * 1 = identical, 0 = completely different.
 */
export function similarity(a: string, b: string): number {
  const maxLen = Math.max(a.length, b.length);
  if (maxLen === 0) return 1;
  return 1 - levenshtein(a, b) / maxLen;
}

export interface FuzzyMatch {
  input: string;
  match: string;
  score: number;
}

/**
 * For each input name that does NOT exactly match an existing name (case-insensitive),
 * find the closest existing name if similarity exceeds the threshold.
 *
 * @param inputs - ingredient names from the uploaded file
 * @param existing - ingredient names already in the database
 * @param threshold - minimum similarity score to flag (default 0.65)
 * @param minLength - minimum string length to attempt fuzzy matching (default 4)
 * @returns Map of input names to their closest existing match
 */
export function findFuzzyWarnings(
  inputs: string[],
  existing: string[],
  threshold = 0.65,
  minLength = 4
): FuzzyMatch[] {
  const warnings: FuzzyMatch[] = [];
  const existingLower = existing.map((n) => n.toLowerCase());

  // Deduplicate inputs
  const seen = new Set<string>();

  for (const input of inputs) {
    const lower = input.toLowerCase().trim();

    // Skip if already processed, exact match exists, or too short
    if (seen.has(lower)) continue;
    seen.add(lower);

    if (lower.length < minLength) continue;
    if (existingLower.includes(lower)) continue;

    // Find closest match
    let bestScore = 0;
    let bestMatch = '';

    for (let i = 0; i < existingLower.length; i++) {
      const score = similarity(lower, existingLower[i]);
      if (score > bestScore) {
        bestScore = score;
        bestMatch = existing[i]; // preserve original casing
      }
    }

    if (bestScore >= threshold) {
      warnings.push({ input, match: bestMatch, score: bestScore });
    }
  }

  return warnings;
}
