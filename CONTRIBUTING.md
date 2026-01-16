# Contributing to NevoFlux

Thank you for your interest in contributing to NevoFlux! This document provides guidelines for contributing to the project.

## Table of Contents

- [Before You Contribute](#before-you-contribute)
- [Pull Request Template](#pull-request-template)
- [Review Process](#review-process)
- [Commit Conventions](#commit-conventions)
- [First-Time Contributors](#first-time-contributors)
- [Reporting Issues](#reporting-issues)
- [Security Vulnerabilities](#security-vulnerabilities)
- [Code of Conduct](#code-of-conduct)
- [License Agreement](#license-agreement)
- [Getting Help](#getting-help)

## Before You Contribute

### Prerequisites

1. Read the [README.md](README.md) for project overview
2. Review [CLAUDE.md](CLAUDE.md) for technical architecture and development workflow
3. Set up your development environment:
   ```bash
   npm install
   npm run download
   npm run import
   npm run bootstrap
   npm run build
   ```
4. Familiarize yourself with the [NevoFlux patch system](CLAUDE.md#nevoflux-development-workflow)

### Ways to Contribute

- **Bug Fixes**: Fix existing issues or defects
- **Features**: Add new functionality to the browser or agent
- **Documentation**: Improve guides, comments, or examples
- **Tests**: Enhance test coverage
- **Performance**: Optimize browser or agent performance
- **UI/UX**: Improve visual design or user interactions
- **Localization**: Add translations
- **Security**: Report vulnerabilities (see [Security](#security-vulnerabilities))

## Pull Request Template

When submitting a pull request, please include the following information:

### PR Type

Select **one** primary category:
- [ ] **Bug Fix** - Fixes an existing issue or defect
- [ ] **Feature** - Adds new functionality
- [ ] **Refactor** - Code restructuring without behavior changes
- [ ] **Performance** - Performance improvements
- [ ] **Documentation** - Documentation updates
- [ ] **Tests** - Test additions or improvements
- [ ] **Chore** - Build process, dependencies, or tooling changes

### Affected Components

Select **all** that apply:
- [ ] **Browser Core** - Changes to `src/zen/` (requires patch system)
- [ ] **Rust Agent** - Changes to `src/nevoflux/crates/`
- [ ] **WebExtension** - Changes to `src/nevoflux/extensions/nevoflux-agent/`
- [ ] **Build System** - Changes to build scripts, configs, or dependencies
- [ ] **UI/UX** - Visual or interaction changes
- [ ] **Documentation** - CLAUDE.md, README.md, or code comments

### Description

**Summary:**
<!-- Brief description of what this PR does (1-2 sentences) -->

**Motivation:**
<!-- Why is this change needed? What problem does it solve? -->

**Related Issues:**
<!-- Link to related issues: Fixes #123, Closes #456, Relates to #789 -->

### Implementation Details

**For Patch System Changes:**
- [ ] Changes exported as patches using `./scripts/export-nevoflux-patches.sh`
- [ ] Patches applied successfully with `npm run import`
- [ ] `src/zen/` directory reverted to clean state before commit
- [ ] Patch files follow naming convention: `<file>-<ext>.patch`

**For Rust Agent Changes:**
- [ ] Code formatted with `cargo fmt`
- [ ] No clippy warnings (`cargo clippy`)
- [ ] All tests pass (`cargo test`)
- [ ] Added/updated documentation comments (`///` or `//!`)

**For WebExtension Changes:**
- [ ] Follows [UI/UX Design Guidelines](CLAUDE.md#uiux-design-guidelines)
- [ ] Uses Zen CSS variables (no hardcoded colors)
- [ ] Tested in light and dark themes
- [ ] Keyboard navigation works correctly
- [ ] ARIA labels added for accessibility

### Testing

**Manual Testing:**
<!-- Describe how you tested this change -->
- [ ] Tested on Linux / macOS / Windows (specify)
- [ ] Tested with light and dark themes
- [ ] Tested in private browsing mode (if applicable)
- [ ] Tested keyboard navigation (if UI change)

**Automated Tests:**
- [ ] Added new tests for new functionality
- [ ] All existing tests pass (`npm run test`)
- [ ] Rust tests pass (`cargo test` in `src/nevoflux/crates/`)

### Screenshots/Videos

<!-- If UI changes, include before/after screenshots or demo videos -->

### Checklist

**Code Quality:**
- [ ] Code follows project [Code Style](CLAUDE.md#code-style) guidelines
- [ ] All comments are written in English
- [ ] No console.log or debug statements left in code
- [ ] Error handling is comprehensive
- [ ] No new ESLint/Prettier warnings

**Security:**
- [ ] No hardcoded credentials or API keys
- [ ] No XSS, SQL injection, or command injection vulnerabilities
- [ ] User input is properly validated and sanitized
- [ ] No eval() or unsafe dynamic code execution

**Performance:**
- [ ] No unnecessary re-renders or layout thrashing (for UI changes)
- [ ] Async operations are properly managed (no blocking operations)
- [ ] No memory leaks introduced

**Documentation:**
- [ ] Added/updated code comments for complex logic
- [ ] Updated CLAUDE.md if changing architecture or workflow
- [ ] Updated README.md if changing user-facing features
- [ ] Added JSDoc/Rustdoc for new public APIs

**License:**
- [ ] All new files include MPL 2.0 license header
- [ ] No third-party code without compatible license

## Review Process

All pull requests go through the following review process:

### 1. Automated Checks (CI/CD)

Before human review, automated checks must pass:

**Build Checks:**
- Clean build succeeds (`npm run build`)
- No build warnings or errors
- Patch application succeeds (for patch system changes)

**Code Quality:**
- ESLint passes (`npm run lint`)
- Prettier formatting passes (`npm run pretty`)
- License headers present (`npm run lc`)
- Rust clippy passes (for Rust changes)
- Rust formatting passes (`cargo fmt --check`)

**Tests:**
- All mochitest suites pass
- Rust unit tests pass
- Integration tests pass (if applicable)

### 2. Code Review Checklist

Reviewers should verify the following based on PR type:

**For All PRs:**
- Code is clear, readable, and follows project conventions
- Changes are well-scoped (one logical change per PR)
- Commit messages follow [Commit Conventions](#commit-conventions)
- No unrelated changes (formatting, refactoring) mixed with functional changes
- Security best practices followed (OWASP Top 10)
- Error handling is comprehensive and appropriate

**For Patch System Changes (`src/zen/`):**
- Patches are minimal and focused
- Patches follow directory structure convention
- Original `src/zen/` files NOT committed
- Patches can be applied cleanly on current main branch
- Changes don't break upstream sync compatibility
- NevoFlux comments clearly marked (e.g., `// NevoFlux: ...`)

**For Rust Agent Changes:**
- Uses idiomatic Rust patterns (Result, Option, match)
- Error types are descriptive and well-structured
- Async code properly uses tokio runtime
- No unsafe code without justification
- Documentation comments complete and accurate
- No panics in production code (use Result instead)
- Proper lifetime annotations where needed
- Dependencies justified and minimal

**For WebExtension Changes:**
- Follows [UI/UX Design Guidelines](CLAUDE.md#uiux-design-guidelines)
- Uses Zen CSS variables for all colors
- Responsive design works at different sidebar widths
- Accessibility requirements met (ARIA, keyboard nav)
- Light/dark theme support verified
- No hardcoded strings (localization ready)
- Native messaging protocol used correctly

**For UI/UX Changes:**
- Design is consistent with Zen Browser aesthetics
- Animations use transform/opacity (GPU-accelerated)
- Spacing follows `--zen-space-*` convention
- Typography follows size/weight guidelines
- Contrast ratios meet WCAG 2.1 AA standards
- Focus indicators visible and accessible
- Reduced motion preference respected

**For Performance-Critical Changes:**
- Profiling data shows improvement
- No performance regressions in common workflows
- Memory usage is acceptable
- Async operations don't block main thread
- Large lists use virtualization if needed

### 3. Review Timeline

- **Initial Review**: Within 2-3 business days
- **Follow-up**: Within 1-2 business days after changes
- **Approval**: At least one maintainer approval required
- **Complex PRs**: May require 2+ maintainer approvals

### 4. Review Feedback

**For Contributors:**
- Address all review comments or explain why change isn't needed
- Use "Request re-review" when ready for another look
- Keep discussion focused and professional
- Update PR description if scope changes

**For Reviewers:**
- Be respectful and constructive
- Distinguish between blocking issues and suggestions
- Provide specific examples or code snippets
- Approve when all blocking issues are resolved

### 5. Merge Requirements

Before merging, ensure:
- All automated checks pass
- At least one maintainer approval
- All review comments addressed
- No merge conflicts with main branch
- Commits are clean (squash if needed)
- Final smoke test performed

**Merge Strategy:**
- **Squash and Merge**: Default for most PRs (single logical change)
- **Rebase and Merge**: For multi-commit PRs with clean history
- **Merge Commit**: For large feature branches

## Commit Conventions

NevoFlux follows [Conventional Commits](https://www.conventionalcommits.org/) specification with project-specific extensions.

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Example:**
```
feat(agent): add LLM streaming support

Implement streaming responses from LLM providers using Server-Sent Events.
This improves UX by showing partial responses as they arrive.

- Add StreamingClient trait for LLM providers
- Implement SSE parser in nevoflux-llm crate
- Update sidebar UI to display streaming messages

Fixes #123
```

### Type

Use **one** of the following types:

| Type | Description | Example |
|------|-------------|---------|
| `feat` | New feature or enhancement | `feat(sidebar): add conversation history` |
| `fix` | Bug fix | `fix(agent): resolve native messaging timeout` |
| `patch` | Patch system changes | `patch(startup): integrate NevoFlux agent init` |
| `refactor` | Code restructuring | `refactor(llm): extract common client logic` |
| `perf` | Performance improvement | `perf(ui): virtualize message list` |
| `style` | Code style/formatting | `style(rust): run cargo fmt` |
| `test` | Test additions/changes | `test(agent): add integration tests` |
| `docs` | Documentation changes | `docs(claude): update contributing guide` |
| `build` | Build system changes | `build(deps): update tokio to 1.35` |
| `ci` | CI/CD changes | `ci(github): add clippy check` |
| `chore` | Maintenance tasks | `chore(git): update .gitignore` |

### Scope

Specify the affected component:

**Browser Components:**
- `startup` - Browser initialization (`src/zen/common/`)
- `workspaces` - Workspace management
- `ui` - General UI changes
- `tabs` - Tab functionality
- `split-view` - Split view feature
- `glance` - Glance preview
- `urlbar` - Address bar
- `mods` - Plugin system

**NevoFlux Components:**
- `agent` - Rust native agent (`src/nevoflux/crates/nevoflux-agent/`)
- `llm` - LLM client library (`nevoflux-llm`)
- `mcp` - MCP protocol client (`nevoflux-mcp`)
- `wasm` - WASM runtime (`nevoflux-wasm`)
- `browser-control` - Browser automation (`nevoflux-browser`)
- `extension` - WebExtension (`src/nevoflux/extensions/`)
- `sidebar` - Sidebar UI
- `content` - Content scripts
- `background` - Background scripts

**Project-Wide:**
- `patch` - Patch system
- `build` - Build configuration
- `deps` - Dependencies
- `config` - Configuration files
- `tests` - Test infrastructure

**Optional scope** for small changes affecting multiple components.

### Subject

- Use imperative mood ("add" not "added" or "adds")
- Don't capitalize first letter
- No period at the end
- Maximum 72 characters
- Be specific and descriptive

**Good:**
```
feat(llm): add Anthropic Claude streaming support
fix(sidebar): resolve message bubble overflow on narrow width
patch(startup): integrate NevoFlux agent initialization
```

**Bad:**
```
feat: new feature  (too vague)
fix(sidebar): Fixed bug  (not imperative, capitalized)
update  (no type/scope, too vague)
```

### Body (Optional)

- Separate from subject with blank line
- Wrap at 72 characters
- Explain **what** and **why**, not **how** (code shows how)
- Use bullet points for multiple changes
- Reference issues and PRs

### Footer (Optional)

**Breaking Changes:**
```
BREAKING CHANGE: remove deprecated LLM provider API

Clients must migrate to new StreamingClient trait.
Migration guide: docs/migration/v2.md
```

**Issue References:**
```
Fixes #123
Closes #456, #789
Relates to #321
```

**Co-authorship:**
```
Co-authored-by: Name <email@example.com>
```

### Special Conventions for NevoFlux

**Patch System Commits:**
```
patch(startup): integrate NevoFlux agent initialization

- Export src/zen/common/modules/ZenStartup.mjs changes as patch
- Add agent initialization call in startup sequence
- Update patch README with new patch file

Patch file: src/nevoflux/patches/common/modules/ZenStartup-mjs.patch
```

**Always include:**
- Mention which patch file was created/modified
- List all affected patch files if multiple

**Rust Agent Commits:**
```
feat(llm): add OpenAI provider support

Implement OpenAI API client in nevoflux-llm crate:
- Add OpenAIClient struct implementing LlmProvider trait
- Support streaming and non-streaming completions
- Add configuration schema for API key and model

Tests: Added integration tests with mock server
Docs: Updated LLM configuration guide
```

**WebExtension Commits:**
```
feat(sidebar): add message editing capability

Allow users to edit their previous messages:
- Add edit button to user message bubbles
- Implement inline editing with textarea
- Auto-save edited messages to conversation history

Accessibility: Full keyboard navigation support
Tested: Light/dark themes, narrow/wide sidebar
```

### Commit Frequency

**Do:**
- Commit logical units of work
- Keep commits focused and atomic
- Write clear commit messages

**Don't:**
- Make "WIP" commits in pull requests
- Mix unrelated changes in one commit
- Commit commented-out code or debug logs
- Make commits like "fix typo" or "oops" (squash them)

### Squashing Commits

Before merging, consider squashing if:
- Multiple commits fix the same issue
- Commits include "fix review comments"
- Commit history is messy with WIP commits

**Keep separate commits if:**
- Refactoring and feature addition
- Multiple independent bug fixes
- Logical progression of related changes

## First-Time Contributors

Welcome! Here's how to get started:

### 1. Find an Issue

- Look for `good-first-issue` or `help-wanted` labels
- Comment on the issue to claim it
- Ask questions if anything is unclear

### 2. Set Up Development Environment

```bash
# Fork the repository on GitHub, then:
git clone https://github.com/YOUR-USERNAME/nevoflux.git
cd nevoflux
git remote add upstream https://github.com/nevoflux/nevoflux.git

# Install dependencies and build
npm install
npm run download
npm run import
npm run bootstrap
npm run build
```

### 3. Create a Branch

```bash
git checkout -b fix/issue-123-description
# or
git checkout -b feat/issue-456-description
```

### 4. Make Changes

- Follow [Code Style](CLAUDE.md#code-style) guidelines
- Write tests for new functionality
- Test your changes thoroughly

### 5. Commit Your Changes

```bash
# For patch system changes (src/zen/)
./scripts/export-nevoflux-patches.sh
./scripts/revert-zen-changes.sh
git add src/nevoflux/
git commit -m "patch(component): fix issue #123"

# For other changes
git add <files>
git commit -m "type(scope): description"
```

### 6. Push and Create PR

```bash
git push origin your-branch-name
```

Then create a pull request on GitHub using the [PR template](#pull-request-template).

### 7. Respond to Reviews

- Address feedback promptly
- Ask for clarification if needed
- Update your PR based on comments

## Reporting Issues

### Before Reporting

- Search existing issues to avoid duplicates
- Verify the issue exists on the latest version
- Collect relevant information (OS, browser version, error messages)

### Issue Template

```markdown
### Description
<!-- Clear description of the issue -->

### Steps to Reproduce
1. Step one
2. Step two
3. Step three

### Expected Behavior
<!-- What should happen -->

### Actual Behavior
<!-- What actually happens -->

### Environment
- OS: [e.g., Ubuntu 22.04, macOS 14.1, Windows 11]
- NevoFlux Version: [e.g., 1.0.0-alpha.1]
- Zen Browser Base Version: [e.g., 1.17.15b]

### Screenshots/Logs
<!-- If applicable, add screenshots or error logs -->

### Additional Context
<!-- Any other relevant information -->
```

## Security Vulnerabilities

**Do NOT open public issues for security vulnerabilities.**

Instead:
1. Email **security@nevoflux.com** with details
2. Include steps to reproduce
3. Allow 90 days for fix before public disclosure
4. We'll acknowledge within 48 hours

For more details, see our [Security Policy](SECURITY.md).

## Code of Conduct

All contributors must follow our Code of Conduct:

### Our Standards

- **Be Respectful**: Treat everyone with respect and professionalism
- **Be Constructive**: Provide helpful feedback and suggestions
- **Be Collaborative**: Work together towards common goals
- **Be Patient**: Everyone is learning and improving
- **Be Inclusive**: Welcome contributors of all backgrounds and skill levels

### Unacceptable Behavior

- Harassment, discrimination, or personal attacks
- Trolling, insulting comments, or political arguments
- Publishing others' private information
- Spam or excessive self-promotion
- Any conduct that would be inappropriate in a professional setting

### Enforcement

Violations can be reported to **conduct@nevoflux.com**. All reports will be reviewed and investigated promptly and fairly.

Consequences may include:
- Warning
- Temporary ban from project participation
- Permanent ban from the project

## License Agreement

By contributing to NevoFlux, you agree that your contributions will be licensed under the **Mozilla Public License 2.0** (MPL 2.0).

All new files must include the MPL 2.0 license header:

**JavaScript/TypeScript:**
```javascript
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
```

**Rust:**
```rust
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
```

**C++:**
```cpp
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
```

## Getting Help

If you need help contributing:

- **Documentation**: Check [CLAUDE.md](CLAUDE.md) and [README.md](README.md)
- **Discussions**: Use [GitHub Discussions](https://github.com/nevoflux/nevoflux/discussions) for questions
- **Chat**: Join our Discord server (link in README)
- **Email**: Contact **maintainers@nevoflux.com**

---

Thank you for contributing to NevoFlux! 🚀
