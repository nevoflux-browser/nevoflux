---
name: "video-overlay-default"
version: "1.0.0"
description: "Default brand identity for the video-overlay template — captions and lower-thirds over user-supplied video."

colors:
  primary: "#e63946"
  secondary: "#4a7a7a"
  accent: "#ffcc33"
  background: "#000000"
  foreground: "#ffffff"

typography:
  hero:
    family: "Inter, system-ui, sans-serif"
    weight: 700
  body:
    family: "Inter, system-ui, sans-serif"
    weight: 500

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
  ease_entrance: "power2.out"
  ease_exit: "power2.in"
  scene_duration_default: "3s"
  stagger_default: "0.2s"
  beat_interval: "2s"

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
    sides: "80px"
---

## Overview

Caption / lower-third / watermark overlays over a user-supplied video. The
underlying composition surface is semi-transparent (`rgba(0,0,0,0.55)` for the
overlay layer); foreground stays white for caption legibility regardless of the
source video's brightness.

## When to override

- `colors.primary` paints lower-third accent bars + caption-box backgrounds
- `colors.foreground` MUST stay light (`#fff` or near-white) for text contrast
  over arbitrary video frames
- Match `aspect` to the source video — caption-bottom calculations depend on it
