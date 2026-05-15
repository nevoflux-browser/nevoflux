/**
 * Input validation utilities for Canvas Share.
 */

/** Valid Crockford base32 characters (lowercase). */
const CROCKFORD_CHARS = '0123456789abcdefghjkmnpqrstvwxyz';

/** Share ID length (10 chars = 48 bits). */
export const SHARE_ID_LEN = 10;

/**
 * Validate a share ID string.
 *
 * Must be exactly 10 characters of lowercase Crockford base32.
 * The client is expected to normalize before sending.
 */
export function isValidShareId(id: string): boolean {
  if (id.length !== SHARE_ID_LEN) return false;
  return [...id].every((c) => CROCKFORD_CHARS.includes(c));
}

/**
 * Validate an owner token hash (should be a 64-char hex string = SHA-256).
 */
export function isValidOwnerTokenHash(hash: string): boolean {
  return /^[0-9a-f]{64}$/.test(hash);
}

/**
 * Hash an IP address using SHA-256 for privacy-preserving abuse tracking.
 */
export async function hashIp(ip: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(ip);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Verify an owner token against a stored hash.
 *
 * Recomputes SHA-256(share_id || base64url_decode(owner_token))
 * and compares against the stored hash.
 */
export async function verifyOwnerToken(
  shareId: string,
  ownerTokenB64: string,
  storedHash: string
): Promise<boolean> {
  try {
    // Decode base64url owner token
    const tokenBytes = base64UrlDecode(ownerTokenB64);
    if (tokenBytes.length !== 32) return false;

    // Compute SHA-256(share_id || owner_token)
    const encoder = new TextEncoder();
    const shareIdBytes = encoder.encode(shareId);
    const combined = new Uint8Array(shareIdBytes.length + tokenBytes.length);
    combined.set(shareIdBytes);
    combined.set(tokenBytes, shareIdBytes.length);

    const hashBuffer = await crypto.subtle.digest('SHA-256', combined);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const computedHash = hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');

    return computedHash === storedHash;
  } catch {
    return false;
  }
}

/** Decode a base64url string (no padding) to Uint8Array. */
export function base64UrlDecode(input: string): Uint8Array {
  // Convert base64url to standard base64
  let base64 = input.replace(/-/g, '+').replace(/_/g, '/');
  // Add padding
  while (base64.length % 4 !== 0) {
    base64 += '=';
  }
  const binaryString = atob(base64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes;
}
