---
name: layout-before-animation
purpose: Discipline for computing positioning/sizing BEFORE introducing animations so GSAP tweens operate on a stable layout
tags: [layout, animation, gsap, discipline]
---

# 先布局后动画 / Layout Before Animation

## 核心原则 / Core Principle

Every element must first be positioned to its "fullest" frame—the moment when it is completely in-place, fully visible, and not yet beginning its exit. Write this position using pure HTML and CSS, without GSAP yet.

This is a hard rule because if you directly write "element tweens in from the left offscreen" in GSAP, you are *guessing* the final position. Two overlapping final positions, text overflowing the canvas, mismatched font sizes—all these problems remain invisible until you render video. **Get the static hero frame correct first; animation is just the path to reach it.**

## 检查清单 / Checklist

- **DO**: compute final positions and sizes in CSS first (hero frame), then apply `opacity: 0` / `transform: scale(0)` / `transform: translateY(N)` as animation entry points
- **DO**: use `.scene-content` containers with `width: 100%`, `height: 100%`, `padding`, `display: flex`, `gap`, and `box-sizing: border-box` for layout
- **DO**: use `gsap.from()` to animate elements *into* their CSS-defined resting positions
- **DO**: rely on clip adapter's data-driven visibility via `data-start + data-duration` (no manual opacity tweens)
- **DON'T**: animate `width` / `height` / `flex-basis` directly—layout-shift cascades and timing becomes nondeterministic
- **DON'T**: use `position: absolute; top: Npx` for main content containers—content overflows when layout changes
- **DON'T**: mix CSS transitions with GSAP tweens on the same property
- **DON'T**: hardcode element dimensions; use flex containers and padding for responsive positioning

## 示例 / Examples

**Correct pattern:**

```css
/* Hero frame: static layout */
.scene-content {
  display: flex;
  flex-direction: column;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: 120px 160px;
  gap: 24px;
  box-sizing: border-box;
}
.title    { font-size: 120px; }
.subtitle { font-size: 42px; }
```

```javascript
// Animation enters from offscreen → CSS-defined resting position
tl.from('.title',    { y: 60, opacity: 0, duration: 0.6, ease: 'power3.out' }, 0.15);
tl.from('.subtitle', { y: 40, opacity: 0, duration: 0.5, ease: 'power3.out' }, 0.35);
```

## 反模式 / Anti-patterns

- **Hardcoded absolute positioning**: `position: absolute; top: 200px; left: 160px; width: 1920px;` — breaks when you change canvas dimensions
- **Animating layout properties**: `gsap.to('.box', { width: 400, height: 300 })` — causes layout recalc cascade and timing skew
- **No static hero frame**: attempting to tween elements into position without first laying them out in CSS — ensures collision and overflow bugs that only appear in final render
- **Manual opacity exit in main scene**: `tl.to('#scene-1', { opacity: 0, duration: 0.4 })` at 3.1s — creates lag between fade and transition; clip adapter manages this automatically
- **Mixed CSS transition + GSAP tween**: `<style> .box { transition: opacity 0.2s; } </style>` + `tl.to('.box', { opacity: 1 })` — both animate opacity simultaneously, causing jitter

## Enforced by

- `comp/timed-element-missing-clip-class` — elements with `data-start` and `data-duration` must have `class="clip"` so the runtime controls visibility (not manual GSAP)
- `comp/gsap-animates-clip-element` — GSAP must not animate `visibility` or `display` on clip elements; the clip adapter manages these
- `comp/overlapping-gsap-tweens` — tweens on the same selector animating the same property without `overwrite: "auto"` will conflict during timeline playback
