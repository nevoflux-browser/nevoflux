---
name: "product-intro-16x9-default"
version: "1.0.0"
description: "Default brand identity for the product-intro-16x9 template — Figma-inspired purple/cyan/green for landscape product reveals."

colors:
  primary: "#a259ff"
  secondary: "#1abcfe"
  accent: "#0acf83"
  background: "#0a0a0f"
  foreground: "#ffffff"

typography:
  hero:
    family: "Inter, 'PingFang SC', -apple-system, sans-serif"
    weight: 800
  body:
    family: "Inter, 'PingFang SC', -apple-system, sans-serif"
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
  ease_default: "power2.out"
  ease_entrance: "back.out(1.7)"
  ease_exit: "power2.in"
  scene_duration_default: "5s"
  stagger_default: "0.3s"
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

Landscape product introduction sharing the 9:16 sibling's triadic palette.
Wider canvas → larger sides margin (100px) and tighter top/bottom for
YouTube/LinkedIn safe-zone discipline.

## When to override

- `colors.primary` is the hero label color
- Keep `aspect.default: 16:9` — template layout assumes landscape grid
- For corporate/B2B beats, raise `motion.beat_interval` to 3-5s
