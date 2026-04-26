---
name: "product-intro-9x16-default"
version: "1.0.0"
description: "Default brand identity for the product-intro-9x16 template — Figma-inspired purple/cyan/green for portrait product reveals."

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
  beat_interval: "1.5s"

voice:
  provider: "kokoro"
  voice_id: "af"
  speed: 1.0

aspect:
  default: "9:16"
  width: 1080
  height: 1920
  safe_zones:
    top: "120px"
    bottom: "220px"
    sides: "60px"
---

## Overview

Portrait product introduction with Figma-inspired triadic palette. Primary leads
the hero scene, secondary supports feature lists, accent highlights the CTA.

## When to override

- `colors.primary` controls the brand color of hero text + feature icons
- `colors.background` should stay dark to preserve type contrast
- Keep aspect 9:16 — template uses portrait safe zones (220px bottom for caption reserve)
