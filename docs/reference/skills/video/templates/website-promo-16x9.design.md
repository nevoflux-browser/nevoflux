---
name: "website-promo-16x9-default"
version: "1.0.0"
description: "Default brand identity for the website-promo-16x9 template — Mode 3 website-to-video with screenshot reveals."

colors:
  primary: "#1a6b6b"
  secondary: "#4a7a7a"
  accent: "#ffcc33"
  background: "#0b1020"
  foreground: "#f3f6f8"

typography:
  hero:
    family: "Inter, 'PingFang SC', system-ui, sans-serif"
    weight: 700
  body:
    family: "Inter, 'PingFang SC', system-ui, sans-serif"
    weight: 400

spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
  xl: "48px"

rounded:
  sm: "4px"
  md: "8px"
  lg: "16px"

motion:
  ease_default: "power2.inOut"
  ease_entrance: "back.out(1.4)"
  ease_exit: "power2.in"
  scene_duration_default: "4s"
  stagger_default: "0.25s"
  beat_interval: "3s"

voice:
  provider: "kokoro"
  voice_id: "af"
  speed: 1.0

aspect:
  default: "16:9"
  width: 1920
  height: 1080
  safe_zones:
    top: "80px"
    bottom: "120px"
    sides: "100px"
---

## Overview

Website-to-video promo layout. Default palette mirrors a clean teal/yellow
corporate scheme (`#1a6b6b` primary, `#ffcc33` accent for CTAs). Mode 3 flow
auto-fills this from `extract_visual_identity` — defaults apply only when no
brand data is supplied.

## When to override

- `colors.primary` is the headline + section-divider color
- `colors.accent` is the single-CTA color (one accent per scene per design.md spec)
- For dark-bg sites swap `colors.background` to a darker hex; keep foreground
  contrast above WCAG AA
