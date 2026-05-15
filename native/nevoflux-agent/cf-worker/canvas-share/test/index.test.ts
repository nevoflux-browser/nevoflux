import { describe, it, expect } from 'vitest';
import { isValidShareId, isValidOwnerTokenHash, base64UrlDecode } from '../src/utils/validation';

describe('isValidShareId', () => {
  it('accepts valid share_id', () => {
    expect(isValidShareId('abc123xyz0')).toBe(true);
  });

  it('accepts all-zero share_id', () => {
    expect(isValidShareId('0000000000')).toBe(true);
  });

  it('accepts share_id with all lowercase Crockford chars', () => {
    expect(isValidShareId('0123456789')).toBe(true);
    expect(isValidShareId('abcdefghjk')).toBe(true);
    expect(isValidShareId('mnpqrstvwx')).toBe(true);
  });

  it('rejects wrong length (too short)', () => {
    expect(isValidShareId('abc123')).toBe(false);
  });

  it('rejects wrong length (too long)', () => {
    expect(isValidShareId('abc123xyz0a')).toBe(false);
  });

  it('rejects empty string', () => {
    expect(isValidShareId('')).toBe(false);
  });

  it('rejects invalid chars (I L O U)', () => {
    expect(isValidShareId('abcdeilou1')).toBe(false);
    expect(isValidShareId('i000000000')).toBe(false);
    expect(isValidShareId('l000000000')).toBe(false);
    expect(isValidShareId('o000000000')).toBe(false);
    expect(isValidShareId('u000000000')).toBe(false);
  });

  it('rejects uppercase chars', () => {
    expect(isValidShareId('ABC123XYZ0')).toBe(false);
  });

  it('rejects chars outside base32', () => {
    expect(isValidShareId('abc-123xy0')).toBe(false);
    expect(isValidShareId('abc_123xy0')).toBe(false);
  });
});

describe('isValidOwnerTokenHash', () => {
  it('accepts valid 64-char hex', () => {
    expect(isValidOwnerTokenHash('a'.repeat(64))).toBe(true);
    expect(isValidOwnerTokenHash('0123456789abcdef'.repeat(4))).toBe(true);
  });

  it('rejects wrong hash length (too short)', () => {
    expect(isValidOwnerTokenHash('a'.repeat(63))).toBe(false);
  });

  it('rejects wrong hash length (too long)', () => {
    expect(isValidOwnerTokenHash('a'.repeat(65))).toBe(false);
  });

  it('rejects uppercase hex', () => {
    expect(isValidOwnerTokenHash('A'.repeat(64))).toBe(false);
  });

  it('rejects non-hex chars', () => {
    expect(isValidOwnerTokenHash('g'.repeat(64))).toBe(false);
    expect(isValidOwnerTokenHash('z'.repeat(64))).toBe(false);
  });

  it('rejects empty string', () => {
    expect(isValidOwnerTokenHash('')).toBe(false);
  });
});

describe('base64UrlDecode', () => {
  it('decodes simple base64url string', () => {
    const bytes = base64UrlDecode('AAAA');
    expect(bytes.length).toBe(3);
    expect(bytes[0]).toBe(0);
    expect(bytes[1]).toBe(0);
    expect(bytes[2]).toBe(0);
  });

  it('decodes base64url with url-safe chars', () => {
    // 'SGVsbG8' = "Hello" in base64 (no padding)
    const bytes = base64UrlDecode('SGVsbG8');
    expect(bytes.length).toBe(5);
    expect(new TextDecoder().decode(bytes)).toBe('Hello');
  });

  it('handles missing padding', () => {
    const bytes = base64UrlDecode('SGVsbG8');
    expect(new TextDecoder().decode(bytes)).toBe('Hello');
  });
});
