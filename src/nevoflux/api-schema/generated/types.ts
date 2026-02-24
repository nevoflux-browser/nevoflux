/**
 * NevoFlux Browser API Types
 * Version: 1.0.0
 * Generated: 2026-01-16T15:36:51.856Z
 *
 * DO NOT EDIT - This file is auto-generated from nevoflux-api.json
 */

// ========== Common Types ==========

export interface ApiResult {
  success: boolean;
  error?: ApiError;
}

export interface ApiError {
  code: number;
  message: string;
  recoverable: boolean;
  suggestion?: string;
}

export interface NavigationOptions {
  timeout?: number;
  waitUntil?: string;
}

export interface NavigationResult {
  success: boolean;
  url: string;
}

export interface ClickOptions {
  button?: string;
  clickCount?: number;
  delay?: number;
  position?: Position;
  force?: boolean;
}

export interface TypeOptions {
  delay?: number;
}

export interface Position {
  x: number;
  y: number;
}

export interface SnapshotOptions {
  interactive?: boolean;
  compact?: boolean;
  depth?: number;
  root?: string;
}

export interface SnapshotResult {
  tree: string;
  refs: Record<string, any>;
}

export interface ElementRef {
  role: string;
  name?: string;
  selector: string;
  tagName?: string;
}

export interface ScreenshotOptions {
  fullPage?: boolean;
  type?: string;
  quality?: number;
}

export interface ScreenshotResult {
  image: string;
  mimeType: string;
  width: number;
  height: number;
}

export interface WaitOptions {
  timeout?: number;
  state?: string;
}

export interface PrivacyConfig {
  enabled: boolean;
  filters?: FilterConfig;
  mode?: string;
  scope?: string;
}

export interface FilterConfig {
  phone?: boolean;
  idCard?: boolean;
  email?: boolean;
  bankCard?: boolean;
  address?: boolean;
  name?: boolean;
}

export interface FilterOptions {
  phone?: boolean;
  idCard?: boolean;
  email?: boolean;
  bankCard?: boolean;
}

export interface FilterResult {
  text: string;
  filteredCount: number;
  filteredItems?: FilteredItem[];
}

export interface FilteredItem {
  type: string;
  position: number;
}

// ========== API Namespaces ==========

export interface NevofluxAPI {
  extraction: {
    getText(selector?: string): Promise<string>;
    getHtml(selector?: string): Promise<string>;
    getValue(selector: string): Promise<string>;
    getAttribute(selector: string, attribute: string): Promise<string>;
    getUrl(): Promise<string>;
    getTitle(): Promise<string>;
    snapshot(options?: SnapshotOptions): Promise<SnapshotResult>;
    screenshot(options?: ScreenshotOptions): Promise<ScreenshotResult>;
  };
  state: {
    isVisible(selector: string): Promise<boolean>;
    isEnabled(selector: string): Promise<boolean>;
    isChecked(selector: string): Promise<boolean>;
    exists(selector: string): Promise<boolean>;
  };
  navigation: {
    open(url: string, options?: NavigationOptions): Promise<NavigationResult>;
    reload(options?: NavigationOptions): Promise<ApiResult>;
    back(): Promise<ApiResult>;
    forward(): Promise<ApiResult>;
    close(): Promise<ApiResult>;
  };
  interaction: {
    click(selector: string, options?: ClickOptions): Promise<ApiResult>;
    type(selector: string, text: string, options?: TypeOptions): Promise<ApiResult>;
    fill(selector: string, text: string): Promise<ApiResult>;
    select(selector: string, value: string): Promise<ApiResult>;
    check(selector: string): Promise<ApiResult>;
    uncheck(selector: string): Promise<ApiResult>;
    hover(selector: string): Promise<ApiResult>;
    scroll(direction: string, amount?: number): Promise<ApiResult>;
    scrollIntoView(selector: string): Promise<ApiResult>;
  };
  wait: {
    forSelector(selector: string, options?: WaitOptions): Promise<ApiResult>;
    forText(text: string, options?: WaitOptions): Promise<ApiResult>;
    forUrl(pattern: string, options?: WaitOptions): Promise<ApiResult>;
    forTimeout(ms: number): Promise<ApiResult>;
  };
  privacy: {
    getConfig(): Promise<PrivacyConfig>;
    setConfig(config: PrivacyConfig): Promise<PrivacyConfig>;
    filter(text: string, options?: FilterOptions): Promise<FilterResult>;
  };
}

// ========== Mode Definitions ==========

export type SessionMode = 'chat' | 'agent' | 'browser_use';

export const API_BY_MODE: Record<SessionMode, string[]> = {
  chat: ['extraction', 'state', 'privacy'],
  agent: ['extraction', 'state', 'privacy'],
  browser_use: ['extraction', 'state', 'navigation', 'interaction', 'wait', 'privacy'],
};
