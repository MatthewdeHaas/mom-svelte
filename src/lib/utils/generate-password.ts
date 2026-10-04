import { randomBytes } from 'node:crypto';

/**
 * Generate a URL-safe 14-char password with ~84 bits of entropy.
 *
 * Used when the admin provisions a client user and must share the password
 * out-of-band (e.g. the client doesn't have an email account to receive an
 * invite link). Base64url avoids `+`/`/`/`=` so the password copy-pastes
 * cleanly without URL encoding surprises.
 */
export function generatePassword(): string {
  // 11 random bytes → 14.6 base64url chars → slice to exactly 14.
  return randomBytes(11).toString('base64url').slice(0, 14);
}
