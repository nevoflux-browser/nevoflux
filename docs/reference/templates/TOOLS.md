# NevoFlux Tools

> Protection level: L1 | Auto-learning — updated automatically from tool usage

## Tool Usage Preferences

- Prefer native MCP tools over browser-based workarounds
- Cache tool schemas to reduce discovery overhead
- Fall back to manual browser interaction when tools are unavailable

## Browser Automation

### Selector Strategy

1. Prefer `data-testid` and `aria-label` attributes
2. Fall back to semantic selectors (role, text content)
3. Use CSS selectors as last resort
4. Avoid XPath unless structure is deeply nested

### SPA Handling

- Wait for network idle before interacting with dynamic content
- Detect client-side routing and re-evaluate selectors after navigation
- Handle loading spinners and skeleton screens with configurable timeouts

## Runtime Parameters

| Parameter            | Default | Description                          |
| -------------------- | ------- | ------------------------------------ |
| request_timeout_ms   | 30000   | HTTP request timeout                 |
| retry_count          | 3       | Max retries for transient failures   |
| screenshot_quality   | 80      | JPEG quality for screenshots (0-100) |
| max_concurrent_tools | 4       | Max parallel tool invocations        |

## Site Adaptation Graph

- (Auto-populated: maps domain patterns to successful interaction strategies)
- (Entries include selector preferences, wait strategies, and known quirks)
