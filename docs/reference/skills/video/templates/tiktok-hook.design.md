---
name: "tiktok-hook-default"
version: "1.0.0"
description: "Default brand identity for the tiktok-hook template — high-saturation 3-scene rotation."

colors:
  primary: "#e63946"
  secondary: "#023e8a"
  accent: "#f4a261"
  background: "#000000"
  foreground: "#ffffff"

typography:
  hero:
    family: "Inter, 'PingFang SC', -apple-system, sans-serif"
    weight: 900
  body:
    family: "Inter, 'PingFang SC', -apple-system, sans-serif"
    weight: 500

spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "64px"
  xl: "96px"

rounded:
  sm: "4px"
  md: "8px"
  lg: "16px"

motion:
  ease_default: "power2.out"
  ease_entrance: "power4.out"
  ease_exit: "power2.in"
  scene_duration_default: "2s"
  stagger_default: "0.15s"
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
    bottom: "200px"
    sides: "48px"
---

## Overview

Hard-cut 3-scene hook for TikTok / Reels. High-saturation primary/secondary/accent
rotate per scene; foreground stays `#ffffff` for maximum pop on each background.

## When to override

- Replace `colors.primary` for the opening hook color (1.0–1.5s scene 1)
- Replace `colors.secondary` for the supporting beat color (scene 2)
- Replace `colors.accent` for the CTA color (scene 3)
- Keep `typography.hero.weight: 900` — TikTok-style hooks need heavy display weight
- Keep `motion.ease_entrance: power4.out` — hard punch landing
