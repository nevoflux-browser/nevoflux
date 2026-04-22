---
name: video
description: Author video compositions (HTML) and render them to MP4. Use when the user wants a short promo video, social-media clip, product intro, or website-to-video walkthrough.
version: 0.1.0
enabled: true
tags:
  - video
  - mp4
  - composition
  - canvas
triggers:
  - /video
  - video
  - 视频
  - composition
  - promo
  - MP4
---

# Video skill (P3 scaffold)

Short stub. Full prose, templates, components, and snippets land in
Week 6-7 / Week 7-8 of the umbrella /video spec.

Tools exposed at this phase:
- `canvas_create_composition` — create a composition artifact.
- `canvas_render_video` — non-blocking render to MP4; sidebar shows progress.
- `canvas_lint_composition` — run the composition linter against an
  existing composition; returns structured issues.
