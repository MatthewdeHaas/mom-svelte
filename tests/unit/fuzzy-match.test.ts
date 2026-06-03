import { describe, it, expect } from 'vitest';
import { levenshtein, similarity, findFuzzyWarnings } from '$lib/utils/fuzzy-match';

// ---------------------------------------------------------------------------
// levenshtein
// ---------------------------------------------------------------------------

describe('levenshtein', () => {
  it('returns 0 for identical strings', () => {
    expect(levenshtein('chicken', 'chicken')).toBe(0);
  });

  it('returns full length when one string is empty', () => {
    expect(levenshtein('', 'abc')).toBe(3);
    expect(levenshtein('hello', '')).toBe(5);
  });

  it('returns 0 for two empty strings', () => {
    expect(levenshtein('', '')).toBe(0);
  });

  it('handles single character substitution', () => {
    expect(levenshtein('cat', 'bat')).toBe(1);
  });

  it('handles single character insertion', () => {
    expect(levenshtein('cat', 'cats')).toBe(1);
  });

  it('handles single character deletion', () => {
    expect(levenshtein('cats', 'cat')).toBe(1);
  });

  it('computes correct distance for longer strings', () => {
    // "kitten" -> "sitting": 3 edits (k→s, e→i, +g)
    expect(levenshtein('kitten', 'sitting')).toBe(3);
  });

  it('is symmetric', () => {
    expect(levenshtein('abc', 'xyz')).toBe(levenshtein('xyz', 'abc'));
  });
});

// ---------------------------------------------------------------------------
// similarity
// ---------------------------------------------------------------------------

describe('similarity', () => {
  it('returns 1 for identical strings', () => {
    expect(similarity('chicken', 'chicken')).toBe(1);
  });

  it('returns 1 for two empty strings', () => {
    expect(similarity('', '')).toBe(1);
  });

  it('returns 0 for completely different strings of equal length', () => {
    // 'abc' vs 'xyz' = 3 edits / maxLen 3 = 0
    expect(similarity('abc', 'xyz')).toBe(0);
  });

  it('returns a value between 0 and 1 for partial matches', () => {
    const score = similarity('Chicken Breast', 'Chicken Beast');
    expect(score).toBeGreaterThan(0);
    expect(score).toBeLessThan(1);
  });

  it('produces higher scores for more similar strings', () => {
    const close = similarity('Chicken Breast', 'Chicken Beast');
    const far = similarity('Chicken Breast', 'Salmon Fillet');
    expect(close).toBeGreaterThan(far);
  });
});

// ---------------------------------------------------------------------------
// findFuzzyWarnings
// ---------------------------------------------------------------------------

describe('findFuzzyWarnings', () => {
  const existing = ['Chicken Breast', 'Salmon Fillet', 'Arborio Rice', 'Fresh Dill'];

  it('returns empty array when inputs exactly match existing (case-insensitive)', () => {
    const result = findFuzzyWarnings(['chicken breast', 'SALMON FILLET'], existing);
    expect(result).toHaveLength(0);
  });

  it('returns empty array when inputs are empty', () => {
    expect(findFuzzyWarnings([], existing)).toHaveLength(0);
  });

  it('returns empty array when existing list is empty', () => {
    expect(findFuzzyWarnings(['Chicken Beast'], [])).toHaveLength(0);
  });

  it('flags a typo that is close to an existing ingredient', () => {
    const result = findFuzzyWarnings(['Chicken Beast'], existing);
    expect(result).toHaveLength(1);
    expect(result[0].input).toBe('Chicken Beast');
    expect(result[0].match).toBe('Chicken Breast');
    expect(result[0].score).toBeGreaterThan(0.65);
  });

  it('does not flag strings shorter than minLength', () => {
    const result = findFuzzyWarnings(['Egg'], ['Egg White', 'Egg Yolk']);
    expect(result).toHaveLength(0);
  });

  it('respects custom minLength parameter', () => {
    const result = findFuzzyWarnings(['Eg'], ['Egg'], 0.65, 2);
    expect(result).toHaveLength(1);
    expect(result[0].match).toBe('Egg');
  });

  it('does not flag inputs that are too dissimilar', () => {
    const result = findFuzzyWarnings(['Unicorn Tears'], existing);
    expect(result).toHaveLength(0);
  });

  it('deduplicates input names (reports each unique input only once)', () => {
    const result = findFuzzyWarnings(
      ['Chicken Beast', 'Chicken Beast', 'chicken beast'],
      existing
    );
    expect(result).toHaveLength(1);
  });

  it('respects custom threshold', () => {
    // "Salmn Fillet" vs "Salmon Fillet" — close but test with very high threshold
    const strict = findFuzzyWarnings(['Salmn Fillet'], existing, 0.99);
    expect(strict).toHaveLength(0);

    const lenient = findFuzzyWarnings(['Salmn Fillet'], existing, 0.5);
    expect(lenient).toHaveLength(1);
  });

  it('preserves original casing of matched ingredient', () => {
    const result = findFuzzyWarnings(['arborio ric'], existing);
    expect(result).toHaveLength(1);
    expect(result[0].match).toBe('Arborio Rice');
  });

  it('handles multiple fuzzy matches across different inputs', () => {
    const result = findFuzzyWarnings(
      ['Chicken Beast', 'Salmn Fillet'],
      existing
    );
    expect(result).toHaveLength(2);
    const inputs = result.map((r) => r.input);
    expect(inputs).toContain('Chicken Beast');
    expect(inputs).toContain('Salmn Fillet');
  });
});
