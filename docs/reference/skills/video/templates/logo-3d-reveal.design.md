---
name: "logo-3d-reveal-default"
version: "1.0.0"
description: "Default brand identity for the logo-3d-reveal template — single-color logo intro on dark canvas."

colors:
  primary: "#1a6b6b"
  secondary: "#4a7a7a"
  accent: "#ff8c42"
  background: "#000000"
  foreground: "#f5f5f7"

typography:
  hero:
    family: "Inter, 'PingFang SC', -apple-system, sans-serif"
    weight: 700
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
  ease_entrance: "power3.inOut"
  ease_exit: "power2.in"
  scene_duration_default: "3s"
  stagger_default: "0.2s"
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
    bottom: "80px"
    sides: "120px"
---

## Overview

3D logo reveal on solid `#000000` canvas. Cinematic easing (`power3.inOut`) for
the camera move. Primary color paints the logo material; foreground for any
attached tagline text.

## When to override

- `colors.primary` is the logo material color — use the brand's exact hex
- Keep `colors.background` dark; lighter backgrounds wash out the 3D shading
- Keep `motion.ease_entrance: power3.inOut` for cinematic sweep
